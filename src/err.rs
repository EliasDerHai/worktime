use std::fmt;

pub type CommandResult<T = ()> = std::result::Result<T, CommandError>;
#[derive(Debug)]
pub enum CommandError {
    DatabaseError(sqlx::Error),
    /// Non critical; String is reason
    Other(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CommandError::DatabaseError(e) => Some(e),
            CommandError::Other(_) => None,
        }
    }
}

impl From<sqlx::Error> for CommandError {
    fn from(e: sqlx::Error) -> Self {
        CommandError::DatabaseError(e)
    }
}

impl From<&str> for CommandError {
    fn from(s: &str) -> Self {
        CommandError::Other(s.to_string())
    }
}

impl From<String> for CommandError {
    fn from(s: String) -> Self {
        CommandError::Other(s)
    }
}
