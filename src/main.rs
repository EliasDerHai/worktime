use cli::WorktimeCommand;
use db::WorktimeDatabase;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePool},
};
use std::{env, ops::Deref, path::PathBuf, sync::LazyLock};
use stdin::{StdIn, get_std_in};
use stdout::{StdOut, get_std_out};
use time::{Clock, get_clock};

mod cli;
mod db;
mod err;
mod stdin;
mod stdout;
mod time;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
static DB_FILE_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    env::current_exe()
        .expect("can't find exe path")
        .join("../worktime.db")
});

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts = SqliteConnectOptions::new()
        .filename(DB_FILE_PATH.deref())
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;
    MIGRATOR.run(&pool).await?;

    let clock = get_clock();
    let db = WorktimeDatabase::new(pool);
    let std_in = get_std_in();
    let mut std_out = get_std_out();
    run_loop(&clock, &db, &std_in, &mut std_out).await;
    Ok(())
}

async fn run_loop(
    clock: &impl Clock,
    db: &WorktimeDatabase,
    std_in: &impl StdIn,
    std_out: &mut impl StdOut,
) {
    let mut command = std_in.parse().unwrap_or(WorktimeCommand::Status);
    while !matches!(command, WorktimeCommand::Quit) {
        let result = command.execute(db, clock).await;
        std_out.print(command, result);
        command = std_in.prompt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cli::ReportKind, db::get_test_worktime_db, stdin::test_utils::MockStdIn,
        stdout::test_utils::StdOutRecorder, time::test_utils::MockClock,
    };

    async fn setup() -> (MockClock, StdOutRecorder, WorktimeDatabase) {
        let clock = MockClock::default();
        let db = get_test_worktime_db().await.unwrap();
        (clock, StdOutRecorder::default(), db)
    }

    #[tokio::test]
    async fn should_record_workday() {
        let (clock, mut recorder, db) = setup().await;

        let std_in: MockStdIn = vec![WorktimeCommand::Start].into();
        clock.set(1, 9, 00);
        run_loop(&clock, &db, &std_in, &mut recorder).await;

        let std_in: MockStdIn = vec![
            WorktimeCommand::Stop,
            WorktimeCommand::Report {
                kind: ReportKind::Day,
            },
        ]
        .into();
        clock.set(1, 15, 00);
        run_loop(&clock, &db, &std_in, &mut recorder).await;

        let last_out = recorder.results.last().unwrap().clone().unwrap();

        assert_ends_with(last_out.as_str(), "6.00h");
    }

    #[tokio::test]
    async fn should_record_workweek() {
        let (clock, mut recorder, db) = setup().await;

        for day_offset in 0..5 {
            clock.set(7 + day_offset, 9, 00); // 7 = Monday
            let std_in: MockStdIn = vec![WorktimeCommand::Start].into();
            run_loop(&clock, &db, &std_in, &mut recorder).await;

            let std_in: MockStdIn = vec![WorktimeCommand::Stop].into();
            clock.set(7 + day_offset, 17, 00); // 7 = Monday
            run_loop(&clock, &db, &std_in, &mut recorder).await;
        }

        let std_in: MockStdIn = vec![WorktimeCommand::Report {
            kind: ReportKind::Week,
        }]
        .into();
        run_loop(&clock, &db, &std_in, &mut recorder).await;
        let last_out = recorder.results.last().unwrap().clone().unwrap();

        assert_ends_with(last_out.as_str(), "40.00h");
    }

    fn assert_ends_with(actual: &str, expected_end: &str) {
        assert!(
            actual.ends_with(expected_end),
            "expected '{actual}' to end with '{expected_end}'"
        );
    }
}
