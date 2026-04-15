use chrono::{DateTime, Datelike, Duration, Local, LocalResult, NaiveDate, TimeZone, Utc};
use clap::ValueEnum;
use std::time::SystemTime;

#[derive(Clone, ValueEnum)]
pub enum Period {
    /// Today
    Today,
    /// Yesterday
    Yesterday,
    /// This week (Monday to today)
    ThisWeek,
    /// Last week (Monday to Sunday)
    LastWeek,
    /// Week before last
    WeekBeforeLast,
    /// This month (1st to today)
    ThisMonth,
    /// Last month
    LastMonth,
    /// Month before last
    MonthBeforeLast,
}

/// Date-based filter for JSONL entries.
pub struct DateFilter {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

impl DateFilter {
    /// Create a filter from a number of days.
    /// `days == 0` means no filter (all entries).
    pub fn from_days(days: u32) -> Self {
        if days == 0 {
            Self {
                from: None,
                to: None,
            }
        } else {
            let today = Local::now().date_naive();
            let from_date = today - Duration::days(i64::from(days.saturating_sub(1)));
            Self {
                from: Some(local_date_start(from_date)),
                to: Some(local_date_end(today)),
            }
        }
    }

    /// Create a filter from explicit date range (YYYY-MM-DD strings).
    pub fn from_range(from: Option<&str>, to: Option<&str>) -> anyhow::Result<Self> {
        let from = from
            .map(|s| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map(local_date_start)
                    .map_err(|e| anyhow::anyhow!("Invalid --from date '{}': {}", s, e))
            })
            .transpose()?;

        let to = to
            .map(|s| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map(local_date_end)
                    .map_err(|e| anyhow::anyhow!("Invalid --to date '{}': {}", s, e))
            })
            .transpose()?;

        Ok(Self { from, to })
    }

    /// Create a filter from a named period (last-week, last-month, etc.)
    pub fn from_period(period: &Period) -> Self {
        let today = Local::now().date_naive();

        let (from_date, to_date) = match period {
            Period::Today => (today, today),
            Period::Yesterday => {
                let d = today - Duration::days(1);
                (d, d)
            }
            Period::ThisWeek => {
                let days_since_mon = today.weekday().num_days_from_monday();
                let monday = today - Duration::days(i64::from(days_since_mon));
                (monday, today)
            }
            Period::LastWeek => {
                let days_since_mon = today.weekday().num_days_from_monday();
                let this_monday = today - Duration::days(i64::from(days_since_mon));
                let last_monday = this_monday - Duration::days(7);
                let last_sunday = this_monday - Duration::days(1);
                (last_monday, last_sunday)
            }
            Period::WeekBeforeLast => {
                let days_since_mon = today.weekday().num_days_from_monday();
                let this_monday = today - Duration::days(i64::from(days_since_mon));
                let wbl_monday = this_monday - Duration::days(14);
                let wbl_sunday = this_monday - Duration::days(8);
                (wbl_monday, wbl_sunday)
            }
            Period::ThisMonth => {
                let first =
                    NaiveDate::from_ymd_opt(today.year(), today.month(), 1).expect("valid date");
                (first, today)
            }
            Period::LastMonth => {
                let first_this =
                    NaiveDate::from_ymd_opt(today.year(), today.month(), 1).expect("valid date");
                let last_day_prev = first_this - Duration::days(1);
                let first_prev =
                    NaiveDate::from_ymd_opt(last_day_prev.year(), last_day_prev.month(), 1)
                        .expect("valid date");
                (first_prev, last_day_prev)
            }
            Period::MonthBeforeLast => {
                let first_this =
                    NaiveDate::from_ymd_opt(today.year(), today.month(), 1).expect("valid date");
                let last_day_prev = first_this - Duration::days(1);
                let first_prev =
                    NaiveDate::from_ymd_opt(last_day_prev.year(), last_day_prev.month(), 1)
                        .expect("valid date");
                let last_day_prev2 = first_prev - Duration::days(1);
                let first_prev2 =
                    NaiveDate::from_ymd_opt(last_day_prev2.year(), last_day_prev2.month(), 1)
                        .expect("valid date");
                (first_prev2, last_day_prev2)
            }
        };

        Self {
            from: Some(local_date_start(from_date)),
            to: Some(local_date_end(to_date)),
        }
    }

    /// Check if a timestamp string passes the filter.
    pub fn matches(&self, timestamp: &str) -> bool {
        let dt = match parse_timestamp(timestamp) {
            Some(d) => d,
            None => return true, // don't drop data silently
        };

        self.matches_datetime(dt)
    }

    /// Check if a Unix timestamp (seconds) passes the filter.
    pub fn matches_unix_seconds(&self, timestamp: i64) -> bool {
        let dt = match DateTime::from_timestamp(timestamp, 0) {
            Some(dt) => dt,
            None => return true, // don't drop data silently
        };

        self.matches_datetime(dt)
    }

    /// Convert lower bound to SystemTime for file mtime pre-filtering.
    pub fn mtime_cutoff(&self) -> Option<SystemTime> {
        self.from.map(SystemTime::from)
    }

    fn matches_datetime(&self, dt: DateTime<Utc>) -> bool {
        if let Some(ref from) = self.from
            && dt < *from
        {
            return false;
        }
        if let Some(ref to) = self.to
            && dt > *to
        {
            return false;
        }
        true
    }
}

fn parse_timestamp(timestamp: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp) {
        return Some(dt.with_timezone(&Utc));
    }
    let with_z = format!("{timestamp}Z");
    if let Ok(dt) = DateTime::parse_from_rfc3339(&with_z) {
        return Some(dt.with_timezone(&Utc));
    }
    None
}

fn local_date_start(date: NaiveDate) -> DateTime<Utc> {
    let naive = date.and_hms_opt(0, 0, 0).expect("valid local start time");
    local_datetime_to_utc(naive)
}

fn local_date_end(date: NaiveDate) -> DateTime<Utc> {
    let naive = date.and_hms_opt(23, 59, 59).expect("valid local end time");
    local_datetime_to_utc(naive)
}

fn local_datetime_to_utc(naive: chrono::NaiveDateTime) -> DateTime<Utc> {
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(dt) => dt.with_timezone(&Utc),
        LocalResult::Ambiguous(earliest, _) => earliest.with_timezone(&Utc),
        LocalResult::None => panic!("invalid local datetime: {naive}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn local_timestamp(date: NaiveDate, hour: u32, minute: u32, second: u32) -> String {
        let naive = date
            .and_hms_opt(hour, minute, second)
            .expect("valid local test time");
        local_datetime_to_utc(naive).to_rfc3339()
    }

    #[test]
    fn test_no_filter() {
        let filter = DateFilter::from_days(0);
        assert!(filter.matches("2020-01-01T00:00:00Z"));
        assert!(filter.matches("2099-01-01T00:00:00Z"));
    }

    #[test]
    fn test_filter_recent() {
        let filter = DateFilter::from_days(1);
        let cutoff = filter.mtime_cutoff().expect("cutoff");
        let cutoff_utc = DateTime::<Utc>::from(cutoff);
        let cutoff_local = cutoff_utc.with_timezone(&Local);
        assert_eq!(cutoff_local.hour(), 0);
        assert_eq!(cutoff_local.minute(), 0);
        assert_eq!(cutoff_local.second(), 0);

        assert!(!filter.matches_unix_seconds(cutoff_utc.timestamp() - 1));
        assert!(filter.matches_unix_seconds(cutoff_utc.timestamp()));
        assert!(filter.matches(&local_timestamp(cutoff_local.date_naive(), 12, 0, 0)));
        assert!(!filter.matches(&local_timestamp(
            cutoff_local.date_naive() - Duration::days(1),
            23,
            59,
            59
        )));
    }

    #[test]
    fn test_unparseable_timestamp_passes() {
        let filter = DateFilter::from_days(1);
        assert!(filter.matches("not-a-date"));
    }

    #[test]
    fn test_unix_seconds_filter() {
        let filter =
            DateFilter::from_range(Some("2026-03-15"), Some("2026-03-17")).expect("valid range");
        let inside = local_datetime_to_utc(
            NaiveDate::from_ymd_opt(2026, 3, 15)
                .expect("valid date")
                .and_hms_opt(9, 30, 0)
                .expect("valid time"),
        )
        .timestamp();
        let outside = local_datetime_to_utc(
            NaiveDate::from_ymd_opt(2026, 3, 14)
                .expect("valid date")
                .and_hms_opt(23, 59, 59)
                .expect("valid time"),
        )
        .timestamp();
        assert!(filter.matches_unix_seconds(inside));
        assert!(!filter.matches_unix_seconds(outside));
    }

    #[test]
    fn test_range_filter() {
        let filter =
            DateFilter::from_range(Some("2026-03-15"), Some("2026-03-17")).expect("valid range");
        assert!(filter.matches(&local_timestamp(
            NaiveDate::from_ymd_opt(2026, 3, 15).expect("valid date"),
            10,
            0,
            0
        )));
        assert!(filter.matches(&local_timestamp(
            NaiveDate::from_ymd_opt(2026, 3, 16).expect("valid date"),
            12,
            0,
            0
        )));
        assert!(filter.matches(&local_timestamp(
            NaiveDate::from_ymd_opt(2026, 3, 17).expect("valid date"),
            23,
            59,
            59
        )));
        assert!(!filter.matches(&local_timestamp(
            NaiveDate::from_ymd_opt(2026, 3, 14).expect("valid date"),
            23,
            59,
            59
        )));
        assert!(!filter.matches(&local_timestamp(
            NaiveDate::from_ymd_opt(2026, 3, 18).expect("valid date"),
            0,
            0,
            0
        )));
    }

    #[test]
    fn test_from_only() {
        let filter = DateFilter::from_range(Some("2026-03-15"), None).expect("valid");
        assert!(!filter.matches(&local_timestamp(
            NaiveDate::from_ymd_opt(2026, 3, 14).expect("valid date"),
            23,
            59,
            59
        )));
        assert!(filter.matches(&local_timestamp(
            NaiveDate::from_ymd_opt(2026, 3, 15).expect("valid date"),
            0,
            0,
            0
        )));
        assert!(filter.matches(&local_timestamp(
            NaiveDate::from_ymd_opt(2099, 1, 1).expect("valid date"),
            0,
            0,
            0
        )));
    }

    #[test]
    fn test_to_only() {
        let filter = DateFilter::from_range(None, Some("2026-03-17")).expect("valid");
        assert!(filter.matches(&local_timestamp(
            NaiveDate::from_ymd_opt(2020, 1, 1).expect("valid date"),
            0,
            0,
            0
        )));
        assert!(filter.matches(&local_timestamp(
            NaiveDate::from_ymd_opt(2026, 3, 17).expect("valid date"),
            23,
            59,
            59
        )));
        assert!(!filter.matches(&local_timestamp(
            NaiveDate::from_ymd_opt(2026, 3, 18).expect("valid date"),
            0,
            0,
            0
        )));
    }
}
