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
