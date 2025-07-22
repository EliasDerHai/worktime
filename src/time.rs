use crate::db::WorktimeSession;
use chrono::{Datelike, Days, Local, NaiveDate, NaiveDateTime, TimeDelta, Weekday};

//##########################################################
// Clock
//##########################################################
pub trait Clock {
    fn get_now(&self) -> NaiveDateTime;
}

struct RealClock {}

impl Clock for RealClock {
    fn get_now(&self) -> NaiveDateTime {
        Local::now().naive_local()
    }
}

pub fn get_clock() -> impl Clock {
    RealClock {}
}

//##########################################################
// Derived time (derived from "NOW"
//##########################################################
pub fn get_today(clock: &impl Clock) -> NaiveDate {
    clock.get_now().date()
}

pub fn get_week_start(clock: &impl Clock) -> NaiveDate {
    let today = get_today(clock);
    let week_offset = today.weekday().days_since(Weekday::Mon);
    today
        .checked_sub_days(Days::new(week_offset.into()))
        .unwrap()
}

pub fn get_month_start(clock: &impl Clock) -> NaiveDate {
    let today = get_today(clock);
    let month_offset = today.day0();
    today
        .checked_sub_days(Days::new(month_offset.into()))
        .unwrap()
}

//##########################################################
// Other utilities (not dependent on NOW)
//##########################################################
pub fn aggregate_session_times(sessions: &[WorktimeSession], now: NaiveDateTime) -> TimeDelta {
    sessions.iter().fold(
        TimeDelta::zero(),
        |curr, WorktimeSession { id: _, start, end }| {
            let start = *start;
            let end = end.unwrap_or(now);
            curr + (end - start)
        },
    )
}

pub fn display_time(
    time: &NaiveDateTime,
) -> chrono::format::DelayedFormat<chrono::format::StrftimeItems<'_>> {
    time.format("%H:%M:%S")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{test_utils::MockClock, *};

    #[test]
    fn should_get_week_start_from_wed() {
        let clock = MockClock::default();
        clock.set(9, 12, 0); // Wednesday
        let actual = get_week_start(&clock);
        let expected = NaiveDate::from_ymd_opt(2025, 7, 7).unwrap();

        assert_eq!(actual, expected);
    }
    #[test]
    fn should_get_week_start_from_sun() {
        let clock = MockClock::default();
        clock.set(13, 12, 0); // Sunday
        let actual = get_week_start(&clock);
        let expected = NaiveDate::from_ymd_opt(2025, 7, 7).unwrap();

        assert_eq!(actual, expected);
    }
}

//##########################################################
// Mock clock
//##########################################################
#[cfg(test)]
pub(crate) mod test_utils {
    use std::cell::RefCell;

    use super::*;

    #[derive(Debug, Default)]
    pub struct MockClock {
        pub mock_time: RefCell<NaiveDateTime>,
    }

    impl Clock for MockClock {
        fn get_now(&self) -> NaiveDateTime {
            *self.mock_time.borrow()
        }
    }

    impl MockClock {
        pub fn set(&self, d: u32, h: u32, m: u32) {
            *self.mock_time.borrow_mut() = NaiveDate::from_ymd_opt(2025, 7, d)
                .unwrap()
                .and_hms_opt(h, m, 0)
                .unwrap();
        }
    }
}
