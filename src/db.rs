use crate::{
    err::CommandResult,
    time::{Clock, display_time},
};
use chrono::{NaiveDate, NaiveDateTime};
use sqlx::{Error, SqlitePool};
use std::fmt::Display;

type Result<T> = sqlx::Result<T>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorktimeSessionId(u32);

impl Display for WorktimeSessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl From<i64> for WorktimeSessionId {
    fn from(value: i64) -> Self {
        WorktimeSessionId(u32::try_from(value).unwrap())
    }
}

impl From<u32> for WorktimeSessionId {
    fn from(value: u32) -> Self {
        WorktimeSessionId(value)
    }
}

impl From<WorktimeSessionId> for u32 {
    fn from(value: WorktimeSessionId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone)]
pub struct WorktimeSession {
    pub id: WorktimeSessionId,
    pub start: NaiveDateTime,
    pub end: Option<NaiveDateTime>,
}

impl Display for WorktimeSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = &self.id;
        let start = display_time(&self.start);
        let end = &self
            .end
            .map(|t| display_time(&t).to_string())
            .unwrap_or("-".to_string());
        write!(f, "id: {id};start: {start};end: {end}")
    }
}

impl From<(i64, NaiveDateTime, Option<NaiveDateTime>)> for WorktimeSession {
    fn from((id, start, end): (i64, NaiveDateTime, Option<NaiveDateTime>)) -> Self {
        let id = WorktimeSessionId::from(id);
        Self { id, start, end }
    }
}

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

    pub async fn get_last_n_sessions(&self, n: u32) -> Result<Vec<WorktimeSession>> {
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
}
