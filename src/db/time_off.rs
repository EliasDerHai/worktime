use std::fmt::Display;

use chrono::NaiveDate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeOffId(pub u32);

impl Display for TimeOffId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl From<i64> for TimeOffId {
    fn from(value: i64) -> Self {
        TimeOffId(u32::try_from(value).unwrap())
    }
}

impl From<u32> for TimeOffId {
    fn from(value: u32) -> Self {
        TimeOffId(value)
    }
}

impl From<TimeOffId> for u32 {
    fn from(value: TimeOffId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
#[sqlx(rename_all = "lowercase")]
pub enum TimeOffKind {
    Holiday,
    Vacation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeOffEntry {
    pub id: TimeOffId,
    pub date: NaiveDate,
    pub kind: TimeOffKind,
    pub label: Option<String>,
}

impl TimeOffEntry {
    #[allow(dead_code)]
    pub fn new(id: TimeOffId, date: NaiveDate, kind: TimeOffKind, label: Option<String>) -> Self {
        Self {
            id,
            date,
            kind,
            label,
        }
    }
}

impl Display for TimeOffEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "id: {}; date: {}; kind: {:?}; label: {:?}",
            self.id, self.date, self.kind, self.label
        )
    }
}

impl From<(i64, NaiveDate, TimeOffKind, Option<String>)> for TimeOffEntry {
    fn from((id, date, kind, label): (i64, NaiveDate, TimeOffKind, Option<String>)) -> Self {
        let id = TimeOffId::from(id);
        Self {
            id,
            date,
            kind,
            label,
        }
    }
}
