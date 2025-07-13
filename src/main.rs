use cli::WorktimeCommand;
use db::WorktimeDatabase;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePool},
};
use stdin::{StdIn, add_linebrakes, get_std_in};
use time::{Clock, get_clock};

mod cli;
mod db;
mod err;
mod stdin;
mod time;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
pub static DB_PATH: &str = "worktime.db";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts = SqliteConnectOptions::new()
        .filename(DB_PATH)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;
    MIGRATOR.run(&pool).await?;

    let clock = get_clock();
    let db = WorktimeDatabase::new(pool, &clock);
    let std_in = get_std_in();
    run(&clock, &db, &std_in).await;
    Ok(())
}

async fn run(clock: &impl Clock, db: &WorktimeDatabase, std_in: &impl StdIn) {
    let mut command = std_in.parse().unwrap_or(WorktimeCommand::Status);
    loop {
        command.execute(db, clock).await;
        add_linebrakes();
        command = std_in.prompt();
    }
}
