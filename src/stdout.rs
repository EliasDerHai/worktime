use crate::{cli::WorktimeCommand, err::CommandResult};

/// proxy for all stdout interaction for testability
pub trait StdOut {
    fn print(&mut self, cmd: WorktimeCommand, r: CommandResult);
}

struct RealStdOut {}

impl StdOut for RealStdOut {
    fn print(&mut self, cmd: WorktimeCommand, r: CommandResult) {
        match r {
            Ok(m) => println!("{m}"),
            Err(e) => match e {
                crate::err::CommandError::DatabaseError(error) => {
                    eprintln!("{cmd:?} failed with: {error}");
                }
                crate::err::CommandError::Other(reason) => {
                    eprintln!("{cmd:?} skipped due to: {reason}");
                }
            },
        }
        add_linebrakes();
    }
}

pub fn add_linebrakes() {
    print!("\n\n");
}

pub fn get_std_out() -> impl StdOut {
    RealStdOut {}
}

//##########################################################
// Mock stdout
//##########################################################
#[cfg(test)]
pub(crate) mod test_utils {
    use super::*;

    #[derive(Debug, Default)]
    pub struct StdOutRecorder {
        pub results: Vec<CommandResult>,
    }

    impl StdOut for StdOutRecorder {
        fn print(&mut self, _: WorktimeCommand, r: CommandResult) {
            self.results.push(r);
        }
    }
}
