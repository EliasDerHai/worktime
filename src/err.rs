use std::{fmt, sync::Arc};

pub type CommandResult<T = String> = std::result::Result<T, CommandError>;
#[derive(Debug, Clone)]
pub enum CommandError {
    DatabaseError(Arc<sqlx::Error>), // ← now Clone
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

impl PartialEq for CommandError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::DatabaseError(l0), Self::DatabaseError(r0)) => {
                format!("{l0:?}") == format!("{r0:?}") // good enough? 
            }
            (Self::Other(l0), Self::Other(r0)) => l0 == r0,
            _ => false,
        }
    }
}

impl From<sqlx::Error> for CommandError {
    fn from(e: sqlx::Error) -> Self {
        CommandError::DatabaseError(Arc::new(e))
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
