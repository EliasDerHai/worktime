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

pub fn get_std_in() -> impl StdIn {
    RealStdIn {}
}

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
        let selection = *prompt_selection(
            "What you want, bruv?",
            &MainMenuCommand::wrapped_iter().collect::<Vec<MainMenuCommand>>(),
        );

        match selection {
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
        let kind = *prompt_selection(
            "What report do you want, bruv?",
            &ReportKind::wrapped_iter().collect::<Vec<ReportKind>>(),
        );

        WorktimeCommand::Report { kind }
    }

    async fn prompt_correct(&self, db: &WorktimeDatabase) -> WorktimeCommand {
        let last_sessions = db
            .get_last_n_sessions(10)
            .await
            .expect("Failed to query previous sessions");
        let session = prompt_selection("Which entry do you want to correct, bruv?", &last_sessions);

        let kind = *prompt_selection(
            "Start or end?",
            &[CorrectionKind::Start, CorrectionKind::End],
        );

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
            id: session.id.into(),
            kind,
            hours,
            minutes,
        }
    }
}

//##########################################################
// UTIL
//##########################################################

static THEME: LazyLock<ColorfulTheme> = LazyLock::new(ColorfulTheme::default);
fn prompt_selection<'item, T: ToString>(prompt: &str, items: &'item [T]) -> &'item T {
    let idx = Select::with_theme(&*THEME)
        .default(0)
        .with_prompt(prompt)
        .items(items)
        .interact()
        .expect("Can't print choices");
    items.get(idx).expect("selection can never be out of range")
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
