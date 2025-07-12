use crate::db::WorktimeSession;
use chrono::{DateTime, Local, NaiveDateTime, TimeDelta};

pub fn display_time(
    time: &NaiveDateTime,
) -> chrono::format::DelayedFormat<chrono::format::StrftimeItems<'_>> {
    time.format("%H:%M:%S")
}

pub fn get_now() -> NaiveDateTime {
    Local::now().naive_local()
}

pub fn get_utc_zero() -> NaiveDateTime {
    DateTime::from_timestamp_millis(0).unwrap().naive_local()
}

pub fn aggregate_session_times(sessions: &[WorktimeSession]) -> TimeDelta {
    sessions.iter().fold(
        TimeDelta::zero(),
        |curr, WorktimeSession { id: _, start, end }| {
            let start = *start;
            let end = end.unwrap_or(get_now());

            curr + (end - start)
        },
    )
}
