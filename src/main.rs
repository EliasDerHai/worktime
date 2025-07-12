use cli::WorktimeCommand;
use db::WorktimeDatabase;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePool},
};

mod cli;
mod db;
mod err;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut command = WorktimeCommand::parse_or_prompt();

    let opts = SqliteConnectOptions::new()
        .filename("./worktime.db")
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;
    MIGRATOR.run(&pool).await?;

    let db = WorktimeDatabase::new(pool);

    loop {
        command.execute(&db).await;
        command = WorktimeCommand::prompt();
    }
}
