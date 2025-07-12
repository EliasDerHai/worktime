use clap::{CommandFactory, Parser, Subcommand};
use dialoguer::{Select, theme::ColorfulTheme};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::{error::Error, path::Path};
use strum::{Display, EnumIter, IntoEnumIterator};

mod db;

#[derive(Parser)]
#[command(name = "worktime", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand, Clone, Copy)]
enum Command {
    /// Start tracking time
    Start,
    /// Stop tracking time
    Stop,
    /// Report today's total work time
    Report,
}

#[derive(Debug, EnumIter, Display, Clone, Copy)]
enum ExtendedCommand {
    /// Start tracking time
    Start,
    /// Stop tracking time
    Stop,
    /// Report today's total work time
    Report,
    /// Print Claps help
    Help,
    /// Quit
    Quit,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let command = match Cli::try_parse() {
        Ok(c) => c.command,
        Err(_e) => {
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("What you want, bruv?")
                .default(0)
                .items(
                    ExtendedCommand::iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<String>>()
                        .as_slice(),
                )
                .interact()
                .unwrap();

            let extended_comm = *ExtendedCommand::iter()
                .collect::<Vec<ExtendedCommand>>()
                .get(selection)
                .unwrap();

            match extended_comm {
                ExtendedCommand::Start => Command::Start,
                ExtendedCommand::Stop => Command::Stop,
                ExtendedCommand::Report => Command::Report,
                ExtendedCommand::Help => {
                    <Cli as CommandFactory>::command()
                        .print_help()
                        .map_err(|e| format!("Error printing help: {}", e))?;
                    std::process::exit(0);
                }
                ExtendedCommand::Quit => {
                    std::process::exit(0);
                }
            }
        }
    };

    let db_path = "worktime.db";
    let first_run = !Path::new(db_path).exists();

    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);

    let pool = SqlitePool::connect_with(opts).await?;

    if first_run {
        db::init_db(&pool).await?;
    }

    match command {
        Command::Start => db::start(&pool).await?,
        Command::Stop => db::stop(&pool).await?,
        Command::Report => db::report(&pool).await?,
    }

    Ok(())
}
