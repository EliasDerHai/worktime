use crate::{
    db::{
        time_off::{TimeOffEntry, TimeOffId, TimeOffKind},
        worktime_session::{WorktimeSession, WorktimeSessionId},
    },
    err::CommandResult,
    time::Clock,
};
use chrono::{NaiveDate, NaiveDateTime};
use sqlx::{Error, SqlitePool};

pub mod time_off;
pub mod worktime_session;

type Result<T> = sqlx::Result<T>;

pub struct WorktimeDatabase {
    pool: SqlitePool,
}

impl WorktimeDatabase {
    pub fn new(pool: SqlitePool) -> Self {
        let p2: SqlitePool = pool.clone();
        tokio::spawn(async move {
            let _ = sanity_check(p2).await;
        });
        Self { pool }
    }

    pub async fn get_last_session(&self) -> Result<Option<WorktimeSession>> {
        let last = sqlx::query!("
            SELECT id, start_time as \"start_time: NaiveDateTime\", end_time as \"end_time: NaiveDateTime\"  
            FROM work_sessions 
            ORDER BY id desc 
            LIMIT 1
        ")
        .fetch_one(&self.pool)
        .await;

        match last {
            Ok(last) => Ok(Some(WorktimeSession::from((
                last.id,
                last.start_time,
                last.end_time,
            )))),
            Err(sqlx::Error::RowNotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn get_last_n_sessions_desc(&self, n: u32) -> Result<Vec<WorktimeSession>> {
        let last = sqlx::query!("
               SELECT id, start_time as \"start_time: NaiveDateTime\", end_time as \"end_time: NaiveDateTime\"  
               FROM work_sessions 
               ORDER BY id desc 
               LIMIT $1
           ", n)
        .fetch_all(&self.pool)
        .await;

        last.map(|rows| {
            rows.iter()
                .map(|r| WorktimeSession::from((r.id, r.start_time, r.end_time)))
                .collect()
        })
    }

    pub async fn get_nth_last_session(&self, n: u32) -> Result<WorktimeSession> {
        let last = sqlx::query!(r#"
               SELECT id, start_time as "start_time: NaiveDateTime", end_time as "end_time: NaiveDateTime"  
               FROM work_sessions 
               ORDER BY id desc 
               LIMIT 1
               OFFSET $1
           "#, 
            n
        )
        .fetch_optional(&self.pool)
        .await;

        last.and_then(|r_opt| {
            match r_opt.map(|r| WorktimeSession::from((r.id, r.start_time, r.end_time))) {
                Some(worktime) => Ok(worktime),
                None => Err(sqlx::Error::RowNotFound),
            }
        })
    }

    pub async fn get_sessions_since(&self, day: NaiveDate) -> Result<Vec<WorktimeSession>> {
        let r = sqlx::query!(
            r#"
                SELECT id, start_time as "start_time: NaiveDateTime", end_time as "end_time: NaiveDateTime"  
                FROM work_sessions 
                WHERE date(start_time) >= date($1)
                ORDER BY id asc
            "#,
            day
        ).fetch_all(&self.pool).await;

        r.map(|rows| {
            rows.iter()
                .map(|r| WorktimeSession::from((r.id, r.start_time, r.end_time)))
                .collect()
        })
    }

    pub async fn insert_start(&self, clock: &impl Clock) -> CommandResult<NaiveDateTime> {
        let c = sqlx::query!(
            r#"
                SELECT count(*) as open_sessions
                FROM work_sessions 
                WHERE end_time IS NULL
           "#
        )
        .fetch_one(&self.pool)
        .await?
        .open_sessions;

        match c {
            0 => (),
            1 => return Err("Session already started".into()),
            n => panic!("Corrupt data - {n} sessions running!"),
        }

        let now = clock.get_now();
        sqlx::query!("INSERT INTO work_sessions (start_time) VALUES ($1)", now)
            .execute(&self.pool)
            .await?;
        Ok(now)
    }

    #[allow(dead_code)]
    pub async fn get_session_by_id(&self, id: WorktimeSessionId) -> Result<WorktimeSession> {
        let r = sqlx::query!(r#"
                SELECT id, start_time as "start_time: NaiveDateTime", end_time as "end_time: NaiveDateTime"  
                FROM work_sessions 
                WHERE id = $1
            "#,
            id.0
        )
            .fetch_one(&self.pool)
            .await;

        r.map(|row| WorktimeSession::from((row.id, row.start_time, row.end_time)))
    }

    pub async fn insert_stop(
        &self,
        id: WorktimeSessionId,
        clock: &impl Clock,
    ) -> Result<NaiveDateTime> {
        let now = clock.get_now();
        self.update_end_time(id, &now).await?;
        Ok(now)
    }

    pub async fn update_start_time(
        &self,
        id: WorktimeSessionId,
        date_time: &NaiveDateTime,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE work_sessions
            SET start_time = $1
            WHERE id = $2
            "#,
            date_time,
            id.0
        )
        .execute(&self.pool)
        .await
        .and_then(result_from_rows_affected)
    }

    pub async fn update_end_time(
        &self,
        id: WorktimeSessionId,
        date_time: &NaiveDateTime,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE work_sessions
            SET end_time = $1
            WHERE id = $2
            "#,
            date_time,
            id.0
        )
        .execute(&self.pool)
        .await
        .and_then(result_from_rows_affected)
    }

    pub async fn insert_time_off(&self, date: NaiveDate, kind: TimeOffKind) -> Result<TimeOffId> {
        let result = sqlx::query!(
            r#"
            INSERT INTO time_off (date, kind) VALUES ($1, $2)
            "#,
            date,
            kind
        )
        .execute(&self.pool)
        .await?;

        Ok(TimeOffId::from(result.last_insert_rowid()))
    }

    pub async fn get_time_off_by_date(&self, date: NaiveDate) -> Result<Option<TimeOffEntry>> {
        let result = sqlx::query!(
            r#"
            SELECT id, date as "date: NaiveDate", kind as "kind: TimeOffKind"
            FROM time_off 
            WHERE date = $1
            "#,
            date
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|row| TimeOffEntry::from((row.id, row.date, row.kind))))
    }

    pub async fn get_all_time_off(&self) -> Result<Vec<TimeOffEntry>> {
        let results = sqlx::query!(
            r#"
            SELECT id, date as "date: NaiveDate", kind as "kind: TimeOffKind"
            FROM time_off 
            ORDER BY date ASC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(results
            .into_iter()
            .map(|row| TimeOffEntry::from((row.id, row.date, row.kind)))
            .collect())
    }

    pub async fn delete_time_off(&self, id: TimeOffId) -> Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM time_off
            WHERE id = $1
            "#,
            id.0
        )
        .execute(&self.pool)
        .await
        .and_then(result_from_rows_affected)
    }
}

// ####################
// UTILS
// ####################

fn result_from_rows_affected(
    query_result: sqlx::sqlite::SqliteQueryResult,
) -> std::result::Result<(), Error> {
    if query_result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(sqlx::Error::RowNotFound)
    }
}

// ####################
// CHECKS
// ####################
async fn sanity_check(pool: SqlitePool) -> Result<()> {
    let open_sessions = sqlx::query!(
        "
        SELECT count(*) as open_sessions
        FROM work_sessions 
        WHERE end_time IS NULL
        "
    )
    .fetch_one(&pool)
    .await?
    .open_sessions;

    match open_sessions {
        0 | 1 => (),
        n => panic!("Corrupt data - {n} sessions running!"),
    }

    let mut all_sessions: Vec<WorktimeSession> = sqlx::query!("
        SELECT id, start_time as \"start_time: NaiveDateTime\", end_time as \"end_time: NaiveDateTime\"  
        FROM work_sessions 
    ")
        .fetch_all(&pool)
        .await?
        .iter()
        .map(|r| WorktimeSession::from((r.id, r.start_time, r.end_time)))
        .collect();

    if !all_sessions.is_sorted_by_key(|s| s.start) {
        all_sessions.sort_by_key(|s| s.start);
    };

    all_sessions.into_iter().fold(
        None,
        |last_end, WorktimeSession { id, start, end }| {
            if let Some(end) = end {
                assert!(
                    end >= start,
                    "Corrupt data - Session '{id}' end {end:?} before start {start:?}"
                );
            }
            if let Some(last_end) = last_end{
                assert!(
                    start >= last_end,
                    "Corrupt data - Session '{id}' overlap prev. end {last_end:?} after next start {start:?}"
                );
            }

            end
        },
    );

    Ok(())
}

#[cfg(test)]
pub async fn get_test_worktime_db() -> Result<WorktimeDatabase> {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let opts = SqliteConnectOptions::new().in_memory(true);
    let pool = SqlitePoolOptions::new()
        // NOTE:
        // every in-memory db connection is it's own isolated 'database'
        // see: https://www.sqlite.org/inmemorydb.html
        // this means that in order to have the migrations available for the whole pool
        // we have to limit the connections to 1. Any other connection wouldn't have the
        // migrations!
        // see: https://github.com/launchbadge/sqlx/issues/362#issuecomment-636661146
        .max_connections(1)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(WorktimeDatabase::new(pool))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::test_utils::MockClock;

    #[tokio::test]
    async fn test_dbs_should_be_isolated() -> Result<()> {
        let clock = MockClock::default();
        let db1 = get_test_worktime_db().await?;
        let db2 = get_test_worktime_db().await?;

        db1.insert_start(&clock).await.unwrap();
        let last_1 = db1.get_last_session().await?;
        let last_2 = db2.get_last_session().await?;

        assert!(last_1.is_some());
        assert!(last_2.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_get_last() -> sqlx::Result<()> {
        let clock = MockClock::default();
        let db = get_test_worktime_db().await?;

        clock.set(4, 8, 0);
        db.insert_start(&clock).await.unwrap();
        clock.set(4, 12, 0);
        let id_1 = db.get_last_session().await.unwrap().unwrap().id;
        db.insert_stop(id_1, &clock).await?;

        clock.set(4, 13, 0);
        db.insert_start(&clock).await.unwrap();
        clock.set(4, 17, 0);
        let id_2 = db.get_last_session().await.unwrap().unwrap().id;
        db.insert_stop(id_2, &clock).await?;

        clock.set(5, 8, 30);
        db.insert_start(&clock).await.unwrap();
        clock.set(5, 12, 0);
        let id_3 = db.get_last_session().await.unwrap().unwrap().id;
        db.insert_stop(id_3, &clock).await?;

        assert_eq!(id_3, db.get_nth_last_session(0).await?.id);
        assert_eq!(id_2, db.get_nth_last_session(1).await?.id);
        assert_eq!(
            vec![id_3, id_2],
            db.get_last_n_sessions_desc(2)
                .await?
                .iter()
                .map(|s| s.id)
                .collect::<Vec<WorktimeSessionId>>()
        );

        Ok(())
    }
}
