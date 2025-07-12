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
    /// Do nothing
    Quit,
}

impl WorktimeCommand {
    pub fn parse_or_exit() -> Self {
        match Cli::try_parse() {
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
                    .expect("Can't print select prompt");

                let extended_comm = *ExtendedCommand::iter()
                    .collect::<Vec<ExtendedCommand>>()
                    .get(selection)
                    .unwrap();

                match extended_comm {
                    ExtendedCommand::Start => WorktimeCommand::Start,
                    ExtendedCommand::Stop => WorktimeCommand::Stop,
                    ExtendedCommand::Report => WorktimeCommand::Report,
                    ExtendedCommand::Help => {
                        <Cli as CommandFactory>::command()
                            .print_help()
                            .expect("Can't print help");
                        std::process::exit(0);
                    }
                    ExtendedCommand::Quit => {
                        std::process::exit(0);
                    }
                }
            }
        }
    }
}
