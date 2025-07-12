use std::fmt::Display;

use chrono::{Local, NaiveDateTime};
use sqlx::{Error, SqlitePool};

use crate::{
    err::CommandResult,
    time::{get_now, get_utc_zero},
};

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
        let v = u32::try_from(value).unwrap();
        WorktimeSessionId(v)
    }
}

#[derive(Debug)]
pub struct WorktimeSession {
    pub id: WorktimeSessionId,
    pub start: NaiveDateTime,
    pub end: Option<NaiveDateTime>,
}

pub struct WorktimeDatabase {
    pool: SqlitePool,
}

impl WorktimeDatabase {
    pub fn new(pool: SqlitePool) -> Self {
        let p2: SqlitePool = pool.clone();
        tokio::spawn(async {
            let _ = sanity_check(p2).await;
        });
        println!("init");
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
            Ok(last) => Ok(Some(WorktimeSession {
                id: last.id.into(),
                start: last.start_time,
                end: last.end_time,
            })),
            Err(sqlx::Error::RowNotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn get_todays_sessions(&self) -> Result<Vec<WorktimeSession>> {
        let today = Local::now().naive_local().date();
        let sessions = sqlx::query!("
        SELECT id, start_time as \"start_time: NaiveDateTime\", end_time as \"end_time: NaiveDateTime\"  
        FROM work_sessions 
        WHERE date(start_time) = date($1)
        ORDER BY id asc
    ", today)
        .fetch_all(&self.pool)
        .await?
        .iter()
    .map(|r| WorktimeSession {
            id: r.id.into(),
            start: r.start_time,
            end: r.end_time,
        })
            .collect();

        Ok(sessions)
    }

    pub async fn insert_start(&self) -> CommandResult<NaiveDateTime> {
        let c = sqlx::query!(
            "
        SELECT count(*) as open_sessions
        FROM work_sessions 
        WHERE end_time IS NULL
        "
        )
        .fetch_one(&self.pool)
        .await?
        .open_sessions;

        match c {
            0 => (),
            1 => return Err("Session already started".into()),
            n => panic!("Corrupt data - {n} sessions running!"),
        }

        let now = Local::now().naive_local();
        sqlx::query!("INSERT INTO work_sessions (start_time) VALUES ($1)", now)
            .execute(&self.pool)
            .await?;
        Ok(now)
    }

    pub async fn insert_stop(&self, id: WorktimeSessionId) -> Result<NaiveDateTime> {
        let now = Local::now().naive_local();
        let updated = sqlx::query!(
            r#"
            UPDATE work_sessions
            SET end_time = $1
            WHERE id = $2
            "#,
            now,
            id.0
        )
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 1 {
            Ok(now)
        } else {
            Err(Error::RowNotFound)
        }
    }
}

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
        .map(|r| WorktimeSession {
            id: r.id.into(),
            start: r.start_time,
            end: r.end_time,
        })
        .collect();

    if !all_sessions.is_sorted_by_key(|s| s.start) {
        all_sessions.sort_by_key(|s| s.start);
    };

    all_sessions.into_iter().fold(
        get_utc_zero(),
        |last_end, WorktimeSession { id, start, end }| {
            let end = end.unwrap_or(get_now());

            assert!(
                end > start,
                "Corrupt data - Session '{id}' end {end:?} before start {start:?}"
            );
            assert!(
                start > last_end,
                "Corrupt data - Session '{id}' overlap prev. end {end:?} after next start {start:?}"
            );

            end
        },
    );

    Ok(())
}
