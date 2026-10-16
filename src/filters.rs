//! Time-window filtering and datetime parsing utilities (Phase 27).
//!
//! Ported from the Python `filters` and the `BaseCollector.parse_datetime`
//! helper. `feed-rs` already yields `DateTime<Utc>` for feed entries, so
//! `parse_datetime_str` is mainly used for JSON APIs (Hugging Face).

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};

use crate::models::NewsItem;

/// Parse common date formats (RFC 2822, ISO 8601 / RFC 3339, naive) into UTC.
pub fn parse_datetime_str(raw: &str) -> Option<DateTime<Utc>> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }

    // RFC 2822 (standard RSS pubDate).
    if let Ok(dt) = DateTime::parse_from_rfc2822(s) {
        return Some(dt.with_timezone(&Utc));
    }

    // RFC 3339 / ISO 8601 (handles trailing 'Z' and explicit offsets).
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }

    // Naive datetimes without timezone — assume UTC.
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(Utc.from_utc_datetime(&ndt));
        }
    }

    // Date-only.
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        if let Some(ndt) = date.and_hms_opt(0, 0, 0) {
            return Some(Utc.from_utc_datetime(&ndt));
        }
    }

    None
}

/// Whether a publication timestamp falls within the last `hours` relative to `now`.
pub fn is_within_time_window(published_at: DateTime<Utc>, hours: i64, now: DateTime<Utc>) -> bool {
    let cutoff = now - Duration::hours(hours);
    published_at >= cutoff
}

/// Filter news items within the specified time window, newest first by default.
pub fn filter_by_time_window(
    items: Vec<NewsItem>,
    hours: i64,
    now: Option<DateTime<Utc>>,
    sort_newest_first: bool,
) -> Vec<NewsItem> {
    let current = now.unwrap_or_else(Utc::now);

    let mut filtered: Vec<NewsItem> = items
        .into_iter()
        .filter(|item| is_within_time_window(item.published_at, hours, current))
        .collect();

    if sort_newest_first {
        filtered.sort_by(|a, b| b.published_at.cmp(&a.published_at));
    }

    filtered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc2822_and_rfc3339() {
        assert!(parse_datetime_str("Wed, 15 Jan 2025 10:00:00 GMT").is_some());
        assert!(parse_datetime_str("2025-01-15T10:00:00Z").is_some());
        assert!(parse_datetime_str("2025-01-15T10:00:00.123Z").is_some());
        assert!(parse_datetime_str("2025-01-15").is_some());
        assert!(parse_datetime_str("not a date").is_none());
    }

    #[test]
    fn time_window_boundaries() {
        let now = Utc.with_ymd_and_hms(2025, 1, 2, 0, 0, 0).unwrap();
        let recent = Utc.with_ymd_and_hms(2025, 1, 1, 12, 0, 0).unwrap();
        let old = Utc.with_ymd_and_hms(2024, 12, 30, 0, 0, 0).unwrap();
        assert!(is_within_time_window(recent, 24, now));
        assert!(!is_within_time_window(old, 24, now));
    }
}
