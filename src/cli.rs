use crate::{
    DB_FILE_PATH,
    db::{WorktimeDatabase, WorktimeSession},
    err::{CommandError, CommandResult},
    time::*,
};
use chrono::NaiveTime;
use clap::{Parser, Subcommand};
use std::{ops::Deref, process::Command};
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
    /// Correct QoL - sets start/end of session with id to hours:minutes
    Correct {
        /// n-th last session (0-based)
        #[arg(default_value_t = 0)]
        nth_last: u32,
        #[arg(value_enum, default_value_t = CorrectionKind::Start)]
        kind: CorrectionKind,
        #[arg()]
        hours: u8,
        #[arg()]
        minutes: u8,
    },
    /// Sqlite3
    Sql,
    /// Prints Clap's help
    /// NOTE: can't be named help
    /// (causes runtime panic due to conflict with clap's help)
    #[command(hide = true)]
    InternalHelp,
    /// Exit program
    #[command(hide = true)]
    Quit,
}

/// the [`WorktimeCommand`] plus Help & Quit for
/// listing the Options in the MainMenu
/// should stay:
///     - iterable
///     - 'flat' (no nested state/data -> stdin selection)
#[derive(Debug, EnumIter, Display, Clone, Copy)]
pub enum MainMenuCommand {
    /// Prints current state
    Status,
    /// Start tracking time
    Start,
    /// Stop tracking time
    Stop,
    /// Report today's total work time
    Report,
    /// Correct QoL
    Correct,
    /// Sqlite3
    Sql,
    /// Print Clap's help
    Help,
    /// Exit program
    Quit,
}

impl MainMenuCommand {
    pub fn wrapped_iter() -> MainMenuCommandIter {
        MainMenuCommand::iter()
    }
}

#[derive(Default, Debug, Clone, Copy, clap::ValueEnum, EnumIter, Display)]
pub enum ReportKind {
    #[default]
    Day,
    Week,
    Month,
}

#[derive(Default, Debug, Clone, Copy, clap::ValueEnum, EnumIter, Display)]
pub enum CorrectionKind {
    #[default]
    Start,
    End,
}

impl ReportKind {
    pub fn wrapped_iter() -> ReportKindIter {
        ReportKind::iter()
    }
}

impl WorktimeCommand {
    pub async fn execute(&self, db: &WorktimeDatabase, clock: &impl Clock) -> CommandResult {
        match self {
            WorktimeCommand::Status => self.status(db).await,
            WorktimeCommand::Start => self.start(db, clock).await,
            WorktimeCommand::Stop => self.stop(db, clock).await,
            WorktimeCommand::Report { kind } => self.report(db, *kind, clock).await,
            WorktimeCommand::Correct {
                nth_last,
                kind,
                hours,
                minutes,
            } => self.correct(db, *nth_last, *kind, *hours, *minutes).await,
            WorktimeCommand::Sql => self.sqlite(),
            WorktimeCommand::InternalHelp => self.help(),
            WorktimeCommand::Quit => Ok("See ya, bruv".to_string()),
        }
    }

    async fn status(&self, db: &WorktimeDatabase) -> CommandResult {
        match db.get_last_session().await? {
            Some(WorktimeSession {
                id: _,
                start: _,
                end: Some(_),
            }) => Ok("Not running".to_string()),
            Some(WorktimeSession {
                id: _,
                start,
                end: None,
            }) => Ok(format!("Running since {start}")),
            None => Err(CommandError::Other("No previous sessions".to_string())),
        }
    }

    async fn start(&self, db: &WorktimeDatabase, clock: &impl Clock) -> CommandResult {
        db.insert_start(clock)
            .await
            .map(|time| format!("Start at {}", display_time(&time)))
    }

    async fn stop(&self, db: &WorktimeDatabase, clock: &impl Clock) -> CommandResult {
        let last = db.get_last_session().await?;

        if last.is_none() {
            return Err("No previous sessions".into());
        }
        let last = last.unwrap();
        if last.end.is_some() {
            return Err("No session started".into());
        }

        db.insert_stop(last.id, clock)
            .await
            .map(|time| format!("Stop at {}", display_time(&time)))
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
        Ok(format!("{kind:?}'s balance: {hours:.2}h"))
    }

    fn sqlite(&self) -> CommandResult {
        match Command::new("sqlite3").arg(DB_FILE_PATH.deref()).spawn() {
            Ok(mut child) => match child.wait() {
                Ok(_) => Ok(String::default()),
                Err(_) => Err("Failed to wait on sqlite3".into()),
            },
            Err(_) => Err("Doesn't seem like you got sqlite3 installed or in $PATH".into()),
        }
    }

    fn help(&self) -> CommandResult {
        let styled = <Cli as clap::CommandFactory>::command().render_help();
        Ok(format!("{styled}"))
    }

    async fn correct(
        &self,
        db: &WorktimeDatabase,
        nth_last: u32,
        kind: CorrectionKind,
        hours: u8,
        minutes: u8,
    ) -> Result<String, CommandError> {
        let session = db.get_nth_last_session(nth_last).await?;
        let id = session.id;

        let date_time = session.start.date().and_time(
            NaiveTime::from_hms_opt(hours as u32, minutes as u32, 0).expect("cannot build time"),
        );

        match kind {
            CorrectionKind::Start => Ok(db.update_start_time(id, &date_time).await.map(|()| {
                format!(
                    "Start time of '{id}' has been updated to '{}'",
                    display_time(&date_time)
                )
            })?),
            CorrectionKind::End => Ok(db.update_end_time(id, &date_time).await.map(|()| {
                format!(
                    "End time of '{id}' has been updated to '{}'",
                    display_time(&date_time)
                )
            })?),
        }
    }
}
