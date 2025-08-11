use crate::{
    cli::{Cli, CorrectionKind, MainMenuCommand, ReportKind, WorktimeCommand},
    db::WorktimeDatabase,
};
use chrono::Timelike;
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
            .get_last_n_sessions_desc(10)
            .await
            .expect("Failed to query previous sessions");
        let session = prompt_selection("Which entry do you want to correct, bruv?", &last_sessions);

        let kind = *prompt_selection(
            "Start or end?",
            &[CorrectionKind::Start, CorrectionKind::End],
        );

        let start_min: u32 = session.start.num_seconds_from_midnight() / 60;
        let end_min: Option<u32> = session.end.map(|t| t.num_seconds_from_midnight() / 60);

        let time_input: String = Input::with_theme(&*THEME)
            .with_prompt("Enter the updated time (HH:MM)")
            .validate_with(|s: &String| -> Result<(), String> {
                let (h, m) = parse_hhmm(s)?;

                let updated_min = h as u32 * 60 + m as u32;

                match (kind, end_min) {
                    (CorrectionKind::Start, Some(end_min)) => {
                        if updated_min > end_min {
                            return Err("Start can't be after end!".to_string());
                        }
                    }
                    (CorrectionKind::End, _) => {
                        if updated_min < start_min {
                            return Err("Start can't be after end!".to_string());
                        }
                    }
                    _ => {}
                }

                Ok(())
            })
            .interact_text()
            .expect("Failed to read input");

        let (hours, minutes) =
            parse_hhmm(&time_input).expect("user-input should be validated already");

        WorktimeCommand::Correct {
            nth_last: last_sessions
                .iter()
                .position(|s| s.id == session.id)
                .unwrap() as u32,
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

/// returns (hours, minutes)
fn parse_hhmm(s: &str) -> Result<(u8, u8), String> {
    let (h, m) = s
        .split_once(':')
        .ok_or_else(|| "Use HH:MM (e.g., 09:30)".to_string())?;

    let h: u8 = h.parse().map_err(|_| "Hours must be 0–23".to_string())?;
    let m: u8 = m.parse().map_err(|_| "Minutes must be 0–59".to_string())?;

    if h > 23 {
        return Err("Hours must be 0–23".into());
    }
    if m > 59 {
        return Err("Minutes must be 0–59".into());
    }

    Ok((h, m))
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
