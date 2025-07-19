use cli::WorktimeCommand;
use db::WorktimeDatabase;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePool},
};
use std::{env, ops::Deref, path::PathBuf, sync::LazyLock};
use stdin::{StdIn, add_linebrakes, get_std_in};
use time::{Clock, get_clock};

mod cli;
mod db;
mod err;
mod stdin;
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
    run_loop(&clock, &db, &std_in).await;
    Ok(())
}

async fn run_loop(clock: &impl Clock, db: &WorktimeDatabase, std_in: &impl StdIn) {
    let mut command = std_in.parse().unwrap_or(WorktimeCommand::Status);
    while !matches!(command, WorktimeCommand::Quit) {
        command.execute(db, clock).await;
        add_linebrakes();
        command = std_in.prompt();
    }
    println!("See ya, bruv");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::get_test_worktime_db, stdin::test_utils::MockStdIn, time::test_utils::MockClock,
    };

    #[tokio::test]
    async fn test() {
        let mut clock = MockClock::default();
        clock.set(15, 8, 00);
        let std_in = MockStdIn::new(vec![WorktimeCommand::Status]);
        let db = get_test_worktime_db(&clock).await.unwrap();

        run_loop(&clock, &db, &std_in).await;
    }
}
