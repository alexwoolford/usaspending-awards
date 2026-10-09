//! Fact-clock helper: TEXT `YYYY-MM-DDTHH:MM:SSZ` (always `Z`, no fraction).

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Utc};

pub const INSTANT_FMT: &str = "%Y-%m-%dT%H:%M:%SZ";
pub const DATE_FMT: &str = "%Y-%m-%d";

/// Current federal fiscal year and the one before it.
/// A fiscal year starts on 1 October of the previous calendar year.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwardWindow {
    pub prior_fy: i32,
    pub current_fy: i32,
    /// 1 October of the prior fiscal year, `YYYY-MM-DD`.
    pub start: String,
}

pub fn award_window(as_of: &str) -> Result<AwardWindow> {
    let date = parse_utc_date(as_of)?;
    let current_fy = if date.month() >= 10 {
        date.year() + 1
    } else {
        date.year()
    };
    let prior_fy = current_fy - 1;
    Ok(AwardWindow {
        prior_fy,
        current_fy,
        start: format!("{:04}-10-01", prior_fy - 1),
    })
}

pub fn utc_iso(dt: DateTime<Utc>) -> String {
    dt.format(INSTANT_FMT).to_string()
}

pub fn utc_date(dt: DateTime<Utc>) -> String {
    dt.format(DATE_FMT).to_string()
}

pub fn parse_utc_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, DATE_FMT).with_context(|| format!("parsing date {s:?}"))
}

pub fn parse_utc_iso(s: &str) -> Result<DateTime<Utc>> {
    if s.contains('.') || s.contains('+') || s.ends_with("UTC") {
        bail!("fact instant must be {INSTANT_FMT}, got {s:?}");
    }
    let naive = NaiveDateTime::parse_from_str(s, INSTANT_FMT)
        .with_context(|| format!("parsing fact instant {s:?}"))?;
    Ok(naive.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fiscal_years_follow_the_october_boundary() {
        let before = award_window("2026-09-30").unwrap();
        assert_eq!(before.prior_fy, 2025);
        assert_eq!(before.current_fy, 2026);
        assert_eq!(before.start, "2024-10-01");

        let opening = award_window("2026-10-08").unwrap();
        assert_eq!(opening.prior_fy, 2026);
        assert_eq!(opening.current_fy, 2027);
        assert_eq!(opening.start, "2025-10-01");

        let next = award_window("2027-10-01").unwrap();
        assert_eq!(next.prior_fy, 2027);
        assert_eq!(next.current_fy, 2028);
        assert_eq!(next.start, "2026-10-01");
    }
}
