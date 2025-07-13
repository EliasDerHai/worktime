use crate::{
    DB_PATH,
    db::{WorktimeDatabase, WorktimeSession},
    err::CommandResult,
    time::*,
};
use clap::{Parser, Subcommand};
use std::process::Command;
use strum::{Display, EnumIter, IntoEnumIterator};

#[derive(Parser)]
#[command(name = "worktime", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: WorktimeCommand,
}

/// responsible for stdin/stdout & logic
#[derive(Debug, Subcommand, Clone, Copy)]
pub enum WorktimeCommand {
    /// Prints current state
    Status,
    /// Start tracking time
    Start,
    /// Stop tracking time
    Stop,
    /// Report today's total work time
    Report {
        /// The kind of report to generate
        #[arg(value_enum, default_value_t = ReportKind::Day)]
        kind: ReportKind,
    },
    /// Sqlite3
    Sql,
}

#[derive(Debug, EnumIter, Display, Clone, Copy)]
pub enum ExtendedCommand {
    /// Prints current state
    Status,
    /// Start tracking time
    Start,
    /// Stop tracking time
    Stop,
    /// Report today's total work time
    Report { kind: ReportKind },
    /// Sqlite3
    Sql,
    /// Print Clap's help
    Help,
    /// Do nothing
    Quit,
}

impl ExtendedCommand {
    pub fn wrapped_iter() -> ExtendedCommandIter {
        ExtendedCommand::iter()
    }
}

#[derive(Default, Debug, Clone, Copy, clap::ValueEnum, EnumIter, Display)]
pub enum ReportKind {
    #[default]
    Day,
    Week,
    Month,
}

impl ReportKind {
    pub fn wrapped_iter() -> ReportKindIter {
        ReportKind::iter()
    }
}

impl WorktimeCommand {
    pub async fn execute(&self, db: &WorktimeDatabase, clock: &impl Clock) {
        let r = match self {
            WorktimeCommand::Status => self.status(db).await,
            WorktimeCommand::Start => self.start(db).await,
            WorktimeCommand::Stop => self.stop(db).await,
            WorktimeCommand::Report { kind } => self.report(db, *kind, clock).await,
            WorktimeCommand::Sql => self.sqlite(),
        };
        if let Err(e) = r {
            match e {
                crate::err::CommandError::DatabaseError(error) => {
                    eprintln!("{self:?} failed with: {error}");
                }
                crate::err::CommandError::Other(reason) => {
                    eprintln!("{self:?} skipped due to: {reason}");
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

    async fn report(
        &self,
        db: &WorktimeDatabase,
        kind: ReportKind,
        clock: &impl Clock,
    ) -> CommandResult {
        let ref_day = match kind {
            ReportKind::Day => get_today(clock),
            ReportKind::Week => get_week_start(clock),
            ReportKind::Month => get_month_start(clock),
        };
        let sessions = db.get_sessions_since(ref_day).await?;
        let delta = aggregate_session_times(&sessions, clock.get_now());
        let hours = delta.num_minutes() as f64 / 60f64;
        println!("{kind:?}'s balance: {hours:.2}h");
        Ok(())
    }

    fn sqlite(&self) -> CommandResult {
        match Command::new("sqlite3").arg(DB_PATH).spawn() {
            Ok(mut child) => match child.wait() {
                Ok(_) => Ok(()),
                Err(_) => Err("Failed to wait on sqlite3".into()),
            },
            Err(_) => Err("Doesn't seem like you got sqlite3 installed or in $PATH".into()),
        }
    }
}
