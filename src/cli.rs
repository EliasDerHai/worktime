use crate::{
    db::{WorktimeDatabase, WorktimeSession},
    err::CommandResult,
};
use chrono::{DateTime, Local, NaiveDateTime, TimeDelta};
use clap::{CommandFactory, Parser, Subcommand};
use dialoguer::{Select, theme::ColorfulTheme};
use strum::{Display, EnumIter, IntoEnumIterator};

#[derive(Parser)]
#[command(name = "worktime", version)]
struct Cli {
    #[command(subcommand)]
    command: WorktimeCommand,
}

#[derive(Debug, Subcommand, Clone, Copy)]
pub enum WorktimeCommand {
    /// Prints current state
    Status,
    /// Start tracking time
    Start,
    /// Stop tracking time
    Stop,
    /// Report today's total work time
    Report,
}

#[derive(Debug, EnumIter, Display, Clone, Copy)]
enum ExtendedCommand {
    /// Prints current state
    Status,
    /// Start tracking time
    Start,
    /// Stop tracking time
    Stop,
    /// Report today's total work time
    Report,
    /// Print Claps help
    Help,
    /// Do nothing
    Quit,
}

impl WorktimeCommand {
    pub fn parse_or_prompt() -> Self {
        match Cli::try_parse() {
            Ok(c) => c.command,
            Err(_) => Self::prompt(),
        }
    }

    pub fn prompt() -> Self {
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
            .expect("Can't print select prompt");

        let extended_comm = *ExtendedCommand::iter()
            .collect::<Vec<ExtendedCommand>>()
            .get(selection)
            .unwrap();

        match extended_comm {
            ExtendedCommand::Status => WorktimeCommand::Status,
            ExtendedCommand::Start => WorktimeCommand::Start,
            ExtendedCommand::Stop => WorktimeCommand::Stop,
            ExtendedCommand::Report => WorktimeCommand::Report,
            ExtendedCommand::Help => {
                <Cli as CommandFactory>::command()
                    .print_help()
                    .expect("Can't print help");
                Self::prompt()
            }
            ExtendedCommand::Quit => {
                std::process::exit(0);
            }
        }
    }

    pub async fn execute(&self, db: &WorktimeDatabase) {
        let r = match self {
            WorktimeCommand::Status => self.status(db).await,
            WorktimeCommand::Start => self.start(db).await,
            WorktimeCommand::Stop => self.stop(db).await,
            WorktimeCommand::Report => self.report(db).await,
        };
        if let Err(e) = r {
            match e {
                crate::err::CommandError::DatabaseError(error) => {
                    eprintln!("{self:?} failed with: {error}")
                }
                crate::err::CommandError::LogicError(reason) => {
                    eprintln!("{self:?} skipped due to: {reason}")
                }
            };
        }
    }

    async fn status(&self, db: &WorktimeDatabase) -> CommandResult {
        match db.get_last_session().await? {
            Some(WorktimeSession {
                id: _,
                start: _,
                end: Some(_),
            }) => println!("Not running"),
            Some(WorktimeSession {
                id: _,
                start,
                end: None,
            }) => println!("Running since {start}"),
            None => eprintln!("No previous sessions"),
        };
        Ok(())
    }

    async fn start(&self, db: &WorktimeDatabase) -> CommandResult {
        db.insert_start()
            .await
            .map(|time| println!("Start at {}", display_time(&time)))
    }

    async fn stop(&self, db: &WorktimeDatabase) -> CommandResult {
        let last = db.get_last_session().await?;

        if last.is_none() {
            return Err("No previous sessions".into());
        }
        let last = last.unwrap();
        if last.end.is_some() {
            return Err("No session started".into());
        }

        db.insert_stop(last.id)
            .await
            .map(|time| println!("Stop at {}", display_time(&time)))
            .map_err(|e| e.into())
    }

    async fn report(&self, db: &WorktimeDatabase) -> CommandResult {
        let sessions = db.get_todays_sessions().await?;
        let delta = aggregate_session_times(sessions);
        let hours = delta.num_minutes() as f64 / 60f64;
        println!("Today's balance: {hours:.2}h",);
        Ok(())
    }
}

fn display_time(
    time: &NaiveDateTime,
) -> chrono::format::DelayedFormat<chrono::format::StrftimeItems<'_>> {
    time.format("%H:%M:%S")
}

fn get_now() -> NaiveDateTime {
    Local::now().naive_local()
}

fn get_utc_zero() -> NaiveDateTime {
    DateTime::from_timestamp_millis(0).unwrap().naive_local()
}

/// asserts no overlap and start before end (might panic)
fn aggregate_session_times(mut sessions: Vec<WorktimeSession>) -> TimeDelta {
    if !sessions.is_sorted_by_key(|s| s.id) {
        sessions.sort_by_key(|s| s.id);
    }

    sessions
        .iter()
        .fold(
            (TimeDelta::zero(), get_utc_zero()),
            |(curr, last_end), WorktimeSession { id: _, start, end }| {
                let start = *start;
                let end = end.unwrap_or(get_now());

                assert!(
                    end > start,
                    "Corrupt data - Session end {end:?} before start {start:?}"
                );
                assert!(
                    start > last_end,
                    "Corrupt data - Session overlap prev. end {end:?} after next start {start:?}"
                );

                let delta = curr + (end - start);
                (delta, end)
            },
        )
        .0
}
