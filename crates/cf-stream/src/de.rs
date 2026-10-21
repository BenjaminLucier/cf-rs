//! Lenient deserializers for fields whose wire type isn't reliable.
//!
//! API quirk: the OpenAPI schema types many integers as `number`, and the
//! API reference examples show `-1` for "unknown" sizes and durations, so
//! integers are accepted as JSON integers, floats or numeric strings rather
//! than failing a whole response over one field.

use serde::{Deserialize, Deserializer};

/// A non-negative integer sent as a JSON integer, float or numeric string.
/// Anything else (including `null`) is `None`.
pub(crate) fn lenient_u64<'de, D: Deserializer<'de>>(de: D) -> Result<Option<u64>, D::Error> {
    Ok(match serde_json::Value::deserialize(de)? {
        serde_json::Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64)),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    })
}

/// An integer that may be negative (`-1` means "unknown"), sent as a JSON
/// integer, float or numeric string. Anything else is `None`.
pub(crate) fn lenient_i64<'de, D: Deserializer<'de>>(de: D) -> Result<Option<i64>, D::Error> {
    Ok(match serde_json::Value::deserialize(de)? {
        serde_json::Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct S {
        #[serde(default, deserialize_with = "lenient_u64")]
        u: Option<u64>,
        #[serde(default, deserialize_with = "lenient_i64")]
        i: Option<i64>,
    }

    fn parse(json: &str) -> (Option<u64>, Option<i64>) {
        let s: S = serde_json::from_str(json).unwrap();
        (s.u, s.i)
    }

    #[test]
    fn lenient_numbers() {
        assert_eq!(parse(r#"{"u":5,"i":-1}"#), (Some(5), Some(-1)));
        assert_eq!(parse(r#"{"u":5.0,"i":2.9}"#), (Some(5), Some(2)));
        assert_eq!(parse(r#"{"u":"7","i":"-3"}"#), (Some(7), Some(-3)));
        assert_eq!(parse(r#"{"u":-1,"i":null}"#), (None, None));
        assert_eq!(parse(r#"{"u":{},"i":[]}"#), (None, None));
        assert_eq!(parse(r#"{}"#), (None, None));
    }
}
