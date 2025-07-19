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
    let db = WorktimeDatabase::new(pool, &clock);
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
        db::get_test_worktime_db, err::CommandError, stdin::test_utils::MockStdIn,
        stdout::test_utils::StdOutRecorder, time::test_utils::MockClock,
    };

    #[tokio::test]
    async fn test() {
        let clock = MockClock::default();
        let db = get_test_worktime_db(&clock).await.unwrap();
        let std_in = MockStdIn::new(vec![WorktimeCommand::Status]);
        let mut std_out = StdOutRecorder::default();

        run_loop(&clock, &db, &std_in, &mut std_out).await;

        // clock.set(15, 8, 00);
        //let actual = std_out.results.first().unwrap().clone().unwrap_err();
        assert_eq!(
            std_out.results,
            vec![Err(CommandError::Other("No previous sessions".to_string()))]
        );
    }
}
