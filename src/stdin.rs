use crate::{
    cli::{Cli, CorrectionKind, MainMenuCommand, ReportKind, WorktimeCommand},
    db::WorktimeDatabase,
};
use clap::Parser;
use dialoguer::{Input, Select, theme::ColorfulTheme};
use std::{env, sync::LazyLock};

/// proxy for all stdin interaction for testability
pub trait StdIn {
    fn parse(&self) -> Option<WorktimeCommand>;
    async fn prompt(&self, db: &WorktimeDatabase) -> WorktimeCommand;
    async fn prompt_report(&self) -> WorktimeCommand;
    async fn prompt_correct(&self, db: &WorktimeDatabase) -> WorktimeCommand;
}

struct RealStdIn {}

static THEME: LazyLock<ColorfulTheme> = LazyLock::new(|| ColorfulTheme::default());

impl StdIn for RealStdIn {
    fn parse(&self) -> Option<WorktimeCommand> {
        match Cli::try_parse() {
            Ok(c) => Some(c.command),
            Err(e) => {
                if env::args_os().count() > 1 {
                    let _ = e.print();
                }
                None
            }
        }
    }

    async fn prompt(&self, db: &WorktimeDatabase) -> WorktimeCommand {
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What you want, bruv?")
            .default(0)
            .items(
                MainMenuCommand::wrapped_iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<String>>()
                    .as_slice(),
            )
            .interact()
            .expect("Can't print select prompt");

        let extended_comm = *MainMenuCommand::wrapped_iter()
            .collect::<Vec<MainMenuCommand>>()
            .get(selection)
            .unwrap();

        match extended_comm {
            MainMenuCommand::Status => WorktimeCommand::Status,
            MainMenuCommand::Start => WorktimeCommand::Start,
            MainMenuCommand::Stop => WorktimeCommand::Stop,
            MainMenuCommand::Report => self.prompt_report().await,
            MainMenuCommand::Sql => WorktimeCommand::Sql,
            MainMenuCommand::Help => WorktimeCommand::InternalHelp,
            MainMenuCommand::Quit => WorktimeCommand::Quit,
            MainMenuCommand::Correct => self.prompt_correct(db).await,
        }
    }

    async fn prompt_report(&self) -> WorktimeCommand {
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What report do you want, bruv?")
            .default(0)
            .items(
                ReportKind::wrapped_iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<String>>()
                    .as_slice(),
            )
            .interact()
            .expect("Can't print choices");

        let kind = *ReportKind::wrapped_iter()
            .collect::<Vec<ReportKind>>()
            .get(selection)
            .unwrap();

        WorktimeCommand::Report { kind }
    }

    async fn prompt_correct(&self, db: &WorktimeDatabase) -> WorktimeCommand {
        let last_sessions = db
            .get_last_n_sessions(10)
            .await
            .expect("Failed to query previous sessions");

        let session_selection = Select::with_theme(&*THEME)
            .with_prompt("Which entry do you want to correct, bruv?")
            .items(
                last_sessions
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<String>>()
                    .as_slice(),
            )
            .interact()
            .expect("Can't print choices");

        let selected_session = last_sessions.get(session_selection).unwrap();

        let kind_selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Start or end?")
            .items(&["Start", "End"])
            .interact()
            .expect("Can't print choices");

        let kind = match kind_selection {
            0 => CorrectionKind::Start,
            1 => CorrectionKind::Start,
            _ => unreachable!("User should only have two options"),
        };

        let hours_str: String = Input::with_theme(&*THEME)
            .with_prompt("Hour (HH, 00-23)")
            .validate_with(|s: &String| -> Result<(), &str> {
                if s.len() != 2 || !s.chars().all(|c| c.is_ascii_digit()) {
                    return Err("Enter exactly two digits, e.g., 09");
                }
                match s.parse::<u8>() {
                    Ok(v) if v <= 23 => Ok(()),
                    _ => Err("Value must be in 00..23"),
                }
            })
            .interact_text()
            .expect("Failed to read hours");

        // Exactly two digits for MM in 00..59
        let minutes_str: String = Input::with_theme(&*THEME)
            .with_prompt("Minute (MM, 00-59)")
            .validate_with(|s: &String| -> Result<(), &str> {
                if s.len() != 2 || !s.chars().all(|c| c.is_ascii_digit()) {
                    return Err("Enter exactly two digits, e.g., 07");
                }
                match s.parse::<u8>() {
                    Ok(v) if v <= 59 => Ok(()),
                    _ => Err("Value must be in 00..59"),
                }
            })
            .interact_text()
            .expect("Failed to read minutes");

        let hours: u8 = hours_str.parse().unwrap();
        let minutes: u8 = minutes_str.parse().unwrap();

        WorktimeCommand::Correct {
            id: selected_session.id.into(),
            kind,
            hours,
            minutes,
        }
    }
}

pub fn get_std_in() -> impl StdIn {
    RealStdIn {}
}

//##########################################################
// Mock stdin
//##########################################################
#[cfg(test)]
pub(crate) mod test_utils {
    use super::*;
    use std::{cell::RefCell, vec::IntoIter};

    pub struct MockStdIn {
        pub commands: RefCell<IntoIter<WorktimeCommand>>,
    }

    impl MockStdIn {
        pub(crate) fn new(vec: Vec<WorktimeCommand>) -> Self {
            Self {
                commands: RefCell::new(vec.into_iter()),
            }
        }
    }

    impl StdIn for MockStdIn {
        fn parse(&self) -> Option<WorktimeCommand> {
            self.commands.borrow_mut().next()
        }

        async fn prompt(&self, _: &WorktimeDatabase) -> WorktimeCommand {
            self.commands
                .borrow_mut()
                .next()
                .unwrap_or(WorktimeCommand::Quit)
        }

        async fn prompt_report(&self) -> WorktimeCommand {
            self.commands
                .borrow_mut()
                .next()
                .unwrap_or(WorktimeCommand::Quit)
        }

        async fn prompt_correct(&self, _: &WorktimeDatabase) -> WorktimeCommand {
            self.commands
                .borrow_mut()
                .next()
                .unwrap_or(WorktimeCommand::Quit)
        }
    }

    impl From<Vec<WorktimeCommand>> for MockStdIn {
        fn from(value: Vec<WorktimeCommand>) -> Self {
            Self::new(value)
        }
    }
}
