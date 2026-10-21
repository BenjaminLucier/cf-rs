//! Small helpers shared by the request builders.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::error::{Error, Result};

/// User metadata captured by a builder. Serialization happens when the
/// setter is called, so the builder needn't hold a borrow; any error is
/// reported when the request is sent.
#[derive(Clone, Debug)]
pub(crate) struct Metadata(Result<serde_json::Value, String>);

impl Metadata {
    pub(crate) fn capture<T: Serialize + ?Sized>(metadata: &T) -> Self {
        Self(serde_json::to_value(metadata).map_err(|e| e.to_string()))
    }

    pub(crate) fn into_value(self) -> Result<serde_json::Value> {
        self.0
            .map_err(|e| Error::InvalidInput(format!("metadata: {e}")))
    }
}

/// A value that can be set, or explicitly cleared with `null`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Patch<T> {
    Set(T),
    Clear,
}

/// Seconds since the Unix epoch.
pub(crate) fn unix_secs(at: SystemTime, what: &str) -> Result<u64> {
    at.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| Error::InvalidInput(format!("{what} is before 1970")))
}

/// Formats a time as RFC 3339 in UTC with whole seconds, e.g.
/// `2021-09-14T16:00:00Z`.
pub(crate) fn rfc3339(at: SystemTime, what: &str) -> Result<String> {
    let secs = unix_secs(at, what)?;
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    Ok(format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    ))
}

/// Days since 1970-01-01 to (year, month, day) in the proleptic Gregorian
/// calendar. Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The RFC 3339 UTC time one microsecond after `raw`, keeping at least
/// microsecond precision, e.g. `2024-01-01T00:00:02.999999Z` becomes
/// `2024-01-01T00:00:03.000000Z`. Returns `None` for anything that isn't a
/// `YYYY-MM-DDTHH:MM:SS[.fraction]Z` time.
pub(crate) fn next_microsecond(raw: &str) -> Option<String> {
    let (secs, nanos, digits) = parse_rfc3339_utc(raw)?;
    let total = i128::from(secs) * 1_000_000_000 + i128::from(nanos) + 1_000;
    let secs = total.div_euclid(1_000_000_000) as i64;
    let nanos = total.rem_euclid(1_000_000_000) as u32;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let (h, min, s) = (rem / 3600, rem % 3600 / 60, rem % 60);
    let fraction = if digits > 6 {
        format!("{nanos:09}")
    } else {
        format!("{:06}", nanos / 1_000)
    };
    Some(format!(
        "{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{fraction}Z"
    ))
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.fraction]Z` into (Unix seconds, nanoseconds,
/// number of fraction digits). Only the `Z` (UTC) form is accepted.
fn parse_rfc3339_utc(raw: &str) -> Option<(i64, u32, usize)> {
    let raw = raw.strip_suffix('Z').or_else(|| raw.strip_suffix('z'))?;
    let (date, time) = raw.split_once(['T', 't'])?;
    let mut date = date.splitn(3, '-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: u32 = date.next()?.parse().ok()?;
    let day: u32 = date.next()?.parse().ok()?;
    let (hms, fraction) = match time.split_once('.') {
        Some((hms, fraction)) => (hms, fraction),
        None => (time, ""),
    };
    let mut hms = hms.splitn(3, ':');
    let hour: i64 = hms.next()?.parse().ok()?;
    let minute: i64 = hms.next()?.parse().ok()?;
    let second: i64 = hms.next()?.parse().ok()?;
    let valid = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && minute < 60
        && second < 60
        && fraction.len() <= 9
        && fraction.bytes().all(|b| b.is_ascii_digit());
    if !valid {
        return None;
    }
    let nanos: u32 = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<9}").parse().ok()?
    };
    let secs = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    Some((secs, nanos, fraction.len()))
}

/// (year, month, day) in the proleptic Gregorian calendar to days since
/// 1970-01-01. Howard Hinnant's `days_from_civil` algorithm, the inverse of
/// [`civil_from_days`].
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Checks that `value` is a finite number in `0.0..=1.0`.
pub(crate) fn unit_interval(what: &str, value: f64) -> Result<f64> {
    if (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(Error::InvalidInput(format!(
            "{what} must be between 0.0 and 1.0, got {value}"
        )))
    }
}

/// Checks a `maxDurationSeconds` value (1 to 36 000).
pub(crate) fn max_duration(secs: u32) -> Result<u32> {
    if (1..=36_000).contains(&secs) {
        Ok(secs)
    } else {
        Err(Error::InvalidInput(format!(
            "max duration must be between 1 and 36000 seconds, got {secs}"
        )))
    }
}

/// The wire value of a string enum, or an [`Error::InvalidInput`] naming
/// `what` if it is an `Unknown` variant.
pub(crate) fn wire<T: Serialize>(what: &str, value: &T) -> Result<String> {
    crate::types::wire_str(value).ok_or_else(|| Error::InvalidInput(format!("unknown {what}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn formats_rfc3339() {
        let at = |s| UNIX_EPOCH + Duration::from_secs(s);
        assert_eq!(rfc3339(at(0), "t").unwrap(), "1970-01-01T00:00:00Z");
        assert_eq!(
            rfc3339(at(1_631_635_200), "t").unwrap(),
            "2021-09-14T16:00:00Z"
        );
        // Leap day.
        assert_eq!(
            rfc3339(at(951_782_400), "t").unwrap(),
            "2000-02-29T00:00:00Z"
        );
        assert_eq!(
            rfc3339(at(4_102_444_799), "t").unwrap(),
            "2099-12-31T23:59:59Z"
        );
        // Sub-second precision is dropped.
        assert_eq!(
            rfc3339(at(0) + Duration::from_millis(999), "t").unwrap(),
            "1970-01-01T00:00:00Z"
        );
        assert!(rfc3339(UNIX_EPOCH - Duration::from_secs(1), "t").is_err());
    }

    #[test]
    fn next_microsecond_steps_past_a_time() {
        let next = |s| next_microsecond(s);
        assert_eq!(
            next("2026-10-21T21:48:30.736283Z").as_deref(),
            Some("2026-10-21T21:48:30.736284Z")
        );
        // Carries into the next second, day, month and year.
        assert_eq!(
            next("2024-12-31T23:59:59.999999Z").as_deref(),
            Some("2025-01-01T00:00:00.000000Z")
        );
        // Leap day.
        assert_eq!(
            next("2024-02-28T23:59:59.999999Z").as_deref(),
            Some("2024-02-29T00:00:00.000000Z")
        );
        // Shorter and longer fractions.
        assert_eq!(
            next("2024-01-01T00:00:00Z").as_deref(),
            Some("2024-01-01T00:00:00.000001Z")
        );
        assert_eq!(
            next("2024-01-01T00:00:00.5Z").as_deref(),
            Some("2024-01-01T00:00:00.500001Z")
        );
        assert_eq!(
            next("2024-01-01T00:00:00.123456789Z").as_deref(),
            Some("2024-01-01T00:00:00.123457789Z")
        );
        // Not a UTC RFC 3339 time.
        for bad in [
            "",
            "t1",
            "2024-01-01",
            "2024-01-01T00:00:00+02:00",
            "2024-13-01T00:00:00Z",
            "2024-01-01T00:00:00.x1Z",
        ] {
            assert_eq!(next(bad), None, "{bad}");
        }
    }

    #[test]
    fn days_round_trip() {
        for days in [-719_468, -1, 0, 1, 11_016, 19_723, 2_932_896] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
    }

    #[test]
    fn ranges() {
        assert!(unit_interval("x", 0.0).is_ok());
        assert!(unit_interval("x", 1.0).is_ok());
        assert!(unit_interval("x", 1.01).is_err());
        assert!(unit_interval("x", f64::NAN).is_err());
        assert!(max_duration(0).is_err());
        assert!(max_duration(36_000).is_ok());
        assert!(max_duration(36_001).is_err());
    }
}
