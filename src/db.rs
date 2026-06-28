use crate::{
    db::{
        config::Config,
        time_off::{TimeOffEntry, TimeOffId, TimeOffKind},
        worktime_session::{WorktimeSession, WorktimeSessionId},
    },
    err::CommandResult,
    time::Clock,
};
use chrono::{Datelike, Days, NaiveDate, NaiveDateTime, TimeDelta, Timelike, Weekday};
use sqlx::{Error, SqlitePool};
use std::{
    collections::{BTreeSet, HashSet},
    ops::Bound,
};

pub mod config;
pub mod time_off;
pub mod worktime_session;

type Result<T> = sqlx::Result<T>;

pub struct WorktimeDatabase {
    pool: SqlitePool,
}

impl WorktimeDatabase {
    pub async fn new(pool: SqlitePool, clock: &impl Clock) -> Result<Self> {
        clamp_overlong_sessions(&pool, clock).await?;

        let p2: SqlitePool = pool.clone();
        tokio::spawn(async move {
            let _ = sanity_check(p2).await;
        });

        Ok(Self { pool })
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

    pub async fn get_all_vacations(&self) -> Result<Vec<TimeOffEntry>> {
        let results = sqlx::query!(
            r#"
            SELECT id, date as "date: NaiveDate", kind as "kind: TimeOffKind", label
            FROM time_off
            WHERE kind = 'vacation'
            ORDER BY date ASC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(results
            .into_iter()
            .map(|row| TimeOffEntry::from((row.id, row.date, row.kind, row.label)))
            .collect())
    }

    pub async fn get_time_off_between_dates(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<TimeOffEntry>> {
        let results = sqlx::query!(
            r#"
            SELECT id, date as "date: NaiveDate", kind as "kind: TimeOffKind", label
            FROM time_off
            WHERE date >= $1 AND date <= $2
            ORDER BY date ASC
            "#,
            from,
            to
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(results
            .into_iter()
            .map(|row| TimeOffEntry::from((row.id, row.date, row.kind, row.label)))
            .collect())
    }

    pub async fn get_config(&self) -> Result<Config> {
        read_config(&self.pool).await
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

    pub async fn delete_holidays_for_year(&self, year: i32) -> Result<u64> {
        let year_start = NaiveDate::from_ymd_opt(year, 1, 1).expect("valid year");
        let year_end = NaiveDate::from_ymd_opt(year + 1, 1, 1).expect("valid year");
        let result = sqlx::query!(
            r#"
            DELETE FROM time_off
            WHERE kind = 'holiday'
            AND date >= $1 AND date < $2
            "#,
            year_start,
            year_end
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Removes whole vacation **blocks**: for each selected day, expands to the
    /// maximal run of vacation days it belongs to (gaps of only weekends and/or
    /// public holidays are bridged) and deletes the entire run, including days
    /// that weren't individually selected. Returns the number of rows removed.
    ///
    /// Because a block is maximal, every vacation row inside its `[from, to]`
    /// span belongs to it — so each block is removed with a single static,
    /// compile-time-checked range delete (no dynamic `WHERE id IN (..)`).
    pub async fn delete_vacation_block(&self, ids: &[TimeOffId]) -> Result<u64> {
        let vacations = self.get_all_vacations().await?;
        if vacations.is_empty() || ids.is_empty() {
            return Ok(0);
        }

        let vac_dates: BTreeSet<NaiveDate> = vacations.iter().map(|v| v.date).collect();
        let span_from = *vac_dates.iter().next().expect("non-empty");
        let span_to = *vac_dates.iter().next_back().expect("non-empty");

        // holidays in the vacation span can bridge gaps between vacation days
        let holidays: HashSet<NaiveDate> = self
            .get_time_off_between_dates(span_from, span_to)
            .await?
            .into_iter()
            .filter(|e| e.kind == TimeOffKind::Holiday)
            .map(|e| e.date)
            .collect();

        // dates of the picked entries → their containing block bounds (deduped)
        let mut blocks: Vec<(NaiveDate, NaiveDate)> = vacations
            .iter()
            .filter(|v| ids.contains(&v.id))
            .map(|v| vacation_block_bounds(&vac_dates, &holidays, v.date))
            .collect();
        blocks.sort();
        blocks.dedup();

        let mut removed = 0u64;
        for (from, to) in blocks {
            let result = sqlx::query!(
                r#"
                DELETE FROM time_off
                WHERE kind = 'vacation'
                AND date >= $1 AND date <= $2
                "#,
                from,
                to
            )
            .execute(&self.pool)
            .await?;
            removed += result.rows_affected();
        }
        Ok(removed)
    }

    pub async fn insert_time_off_or_ignore(
        &self,
        date: NaiveDate,
        kind: TimeOffKind,
        label: Option<&str>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT OR IGNORE INTO time_off (date, kind, label) VALUES ($1, $2, $3)
            "#,
            date,
            kind,
            label
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_all_sessions(&self) -> Result<Vec<WorktimeSession>> {
        sqlx::query!(
            r#"
            SELECT id, start_time as "start_time: NaiveDateTime", end_time as "end_time: NaiveDateTime"
            FROM work_sessions
            ORDER BY id asc
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map(|rows| {
            rows.iter()
                .map(|r| WorktimeSession::from((r.id, r.start_time, r.end_time)))
                .collect()
        })
    }

    pub async fn remove_sessions_by_day(&self, d: NaiveDate) -> Result<()> {
        let day_start = NaiveDateTime::from(d);
        let day_end = NaiveDateTime::from(d)
            .checked_add_days(Days::new(1))
            .unwrap();
        sqlx::query!(
            r#"
            DELETE FROM work_sessions
            WHERE start_time >= $1 AND start_time < $2
            "#,
            day_start,
            day_end
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_session(&self, d: NaiveDate, hours_per_day: u32) -> CommandResult<()> {
        let starting_hour = 8;
        let day_start = NaiveDateTime::from(d)
            .with_hour(starting_hour)
            .ok_or("Worktime start should be < 24")?;
        let day_end = NaiveDateTime::from(d)
            .with_hour(starting_hour + hours_per_day)
            .ok_or("Worktime end should be < 24 (default start + specified hours)")?;

        sqlx::query!(
            r#"
            INSERT INTO work_sessions (start_time, end_time)
            VALUES ($1, $2)
            "#,
            day_start,
            day_end
        )
        .execute(&self.pool)
        .await?;
        Ok(())
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

/// Expands `selected` to the inclusive `[from, to]` span of the contiguous
/// vacation block it belongs to. Two vacation days are in the same block when
/// every calendar day strictly between them is not a working day (holiday, weekend, vacations).
fn vacation_block_bounds(
    vacations: &BTreeSet<NaiveDate>,
    holidays: &HashSet<NaiveDate>,
    selected: NaiveDate,
) -> (NaiveDate, NaiveDate) {
    let is_holiday_or_weekend =
        |d: NaiveDate| matches!(d.weekday(), Weekday::Sat | Weekday::Sun) || holidays.contains(&d);

    let gap_is_bridgeable = |a: NaiveDate, b: NaiveDate| {
        let mut d = a.succ_opt().expect("date in range");
        while d < b {
            if !is_holiday_or_weekend(d) {
                return false;
            }
            d = d.succ_opt().expect("date in range");
        }
        true
    };

    let mut from = selected;
    while let Some(&prev) = vacations.range(..from).next_back() {
        if gap_is_bridgeable(prev, from) {
            from = prev;
        } else {
            break;
        }
    }

    let mut to = selected;
    while let Some(&next) = vacations
        .range((Bound::Excluded(to), Bound::Unbounded))
        .next()
    {
        if gap_is_bridgeable(to, next) {
            to = next;
        } else {
            break;
        }
    }

    (from, to)
}

// ####################
// CONFIG
// ####################

pub(crate) async fn read_config(pool: &SqlitePool) -> Result<Config> {
    let row = sqlx::query!(
        r#"
        SELECT hours_per_holiday, expected_weekly_hours, max_session_hours
        FROM config
        LIMIT 1
        "#
    )
    .fetch_one(pool)
    .await?;

    Ok(Config {
        hours_per_holiday: row.hours_per_holiday,
        expected_weekly_hours: row.expected_weekly_hours,
        max_session_hours: row.max_session_hours,
    })
}

// ####################
// CLEANUP
// ####################

async fn clamp_overlong_sessions(pool: &SqlitePool, clock: &impl Clock) -> Result<()> {
    let max_hours = read_config(pool).await?.max_session_hours;
    let now = clock.get_now();

    // truncate completed over-long sessions
    sqlx::query!(
        r#"
        UPDATE work_sessions
        SET end_time = datetime(start_time, '+' || $1 || ' hours')
        WHERE end_time IS NOT NULL
          AND end_time > datetime(start_time, '+' || $1 || ' hours')
        "#,
        max_hours
    )
    .execute(pool)
    .await?;

    // close a stale open session using the Clock-provided `now`
    let cap = TimeDelta::hours(max_hours);
    let open = sqlx::query!(
        r#"
        SELECT id, start_time as "start_time: NaiveDateTime"
        FROM work_sessions
        WHERE end_time IS NULL
        ORDER BY id DESC LIMIT 1
        "#
    )
    .fetch_optional(pool)
    .await?;

    if let Some(row) = open {
        let capped_end = row.start_time + cap;
        if now > capped_end {
            sqlx::query!(
                "UPDATE work_sessions SET end_time = $1 WHERE id = $2",
                capped_end,
                row.id
            )
            .execute(pool)
            .await?;
        }
    }

    Ok(())
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
    // default MockClock time (epoch) means startup clamping is a no-op for the
    // 2025-dated sessions tests insert; tests exercise clamping directly instead.
    let clock = crate::time::test_utils::MockClock::default();
    WorktimeDatabase::new(pool, &clock).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::test_utils::MockClock;

    // July 2025: 7=Mon .. 11=Fri, 12/13=Sat/Sun, 14=Mon .. 18=Fri
    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 7, day).unwrap()
    }

    #[test]
    fn block_bridges_weekend() {
        let vacations: BTreeSet<NaiveDate> = [7, 8, 9, 10, 11, 14, 15, 16, 17, 18]
            .map(d)
            .into_iter()
            .collect();
        let holidays = HashSet::new();
        // picking any day in the two-week run yields the full span
        assert_eq!(
            vacation_block_bounds(&vacations, &holidays, d(14)),
            (d(7), d(18))
        );
        assert_eq!(
            vacation_block_bounds(&vacations, &holidays, d(7)),
            (d(7), d(18))
        );
    }

    #[test]
    fn block_bridges_midweek_holiday() {
        // Mon 14 is a holiday (no vacation row), so the two runs stay one block
        let vacations: BTreeSet<NaiveDate> = [7, 8, 9, 10, 11, 15, 16, 17, 18]
            .map(d)
            .into_iter()
            .collect();
        let holidays: HashSet<NaiveDate> = [d(14)].into_iter().collect();
        assert_eq!(
            vacation_block_bounds(&vacations, &holidays, d(8)),
            (d(7), d(18))
        );
    }

    #[test]
    fn working_day_gap_splits_blocks() {
        // gap 14(Mon)/15(Tue) are plain working days → two separate blocks
        let vacations: BTreeSet<NaiveDate> =
            [7, 8, 9, 10, 11, 16, 17, 18].map(d).into_iter().collect();
        let holidays = HashSet::new();
        assert_eq!(
            vacation_block_bounds(&vacations, &holidays, d(9)),
            (d(7), d(11))
        );
        assert_eq!(
            vacation_block_bounds(&vacations, &holidays, d(16)),
            (d(16), d(18))
        );
    }

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

    #[tokio::test]
    async fn clamp_closes_stale_open_session() -> Result<()> {
        let clock = MockClock::default();
        let db = get_test_worktime_db().await?;

        clock.set(4, 8, 0);
        db.insert_start(&clock).await.unwrap();

        clock.add_hours(13);
        clamp_overlong_sessions(&db.pool, &clock).await?;

        let session = db.get_last_session().await?.unwrap();
        assert_eq!(session.end, Some(clock.get(4, 20, 0)));

        Ok(())
    }

    #[tokio::test]
    async fn clamp_truncates_overlong_completed_session() -> Result<()> {
        let clock = MockClock::default();
        let db = get_test_worktime_db().await?;

        clock.set(4, 6, 0);
        db.insert_start(&clock).await.unwrap();
        let id = db.get_last_session().await?.unwrap().id;
        clock.set(4, 20, 0); // 14h session
        db.insert_stop(id, &clock).await?;

        // truncation of completed sessions is independent of "now"
        clamp_overlong_sessions(&db.pool, &clock).await?;

        let session = db.get_session_by_id(id).await?;
        assert_eq!(session.end, Some(clock.get(4, 18, 0))); // 06:00 + 12h

        Ok(())
    }

    #[tokio::test]
    async fn clamp_leaves_open_session_within_cap() -> Result<()> {
        let clock = MockClock::default();
        let db = get_test_worktime_db().await?;

        clock.set(4, 9, 0);
        db.insert_start(&clock).await.unwrap();

        // only 3h elapsed, still under the 12h cap
        clock.add_hours(3);
        clamp_overlong_sessions(&db.pool, &clock).await?;

        let session = db.get_last_session().await?.unwrap();
        assert_eq!(session.end, None);

        Ok(())
    }
}
