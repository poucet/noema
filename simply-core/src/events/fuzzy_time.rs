//! Pragmatic fuzzy time parsing: turn human phrases like `"in 5 minutes"`,
//! `"tomorrow morning"`, or `"9:30pm"` into a concrete instant.
//!
//! This covers the common relative/named expressions used by one-shot timers.
//! Richer natural-language resolution (and LLM-assisted compilation that
//! preserves the original text) lands in the Stage 4 intent compiler; this
//! parser is the deterministic baseline it can fall back to.
//!
//! All resolution is relative to a caller-supplied `now`, in `now`'s own
//! reference frame, which keeps the function pure and testable. Callers that
//! want local-time semantics should pass a local `now`.

use chrono::{DateTime, Duration, TimeZone, Utc};

/// Named times of day → (hour, minute).
const NAMED_TIMES: &[(&str, (u32, u32))] = &[
    ("midnight", (0, 0)),
    ("morning", (9, 0)),
    ("midday", (12, 0)),
    ("noon", (12, 0)),
    ("afternoon", (15, 0)),
    ("evening", (18, 0)),
    ("night", (21, 0)),
];

/// Default time used when a day is named without a clock, e.g. `"tomorrow"`.
const DEFAULT_TIME: (u32, u32) = (9, 0);

/// Parse a fuzzy time expression into a concrete instant relative to `now`.
///
/// Returns `None` if the phrase isn't understood. Supported shapes:
/// - durations: `"in 5 minutes"`, `"in 2 hours"`, `"30s"`, `"1d"`, `"2w"`
/// - named days: `"today"`, `"tomorrow"`, `"tonight"`
/// - named times: `"morning"`, `"noon"`, `"evening"`, `"night"`, `"midnight"`
/// - clock times: `"9am"`, `"9:30pm"`, `"15:00"`, `"at 9 am"`
/// - combinations: `"tomorrow morning"`, `"tomorrow at 9am"`, `"today at 15:30"`
pub fn parse_fuzzy(input: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let normalized = input.trim().to_lowercase();
    if normalized.is_empty() {
        return None;
    }

    if let Some(delta) = parse_duration(&normalized) {
        return Some(now + delta);
    }

    resolve_calendar(&normalized, now)
}

/// Parse a relative duration like `"in 5 minutes"`, `"5m"`, or `"2 hours"`.
fn parse_duration(input: &str) -> Option<Duration> {
    let body = input.strip_prefix("in ").unwrap_or(input);
    // Collapse "5 minutes" → "5minutes" so number and unit sit together.
    let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();

    let split = compact.find(|c: char| !c.is_ascii_digit())?;
    if split == 0 {
        return None;
    }
    let (num, unit) = compact.split_at(split);
    let n: i64 = num.parse().ok()?;

    // Strip a trailing plural 's', but never reduce the bare "s" (seconds).
    let unit = unit
        .strip_suffix('s')
        .filter(|u| !u.is_empty())
        .unwrap_or(unit);

    let duration = match unit {
        "s" | "sec" | "second" => Duration::seconds(n),
        "m" | "min" | "minute" => Duration::minutes(n),
        "h" | "hr" | "hour" => Duration::hours(n),
        "d" | "day" => Duration::days(n),
        "w" | "week" => Duration::weeks(n),
        _ => return None,
    };
    Some(duration)
}

/// Resolve named-day / named-time / clock combinations.
fn resolve_calendar(input: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let mut day_offset: Option<i64> = None;
    if input.contains("tomorrow") {
        day_offset = Some(1);
    } else if input.contains("today") || input.contains("tonight") {
        day_offset = Some(0);
    }

    // Time of day: named word first, then an explicit clock token overrides.
    let mut time: Option<(u32, u32)> = None;
    if input.contains("tonight") {
        time = Some((21, 0));
    }
    for (word, hm) in NAMED_TIMES {
        if input.contains(word) {
            time = Some(*hm);
        }
    }
    let normalized = input
        .replace(" am", "am")
        .replace(" pm", "pm")
        .replace("a.m.", "am")
        .replace("p.m.", "pm");
    for token in normalized.split_whitespace() {
        if let Some(hm) = parse_clock(token) {
            time = Some(hm);
        }
    }

    let explicit_day = day_offset.is_some();
    let (hour, minute) = match (time, explicit_day) {
        (Some(hm), _) => hm,
        (None, true) => DEFAULT_TIME,
        (None, false) => return None,
    };

    let base_date = (now + Duration::days(day_offset.unwrap_or(0))).date_naive();
    let naive = base_date.and_hms_opt(hour, minute, 0)?;
    let mut candidate = Utc.from_utc_datetime(&naive);

    // A bare time ("9am") with no named day rolls forward if already past.
    if !explicit_day && candidate < now {
        candidate += Duration::days(1);
    }
    Some(candidate)
}

/// Parse a single clock token: `9am`, `9:30pm`, `15:00`, or `9`.
fn parse_clock(token: &str) -> Option<(u32, u32)> {
    let (body, pm) = if let Some(rest) = token.strip_suffix("am") {
        (rest, Some(false))
    } else if let Some(rest) = token.strip_suffix("pm") {
        (rest, Some(true))
    } else {
        (token, None)
    };

    let (hour_str, minute_str) = body.split_once(':').unwrap_or((body, "0"));
    let mut hour: u32 = hour_str.parse().ok()?;
    let minute: u32 = minute_str.parse().ok()?;
    if minute >= 60 {
        return None;
    }

    match pm {
        Some(false) if hour == 12 => hour = 0, // 12am → 00:00
        Some(true) if hour != 12 => hour += 12, // 1pm → 13:00
        _ => {}
    }
    (hour < 24).then_some((hour, minute))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn relative_durations() {
        let now = at(2026, 6, 1, 12, 0);
        assert_eq!(parse_fuzzy("in 5 minutes", now), Some(now + Duration::minutes(5)));
        assert_eq!(parse_fuzzy("in 2 hours", now), Some(now + Duration::hours(2)));
        assert_eq!(parse_fuzzy("30s", now), Some(now + Duration::seconds(30)));
        assert_eq!(parse_fuzzy("1d", now), Some(now + Duration::days(1)));
        assert_eq!(parse_fuzzy("2 weeks", now), Some(now + Duration::weeks(2)));
    }

    #[test]
    fn named_day_and_time() {
        let now = at(2026, 6, 1, 12, 0);
        assert_eq!(parse_fuzzy("tomorrow morning", now), Some(at(2026, 6, 2, 9, 0)));
        assert_eq!(parse_fuzzy("tomorrow", now), Some(at(2026, 6, 2, 9, 0)));
        assert_eq!(parse_fuzzy("tonight", now), Some(at(2026, 6, 1, 21, 0)));
        assert_eq!(parse_fuzzy("today at 15:30", now), Some(at(2026, 6, 1, 15, 30)));
    }

    #[test]
    fn clock_times() {
        let now = at(2026, 6, 1, 12, 0);
        // 9am already passed today → rolls to tomorrow.
        assert_eq!(parse_fuzzy("9am", now), Some(at(2026, 6, 2, 9, 0)));
        // 3pm still ahead today.
        assert_eq!(parse_fuzzy("3pm", now), Some(at(2026, 6, 1, 15, 0)));
        assert_eq!(parse_fuzzy("tomorrow at 9am", now), Some(at(2026, 6, 2, 9, 0)));
        assert_eq!(parse_fuzzy("9:30pm", now), Some(at(2026, 6, 1, 21, 30)));
        assert_eq!(parse_fuzzy("at 9 am", now), Some(at(2026, 6, 2, 9, 0)));
        assert_eq!(parse_fuzzy("12am", now), Some(at(2026, 6, 2, 0, 0)));
        assert_eq!(parse_fuzzy("12pm", now), Some(at(2026, 6, 1, 12, 0)));
    }

    #[test]
    fn unparseable_returns_none() {
        let now = at(2026, 6, 1, 12, 0);
        assert_eq!(parse_fuzzy("whenever", now), None);
        assert_eq!(parse_fuzzy("", now), None);
        assert_eq!(parse_fuzzy("25:00", now), None);
    }
}
