use crate::{
    cli::{Cli, ExtendedCommand, ReportKind, WorktimeCommand},
    stdout::add_linebrakes,
};
use clap::{CommandFactory, Parser};
use dialoguer::{Select, theme::ColorfulTheme};
use std::env;

/// proxy for all stdin interaction for testability
pub trait StdIn {
    fn parse(&self) -> Option<WorktimeCommand>;
    fn prompt(&self) -> WorktimeCommand;
    fn prompt_report_with_kind(&self) -> WorktimeCommand;
}

struct RealStdIn {}

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

    fn prompt(&self) -> WorktimeCommand {
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What you want, bruv?")
            .default(0)
            .items(
                ExtendedCommand::wrapped_iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<String>>()
                    .as_slice(),
            )
            .interact()
            .expect("Can't print select prompt");

        let extended_comm = *ExtendedCommand::wrapped_iter()
            .collect::<Vec<ExtendedCommand>>()
            .get(selection)
            .unwrap();

        match extended_comm {
            ExtendedCommand::Status => WorktimeCommand::Status,
            ExtendedCommand::Start => WorktimeCommand::Start,
            ExtendedCommand::Stop => WorktimeCommand::Stop,
            ExtendedCommand::Report { kind: _ } => self.prompt_report_with_kind(),
            ExtendedCommand::Sql => WorktimeCommand::Sql,
            ExtendedCommand::Help => {
                <Cli as CommandFactory>::command()
                    .print_help()
                    .expect("Can't print help");
                add_linebrakes();
                self.prompt()
            }
            ExtendedCommand::Quit => WorktimeCommand::Quit,
        }
    }

    fn prompt_report_with_kind(&self) -> WorktimeCommand {
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

        fn prompt(&self) -> WorktimeCommand {
            self.commands
                .borrow_mut()
                .next()
                .unwrap_or(WorktimeCommand::Quit)
        }

        fn prompt_report_with_kind(&self) -> WorktimeCommand {
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
