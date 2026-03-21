use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
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
            let now = Utc::now();
            let start = now - Duration::days(i64::from(days));
            let from = start
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .expect("valid time")
                .and_utc();
            Self {
                from: Some(from),
                to: None,
            }
        }
    }

    /// Create a filter from explicit date range (YYYY-MM-DD strings).
    pub fn from_range(from: Option<&str>, to: Option<&str>) -> anyhow::Result<Self> {
        let from = from
            .map(|s| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map(|d| d.and_hms_opt(0, 0, 0).expect("valid time").and_utc())
                    .map_err(|e| anyhow::anyhow!("Invalid --from date '{}': {}", s, e))
            })
            .transpose()?;

        let to = to
            .map(|s| {
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map(|d| d.and_hms_opt(23, 59, 59).expect("valid time").and_utc())
                    .map_err(|e| anyhow::anyhow!("Invalid --to date '{}': {}", s, e))
            })
            .transpose()?;

        Ok(Self { from, to })
    }

    /// Create a filter from a named period (last-week, last-month, etc.)
    pub fn from_period(period: &Period) -> Self {
        let today = Utc::now().date_naive();

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
            from: Some(from_date.and_hms_opt(0, 0, 0).expect("valid").and_utc()),
            to: Some(to_date.and_hms_opt(23, 59, 59).expect("valid").and_utc()),
        }
    }

    /// Check if a timestamp string passes the filter.
    pub fn matches(&self, timestamp: &str) -> bool {
        let dt = parse_timestamp(timestamp);
        let dt = match dt {
            Some(d) => d,
            None => return true, // don't drop data silently
        };

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

    /// Convert lower bound to SystemTime for file mtime pre-filtering.
    pub fn mtime_cutoff(&self) -> Option<SystemTime> {
        self.from.map(SystemTime::from)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_filter() {
        let filter = DateFilter::from_days(0);
        assert!(filter.matches("2020-01-01T00:00:00Z"));
        assert!(filter.matches("2099-01-01T00:00:00Z"));
    }

    #[test]
    fn test_filter_recent() {
        let filter = DateFilter::from_days(1);
        assert!(!filter.matches("2020-01-01T00:00:00Z"));
        assert!(filter.matches("2099-01-01T00:00:00Z"));
    }

    #[test]
    fn test_unparseable_timestamp_passes() {
        let filter = DateFilter::from_days(1);
        assert!(filter.matches("not-a-date"));
    }

    #[test]
    fn test_range_filter() {
        let filter =
            DateFilter::from_range(Some("2026-03-15"), Some("2026-03-17")).expect("valid range");
        assert!(filter.matches("2026-03-15T10:00:00Z"));
        assert!(filter.matches("2026-03-16T12:00:00Z"));
        assert!(filter.matches("2026-03-17T23:59:59Z"));
        assert!(!filter.matches("2026-03-14T23:59:59Z"));
        assert!(!filter.matches("2026-03-18T00:00:00Z"));
    }

    #[test]
    fn test_from_only() {
        let filter = DateFilter::from_range(Some("2026-03-15"), None).expect("valid");
        assert!(!filter.matches("2026-03-14T23:59:59Z"));
        assert!(filter.matches("2026-03-15T00:00:00Z"));
        assert!(filter.matches("2099-01-01T00:00:00Z"));
    }

    #[test]
    fn test_to_only() {
        let filter = DateFilter::from_range(None, Some("2026-03-17")).expect("valid");
        assert!(filter.matches("2020-01-01T00:00:00Z"));
        assert!(filter.matches("2026-03-17T23:59:59Z"));
        assert!(!filter.matches("2026-03-18T00:00:00Z"));
    }
}
