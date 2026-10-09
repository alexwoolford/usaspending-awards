//! Fact-clock helper: TEXT `YYYY-MM-DDTHH:MM:SSZ` (always `Z`, no fraction).

use anyhow::{bail, Context, Result};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

pub const INSTANT_FMT: &str = "%Y-%m-%dT%H:%M:%SZ";
pub const DATE_FMT: &str = "%Y-%m-%d";
pub const WINDOW_START: &str = "2025-10-01";

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
