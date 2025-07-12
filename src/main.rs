use cli::Command;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePool},
};
use std::error::Error;

mod cli;
mod db;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let command = Command::parse_or_exit();

    let opts = SqliteConnectOptions::new()
        .filename("./worktime.db")
        .create_if_missing(true);

    println!("sqlite connect");
    let pool = SqlitePool::connect_with(opts).await?;
    println!("sqlite connected");

    println!("migration");
    MIGRATOR.run(&pool).await?;
    println!("migration done");

    println!("executing command");
    match command {
        Command::Start => db::start(&pool).await?,
        Command::Stop => db::stop(&pool).await?,
        Command::Report => db::report(&pool).await?,
    }
    println!("executed command");

    Ok(())
}
