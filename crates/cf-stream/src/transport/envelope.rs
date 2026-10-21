//! Parsing of Cloudflare's `{success, errors, messages, result}` envelope.

use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::{ApiError, Error, Result};

/// The standard v4 response envelope. Only the fields this crate uses are
/// modelled; everything else (`messages`, `result_info`, ...) is ignored.
#[derive(Deserialize)]
struct Envelope<T> {
    success: Option<bool>,
    #[serde(default)]
    errors: Vec<ApiError>,
    result: Option<T>,
    /// Remaining items after this page (video list only).
    #[serde(default, deserialize_with = "lenient_count")]
    range: Option<u64>,
    /// Total matching items (video list with `include_counts` only).
    #[serde(default, deserialize_with = "lenient_count")]
    total: Option<u64>,
}

/// The top-level `range` and `total` fields of a list response.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PageInfo {
    pub(crate) range: Option<u64>,
    pub(crate) total: Option<u64>,
}

/// The error-relevant part of an envelope, parsed without looking at `result`.
#[derive(Deserialize)]
struct ErrorEnvelope {
    success: Option<bool>,
    #[serde(default)]
    errors: Vec<ApiError>,
}

/// Turns a response into `T`, the envelope's `result`.
///
/// A missing or `null` result is accepted only when `T` can be deserialized
/// from `null` (e.g. [`serde::de::IgnoredAny`]); otherwise it is a decode
/// error.
pub(crate) fn parse<T: DeserializeOwned>(
    status: StatusCode,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<T> {
    parse_page(status, headers, body).map(|(result, _)| result)
}

/// Like [`parse`], but also returns the envelope's `range` and `total`.
pub(crate) fn parse_page<T: DeserializeOwned>(
    status: StatusCode,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(T, PageInfo)> {
    if !status.is_success() {
        return Err(api_error(status, headers, body));
    }
    let envelope: Envelope<T> = serde_json::from_slice(body).map_err(|e| Error::decode(body, e))?;
    if envelope.success == Some(false) {
        return Err(Error::Api {
            status: status.as_u16(),
            errors: envelope.errors,
            retry_after: retry_after(headers),
        });
    }
    let info = PageInfo {
        range: envelope.range,
        total: envelope.total,
    };
    let result = match envelope.result {
        Some(result) => result,
        // A missing or `null` result is fine for types that accept `null`.
        None => serde_json::from_value(serde_json::Value::Null)
            .map_err(|e| Error::decode(body, format!("missing `result`: {e}")))?,
    };
    Ok((result, info))
}

/// For endpoints whose successful response is ignored (deletes, live input
/// enable/disable). Any 2xx response is a success unless it is a JSON
/// envelope with `"success": false`.
///
/// API quirk (observed 2026-10-21): delete responses differ by endpoint and
/// from the API reference. A video delete returns 200 with an empty body;
/// key, caption, watermark, live input and output deletes return an
/// envelope with `"result": ""`; others return `"result": "ok"`. The
/// reference also lists a bare `{}` for live input and output deletes,
/// which is accepted too.
pub(crate) fn check(status: StatusCode, headers: &HeaderMap, body: &[u8]) -> Result<()> {
    if !status.is_success() {
        return Err(api_error(status, headers, body));
    }
    match serde_json::from_slice::<ErrorEnvelope>(body) {
        Ok(envelope) if envelope.success == Some(false) => Err(Error::Api {
            status: status.as_u16(),
            errors: envelope.errors,
            retry_after: retry_after(headers),
        }),
        _ => Ok(()),
    }
}

/// Builds an [`Error::Api`] from an error response, parsing the envelope's
/// `errors` if the body is one.
pub(crate) fn api_error(status: StatusCode, headers: &HeaderMap, body: &[u8]) -> Error {
    let errors = serde_json::from_slice::<ErrorEnvelope>(body)
        .map(|e| e.errors)
        .unwrap_or_default();
    Error::Api {
        status: status.as_u16(),
        errors,
        retry_after: retry_after(headers),
    }
}

/// For a non-JSON success response (WebVTT captions), detects a JSON error
/// envelope served with a 2xx status.
pub(crate) fn embedded_error(
    status: StatusCode,
    headers: &HeaderMap,
    body: &[u8],
) -> Option<Error> {
    let is_json = headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("application/json"));
    if !is_json {
        return None;
    }
    let envelope = serde_json::from_slice::<ErrorEnvelope>(body).ok()?;
    (envelope.success == Some(false)).then(|| Error::Api {
        status: status.as_u16(),
        errors: envelope.errors,
        retry_after: retry_after(headers),
    })
}

/// Accepts a count given as a JSON number or a numeric string; anything else
/// (including `null`) is `None`, so an unexpected shape never fails a
/// response that is otherwise fine.
fn lenient_count<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Option<u64>, D::Error> {
    Ok(match serde_json::Value::deserialize(de)? {
        serde_json::Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64)),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    })
}

/// Parses `Retry-After` given in seconds. The HTTP-date form isn't used by
/// the Cloudflare API and is ignored.
fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::IgnoredAny;

    fn ok() -> StatusCode {
        StatusCode::OK
    }

    #[test]
    fn parses_result() {
        let body = br#"{"success":true,"errors":[],"messages":[],"result":{"a":1},"extra":true}"#;
        let v: serde_json::Value = parse(ok(), &HeaderMap::new(), body).unwrap();
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn null_result_is_fine_for_ignored_any() {
        let body = br#"{"success":true,"errors":[],"messages":[],"result":null}"#;
        let _: IgnoredAny = parse(ok(), &HeaderMap::new(), body).unwrap();
        let body = br#"{"success":true,"errors":[],"messages":[],"result":{}}"#;
        let _: IgnoredAny = parse(ok(), &HeaderMap::new(), body).unwrap();
        let body = br#"{"success":true,"errors":[],"messages":[],"result":""}"#;
        let _: IgnoredAny = parse(ok(), &HeaderMap::new(), body).unwrap();
    }

    #[test]
    fn null_result_is_an_error_for_structs() {
        #[derive(Debug, Deserialize)]
        struct S {
            #[allow(dead_code)]
            a: u32,
        }
        let body = br#"{"success":true,"errors":[],"messages":[],"result":null}"#;
        let err = parse::<S>(ok(), &HeaderMap::new(), body).unwrap_err();
        assert!(matches!(err, Error::Decode { .. }), "{err:?}");
    }

    #[test]
    fn success_false_with_200_is_an_api_error() {
        let body = br#"{"success":false,"errors":[{"code":5400,"message":"bad"}],"messages":[],"result":null}"#;
        let err = parse::<IgnoredAny>(ok(), &HeaderMap::new(), body).unwrap_err();
        assert_eq!(err.status(), Some(200));
        assert_eq!(err.api_errors()[0].code(), 5400);
    }

    #[test]
    fn non_json_error_body_still_gives_status() {
        let err =
            parse::<IgnoredAny>(StatusCode::BAD_GATEWAY, &HeaderMap::new(), b"<html>").unwrap_err();
        assert_eq!(err.status(), Some(502));
        assert!(err.api_errors().is_empty());
    }

    #[test]
    fn page_info() {
        let body =
            br#"{"success":true,"errors":[],"messages":[],"result":[],"range":3,"total":"10"}"#;
        let (_, info): (Vec<serde_json::Value>, _) =
            parse_page(ok(), &HeaderMap::new(), body).unwrap();
        assert_eq!(info.range, Some(3));
        assert_eq!(info.total, Some(10));

        let body = br#"{"success":true,"result":[],"range":{"odd":1},"total":null}"#;
        let (_, info): (Vec<serde_json::Value>, _) =
            parse_page(ok(), &HeaderMap::new(), body).unwrap();
        assert_eq!(info, PageInfo::default());
    }

    #[test]
    fn check_accepts_any_success_body() {
        for body in [
            &b""[..],
            b"{}",
            b"not json",
            br#"{"success":true,"errors":[],"messages":[],"result":"ok"}"#,
            br#"{"success":true,"errors":[],"messages":[],"result":""}"#,
        ] {
            check(ok(), &HeaderMap::new(), body).unwrap();
        }
        check(StatusCode::NO_CONTENT, &HeaderMap::new(), b"").unwrap();
    }

    #[test]
    fn check_rejects_failures() {
        let body = br#"{"success":false,"errors":[{"code":10003,"message":"nope"}]}"#;
        let err = check(ok(), &HeaderMap::new(), body).unwrap_err();
        assert_eq!(err.api_errors()[0].code(), 10003);
        let err = check(StatusCode::NOT_FOUND, &HeaderMap::new(), b"").unwrap_err();
        assert!(err.is_not_found());
    }

    #[test]
    fn retry_after_seconds() {
        let mut h = HeaderMap::new();
        h.insert(reqwest::header::RETRY_AFTER, "30".parse().unwrap());
        let err = api_error(StatusCode::TOO_MANY_REQUESTS, &h, b"{}");
        assert!(err.is_rate_limited());
        assert_eq!(err.retry_after(), Some(Duration::from_secs(30)));
    }
}
