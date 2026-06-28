use crate::{
    DB_FILE_PATH,
    db::{
        WorktimeDatabase,
        time_off::{TimeOffEntry, TimeOffKind},
        worktime_session::WorktimeSession,
    },
    err::{CommandError, CommandResult},
    http,
    time::*,
};
use chrono::{Datelike, NaiveDate, NaiveTime, Weekday};
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::{collections::HashSet, ops::Deref, process::Command};
use strum::{Display, EnumIter, IntoEnumIterator};

#[derive(Parser)]
#[command(name = "worktime", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: WorktimeCommand,
}

/// responsible for stdin/stdout & logic
#[derive(Debug, Subcommand, Clone)]
pub enum WorktimeCommand {
    /// Prints current state
    Status,
    /// Start tracking time
    Start,
    /// Stop tracking time
    Stop,
    /// Report today's total work time
    Report {
        /// The kind of report to generate
        #[arg(value_enum, default_value_t = ReportKind::Day)]
        kind: ReportKind,
    },
    /// Correct QoL - sets start/end of session with id to hours:minutes
    Correct {
        /// n-th last session (0-based)
        #[arg(default_value_t = 0)]
        nth_last: u32,
        #[arg(value_enum, default_value_t = CorrectionKind::Start)]
        kind: CorrectionKind,
        #[arg()]
        hours: u8,
        #[arg()]
        minutes: u8,
    },
    /// Overwrite multiple days (no precise start-end)
    #[command(skip)]
    Overwrite {
        days: Vec<NaiveDate>,
        hours_per_day: u32,
    },
    /// Sync public holidays from the internet for the current year
    #[command(skip)]
    SyncHolidays {
        country_code: String,
        county: Option<String>,
    },
    /// Add vacation days for a date range
    #[command(skip)]
    AddVacation {
        from: NaiveDate,
        to: NaiveDate,
        label: Option<String>,
    },
    /// Sqlite3
    Sql,
    /// Prints Clap's help
    /// NOTE: can't be named help
    /// (causes runtime panic due to conflict with clap's help)
    #[command(hide = true)]
    InternalHelp,
    /// Exit program
    #[command(hide = true)]
    Quit,
}

/// the [`WorktimeCommand`] plus Help & Quit for
/// listing the Options in the MainMenu
/// should stay:
///     - iterable
///     - 'flat' (no nested state/data -> stdin selection)
#[derive(Debug, EnumIter, Display, Clone, Copy)]
pub enum MainMenuCommand {
    /// Prints current state
    Status,
    /// Start tracking time
    Start,
    /// Stop tracking time
    Stop,
    /// Report today's total work time
    Report,
    /// Correct (update indiv start or end)
    Correct,
    /// Correct (add/update multiple days)
    Overwrite,
    /// Sync public holidays
    SyncHolidays,
    /// Add vacation days
    AddVacation,
    /// Sqlite3
    Sql,
    /// Print Clap's help
    Help,
    /// Exit program
    Quit,
}

impl MainMenuCommand {
    pub fn wrapped_iter() -> MainMenuCommandIter {
        MainMenuCommand::iter()
    }
}

#[derive(Default, Debug, Clone, Copy, clap::ValueEnum, EnumIter, Display)]
pub enum ReportKind {
    #[default]
    Day,
    Week,
    Month,
    Timeline,
}

#[derive(Default, Debug, Clone, Copy, clap::ValueEnum, EnumIter, Display)]
pub enum CorrectionKind {
    #[default]
    Start,
    End,
}

impl ReportKind {
    pub fn wrapped_iter() -> ReportKindIter {
        ReportKind::iter()
    }
}

impl WorktimeCommand {
    pub async fn execute(&self, db: &WorktimeDatabase, clock: &impl Clock) -> CommandResult {
        match self {
            WorktimeCommand::Status => self.status(db).await,
            WorktimeCommand::Start => self.start(db, clock).await,
            WorktimeCommand::Stop => self.stop(db, clock).await,
            WorktimeCommand::Report { kind } => self.report(db, *kind, clock).await,
            WorktimeCommand::Correct {
                nth_last,
                kind,
                hours,
                minutes,
            } => self.correct(db, *nth_last, *kind, *hours, *minutes).await,
            WorktimeCommand::Overwrite {
                days,
                hours_per_day,
            } => self.overwrite_days(db, days, *hours_per_day).await,
            WorktimeCommand::SyncHolidays {
                country_code,
                county,
            } => self.sync_holidays(db, clock, country_code, county).await,
            WorktimeCommand::AddVacation { from, to, label } => {
                self.add_vacation(db, *from, *to, label.as_deref()).await
            }
            WorktimeCommand::Sql => self.sqlite(),
            WorktimeCommand::InternalHelp => self.help(),
            WorktimeCommand::Quit => Ok("See ya, bruv".to_string()),
        }
    }

    async fn status(&self, db: &WorktimeDatabase) -> CommandResult {
        match db.get_last_session().await? {
            Some(WorktimeSession {
                id: _,
                start: _,
                end: Some(_),
            }) => Ok("Not running".to_string()),
            Some(WorktimeSession {
                id: _,
                start,
                end: None,
            }) => Ok(format!("Running since {start}")),
            None => Err(CommandError::Other("No previous sessions".to_string())),
        }
    }

    async fn start(&self, db: &WorktimeDatabase, clock: &impl Clock) -> CommandResult {
        db.insert_start(clock)
            .await
            .map(|time| format!("Start at {}", display_time(&time)))
    }

    async fn stop(&self, db: &WorktimeDatabase, clock: &impl Clock) -> CommandResult {
        let last = db.get_last_session().await?;

        if last.is_none() {
            return Err("No previous sessions".into());
        }
        let last = last.unwrap();
        if last.end.is_some() {
            return Err("No session started".into());
        }

        db.insert_stop(last.id, clock)
            .await
            .map(|time| format!("Stop at {}", display_time(&time)))
            .map_err(|e| e.into())
    }

    async fn report(
        &self,
        db: &WorktimeDatabase,
        kind: ReportKind,
        clock: &impl Clock,
    ) -> CommandResult {
        if let ReportKind::Timeline = kind {
            let sessions: Vec<WorktimeSession> = db
                .get_all_sessions()
                .await?
                .into_iter()
                .filter(|session| session.end.is_some())
                .collect();

            if sessions.is_empty() {
                return Err(CommandError::Other("No completed sessions found!".into()));
            } else {
                let to = get_today(clock);
                let from = sessions
                    .iter()
                    .map(|s| s.start.date())
                    .min()
                    .unwrap_or(to);
                let time_off = db.get_time_off_between_dates(from, to).await?;
                return Ok(render_timeline(&sessions, from, to, &time_off));
            };
        }

        let ref_day = match kind {
            ReportKind::Day => get_today(clock),
            ReportKind::Week => get_week_start(clock),
            ReportKind::Month => get_month_start(clock),
            ReportKind::Timeline => unreachable!(),
        };
        let today = get_today(clock);
        let sessions = db.get_sessions_since(ref_day).await?;
        let delta = aggregate_session_times(&sessions, clock.get_now());
        let hours_worked = delta.num_minutes() as f64 / 60f64;

        let config = db.get_config().await?;
        let time_off = db.get_time_off_between_dates(ref_day, today).await?;

        let daily_target = config.expected_weekly_hours / 5;

        let working_weekdays: i64 = {
            let mut count = 0i64;
            let mut d = ref_day;
            while d <= today {
                if !matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
                    count += 1;
                }
                d = d.succ_opt().unwrap();
            }
            count
        };

        let is_weekday =
            |e: &&TimeOffEntry| !matches!(e.date.weekday(), Weekday::Sat | Weekday::Sun);

        let holiday_weekdays: i64 = time_off
            .iter()
            .filter(|e| e.kind == TimeOffKind::Holiday && is_weekday(e))
            .count() as i64;

        let vacation_weekdays: i64 = time_off
            .iter()
            .filter(|e| e.kind == TimeOffKind::Vacation && is_weekday(e))
            .count() as i64;

        let expected_hours = working_weekdays * daily_target
            - holiday_weekdays * config.hours_per_holiday
            - vacation_weekdays * config.hours_per_holiday;

        let time_off_note = {
            let holiday_part = match holiday_weekdays {
                0 => None,
                1 => Some("1 holiday".to_string()),
                n => Some(format!("{n} holidays")),
            };
            let vacation_part = match vacation_weekdays {
                0 => None,
                1 => Some("1 vacation day".to_string()),
                n => Some(format!("{n} vacation days")),
            };
            match (holiday_part, vacation_part) {
                (None, None) => String::new(),
                (Some(h), None) => format!(" ({h})"),
                (None, Some(v)) => format!(" ({v})"),
                (Some(h), Some(v)) => format!(" ({h}, {v})"),
            }
        };

        Ok(format!(
            "{kind:?}: {hours_worked:.1}h worked / {expected_hours}.0h expected{time_off_note}"
        ))
    }

    fn sqlite(&self) -> CommandResult {
        match Command::new("sqlite3").arg(DB_FILE_PATH.deref()).spawn() {
            Ok(mut child) => match child.wait() {
                Ok(_) => Ok(String::default()),
                Err(_) => Err("Failed to wait on sqlite3".into()),
            },
            Err(_) => Err("Doesn't seem like you got sqlite3 installed or in $PATH".into()),
        }
    }

    fn help(&self) -> CommandResult {
        let styled = <Cli as clap::CommandFactory>::command().render_help();
        Ok(format!("{styled}"))
    }

    async fn correct(
        &self,
        db: &WorktimeDatabase,
        nth_last: u32,
        kind: CorrectionKind,
        hours: u8,
        minutes: u8,
    ) -> Result<String, CommandError> {
        let session = db.get_nth_last_session(nth_last).await?;
        let id = session.id;

        let date_time = session.start.date().and_time(
            NaiveTime::from_hms_opt(hours as u32, minutes as u32, 0).expect("cannot build time"),
        );

        match kind {
            CorrectionKind::Start => Ok(db.update_start_time(id, &date_time).await.map(|()| {
                format!(
                    "Start time of '{id}' has been updated to '{}'",
                    display_time(&date_time)
                )
            })?),
            CorrectionKind::End => Ok(db.update_end_time(id, &date_time).await.map(|()| {
                format!(
                    "End time of '{id}' has been updated to '{}'",
                    display_time(&date_time)
                )
            })?),
        }
    }

    async fn sync_holidays(
        &self,
        db: &WorktimeDatabase,
        clock: &impl Clock,
        country_code: &str,
        county: &Option<String>,
    ) -> CommandResult {
        let year = clock.get_now().year();

        let holidays = http::fetch::get_public_holidays(&http::CLIENT, year, country_code)
            .await
            .map_err(|e| CommandError::Other(format!("Failed to reach holiday API: {e}")))?;

        if holidays.is_empty() {
            return Ok(format!("No holidays found for {country_code} in {year}"));
        }

        db.delete_holidays_for_year(year).await?;

        let results = holidays
            .iter()
            .filter(|holiday| match county {
                None => holiday.counties.is_none(),
                Some(county) => {
                    holiday.counties.is_none()
                        || holiday
                            .counties
                            .as_ref()
                            .is_some_and(|counties| counties.contains(county))
                }
            })
            .map(|holiday| {
                db.insert_time_off_or_ignore(
                    holiday.date,
                    TimeOffKind::Holiday,
                    Some(holiday.name.as_str()),
                )
            });

        let mut stored = 0;

        for r in results {
            match r.await {
                Ok(()) => stored += 1,
                Err(e) => {
                    db.delete_holidays_for_year(year).await?;
                    return Err(e.into());
                }
            }
        }

        Ok(format!("{stored} holidays stored for {country_code}"))
    }

    async fn add_vacation(
        &self,
        db: &WorktimeDatabase,
        from: NaiveDate,
        to: NaiveDate,
        label: Option<&str>,
    ) -> CommandResult {
        if from > to {
            return Err(CommandError::Other(
                "Start date must be before end date".into(),
            ));
        }

        let existing = db.get_time_off_between_dates(from, to).await?;
        let existing_dates: HashSet<NaiveDate> = existing.iter().map(|e| e.date).collect();
        let holiday_weekday_count = existing
            .iter()
            .filter(|e| {
                e.kind == TimeOffKind::Holiday
                    && !matches!(e.date.weekday(), Weekday::Sat | Weekday::Sun)
            })
            .count();

        let mut added = 0usize;
        let mut day = from;
        while day <= to {
            if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun)
                && !existing_dates.contains(&day)
            {
                db.insert_time_off_or_ignore(day, TimeOffKind::Vacation, label)
                    .await?;
                added += 1;
            }
            day = day.succ_opt().unwrap();
        }

        let skip_note = match holiday_weekday_count {
            0 => String::new(),
            1 => ", skipped 1 public holiday".to_string(),
            n => format!(", skipped {n} public holidays"),
        };

        Ok(format!(
            "Added {added} vacation days ({from} – {to}{skip_note})"
        ))
    }

    async fn overwrite_days(
        &self,
        db: &WorktimeDatabase,
        days: &[NaiveDate],
        hours_per_day: u32,
    ) -> Result<String, CommandError> {
        for d in days {
            db.remove_sessions_by_day(*d).await?;
            db.insert_session(*d, hours_per_day).await?;
        }

        Ok(format!("Updated {} days", days.len()))
    }
}

fn render_timeline(
    sessions: &[WorktimeSession],
    from: NaiveDate,
    to: NaiveDate,
    time_off: &[TimeOffEntry],
) -> String {
    if from > to {
        return "No sessions recorded yet".to_string();
    }
    let mut rows = Vec::new();
    let mut day = from;
    while day <= to {
        let date_label = day.format("%a %Y-%m-%d").to_string();
        let blocks: String = (0u32..24)
            .map(|hour| {
                if minutes_worked_in_hour(sessions, day, hour) >= 30 {
                    '█'
                } else {
                    '░'
                }
            })
            .collect();

        let time_off_entry = time_off.iter().find(|e| e.date == day);
        let is_holiday = time_off_entry.is_some_and(|e| e.kind == TimeOffKind::Holiday);
        let is_vacation = time_off_entry.is_some_and(|e| e.kind == TimeOffKind::Vacation);
        let is_weekend = matches!(day.weekday(), Weekday::Sat | Weekday::Sun);

        let suffix = time_off_entry
            .and_then(|e| e.label.as_deref())
            .map(|l| format!("  {l}"))
            .unwrap_or_default();

        let row_text = format!("{}  {}{}", date_label, blocks, suffix);

        let row = if is_holiday {
            row_text.red().to_string()
        } else if is_vacation {
            row_text.yellow().to_string()
        } else if is_weekend {
            row_text.truecolor(180, 80, 80).to_string()
        } else {
            row_text
        };

        rows.push(row);
        day = day.succ_opt().unwrap();
    }
    rows.join("\n")
}

fn minutes_worked_in_hour(sessions: &[WorktimeSession], date: NaiveDate, hour: u32) -> i64 {
    let hour_start = date.and_time(NaiveTime::from_hms_opt(hour, 0, 0).unwrap());
    let hour_end = if hour < 23 {
        date.and_time(NaiveTime::from_hms_opt(hour + 1, 0, 0).unwrap())
    } else {
        date.succ_opt().unwrap().and_time(NaiveTime::MIN)
    };

    sessions
        .iter()
        .filter(|session| session.end.is_some())
        .map(|session| {
            let s_end = session.end.unwrap();
            let overlap_start = session.start.max(hour_start);
            let overlap_end = s_end.min(hour_end);
            if overlap_end > overlap_start {
                (overlap_end - overlap_start).num_minutes()
            } else {
                0
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use std::array::from_ref;

    use super::*;
    use crate::db::worktime_session::WorktimeSessionId;
    use chrono::NaiveDateTime;

    fn session(start: NaiveDateTime, end: Option<NaiveDateTime>) -> WorktimeSession {
        WorktimeSession::new(WorktimeSessionId::from(0u32), start, end)
    }

    fn dt(y: i32, mo: u32, d: u32, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap())
    }

    #[test]
    fn filled_hours_for_standard_session() {
        let s = session(dt(2026, 4, 21, 9, 0), Some(dt(2026, 4, 21, 17, 30)));
        let day = NaiveDate::from_ymd_opt(2026, 4, 21).unwrap();

        for hour in 0u32..9 {
            assert_eq!(minutes_worked_in_hour(from_ref(&s), day, hour), 0);
        }
        for hour in 9u32..17 {
            assert_eq!(minutes_worked_in_hour(from_ref(&s), day, hour), 60);
        }
        assert_eq!(minutes_worked_in_hour(from_ref(&s), day, 17), 30);
        for hour in 18u32..24 {
            assert_eq!(minutes_worked_in_hour(from_ref(&s), day, hour), 0);
        }
    }

    #[test]
    fn empty_day_renders_all_light_blocks() {
        let day = NaiveDate::from_ymd_opt(2026, 4, 21).unwrap();
        let output = render_timeline(&[], day, day, &[]);
        assert!(output.contains(&"░".repeat(24)));
        assert!(!output.contains('█'));
    }

    #[test]
    fn exactly_30_min_is_filled() {
        let s = session(dt(2026, 4, 21, 9, 0), Some(dt(2026, 4, 21, 9, 30)));
        let day = NaiveDate::from_ymd_opt(2026, 4, 21).unwrap();
        assert_eq!(minutes_worked_in_hour(&[s], day, 9), 30);
    }

    #[test]
    fn twenty_nine_min_is_not_filled() {
        let s = session(dt(2026, 4, 21, 9, 1), Some(dt(2026, 4, 21, 9, 30)));
        let day = NaiveDate::from_ymd_opt(2026, 4, 21).unwrap();
        assert_eq!(minutes_worked_in_hour(&[s], day, 9), 29);
    }

    #[test]
    fn cross_midnight_session_splits_across_days() {
        let s = session(dt(2026, 4, 21, 22, 0), Some(dt(2026, 4, 22, 2, 0)));
        let day1 = NaiveDate::from_ymd_opt(2026, 4, 21).unwrap();
        let day2 = NaiveDate::from_ymd_opt(2026, 4, 22).unwrap();

        assert_eq!(minutes_worked_in_hour(from_ref(&s), day1, 21), 0);
        assert_eq!(minutes_worked_in_hour(from_ref(&s), day1, 22), 60);
        assert_eq!(minutes_worked_in_hour(from_ref(&s), day1, 23), 60);
        assert_eq!(minutes_worked_in_hour(from_ref(&s), day2, 0), 60);
        assert_eq!(minutes_worked_in_hour(from_ref(&s), day2, 1), 60);
        assert_eq!(minutes_worked_in_hour(from_ref(&s), day2, 2), 0);
    }

    #[test]
    fn no_sessions_returns_message() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 21).unwrap();
        // from > to triggers the early return
        let output = render_timeline(&[], today, today, &[]);
        // should render a row for "today" (from == to, no sessions → all empty)
        assert!(output.contains("░"));
    }
}
