use crate::{
    DB_PATH,
    db::{WorktimeDatabase, WorktimeSession},
    err::CommandResult,
    time::*,
};
use clap::{CommandFactory, Parser, Subcommand};
use dialoguer::{Select, theme::ColorfulTheme};
use std::{env, process::Command};
use strum::{Display, EnumIter, IntoEnumIterator};

#[derive(Parser)]
#[command(name = "worktime", version)]
struct Cli {
    #[command(subcommand)]
    command: WorktimeCommand,
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
enum ExtendedCommand {
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
    /// Print Claps help
    Help,
    /// Do nothing
    Quit,
}

#[derive(Default, Debug, Clone, Copy, clap::ValueEnum, EnumIter, Display)]
pub enum ReportKind {
    #[default]
    Day,
    Week,
    Month,
}

impl WorktimeCommand {
    pub fn parse_or_prompt() -> Self {
        match Cli::try_parse() {
            Ok(c) => c.command,
            Err(e) => {
                if env::args_os().count() > 1 {
                    let _ = e.print();
                }
                Self::prompt()
            }
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
            ExtendedCommand::Report { kind: _ } => Self::prompt_report_with_kind(),
            ExtendedCommand::Sql => WorktimeCommand::Sql,
            ExtendedCommand::Help => {
                <Cli as CommandFactory>::command()
                    .print_help()
                    .expect("Can't print help");
                add_linebrakes();
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
            WorktimeCommand::Report { kind } => self.report(db, *kind).await,
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
        add_linebrakes();
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

    async fn report(&self, db: &WorktimeDatabase, kind: ReportKind) -> CommandResult {
        let ref_day = match kind {
            ReportKind::Day => get_today(),
            ReportKind::Week => get_week_start(),
            ReportKind::Month => get_month_start(),
        };
        let sessions = db.get_sessions_since(ref_day).await?;
        let delta = aggregate_session_times(&sessions);
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

    fn prompt_report_with_kind() -> WorktimeCommand {
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What report do you want, bruv?")
            .default(0)
            .items(
                ReportKind::iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<String>>()
                    .as_slice(),
            )
            .interact()
            .expect("Can't print choices");

        let kind = *ReportKind::iter()
            .collect::<Vec<ReportKind>>()
            .get(selection)
            .unwrap();

        WorktimeCommand::Report { kind }
    }
}

fn add_linebrakes() {
    print!("\n\n");
}
