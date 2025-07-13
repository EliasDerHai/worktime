use crate::db::WorktimeSession;
use chrono::{DateTime, Datelike, Days, Local, NaiveDate, NaiveDateTime, TimeDelta, Weekday};

pub fn display_time(
    time: &NaiveDateTime,
) -> chrono::format::DelayedFormat<chrono::format::StrftimeItems<'_>> {
    time.format("%H:%M:%S")
}

// TODO: trait this so we can use virtual time for testing
pub fn get_now() -> NaiveDateTime {
    Local::now().naive_local()
}

pub fn get_today() -> NaiveDate {
    get_now().date()
}

pub fn get_week_start() -> NaiveDate {
    let today = get_today();
    let week_offset = today.weekday().days_since(Weekday::Mon);
    let week_start = today
        .checked_sub_days(Days::new(week_offset.into()))
        .unwrap();
    week_start
}

pub fn get_month_start() -> NaiveDate {
    let today = get_today();
    let month_offset = today.day0();
    let month_start = today
        .checked_sub_days(Days::new(month_offset.into()))
        .unwrap();
    month_start
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
