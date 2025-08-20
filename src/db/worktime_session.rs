use std::fmt::Display;

use chrono::NaiveDateTime;

use crate::time::display_time;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorktimeSessionId(pub u32);

impl Display for WorktimeSessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl From<i64> for WorktimeSessionId {
    fn from(value: i64) -> Self {
        WorktimeSessionId(u32::try_from(value).unwrap())
    }
}

impl From<u32> for WorktimeSessionId {
    fn from(value: u32) -> Self {
        WorktimeSessionId(value)
    }
}

impl From<WorktimeSessionId> for u32 {
    fn from(value: WorktimeSessionId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktimeSession {
    pub id: WorktimeSessionId,
    pub start: NaiveDateTime,
    pub end: Option<NaiveDateTime>,
}

impl WorktimeSession {
    #[allow(dead_code)]
    pub fn new(id: WorktimeSessionId, start: NaiveDateTime, end: Option<NaiveDateTime>) -> Self {
        Self { id, start, end }
    }
}

impl Display for WorktimeSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = &self.id;
        let start = display_time(&self.start);
        let end = &self
            .end
            .map(|t| display_time(&t).to_string())
            .unwrap_or("-".to_string());
        write!(f, "id: {id};start: {start};end: {end}")
    }
}

impl From<(i64, NaiveDateTime, Option<NaiveDateTime>)> for WorktimeSession {
    fn from((id, start, end): (i64, NaiveDateTime, Option<NaiveDateTime>)) -> Self {
        let id = WorktimeSessionId::from(id);
        Self { id, start, end }
    }
}
