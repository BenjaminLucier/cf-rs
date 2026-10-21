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
    match envelope.result {
        Some(result) => Ok(result),
        // A missing or `null` result is fine for types that accept `null`.
        None => serde_json::from_value(serde_json::Value::Null)
            .map_err(|e| Error::decode(body, format!("missing `result`: {e}"))),
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

/// For a non-JSON success response (the blob endpoint), detects a JSON error
/// envelope served with a 2xx status.
///
/// Not verified live: this is a defensive check; a JSON error with a 2xx
/// status has never been observed on the blob endpoint. It only applies to
/// `application/json` responses, so it can't misread image bytes.
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
    fn retry_after_seconds() {
        let mut h = HeaderMap::new();
        h.insert(reqwest::header::RETRY_AFTER, "30".parse().unwrap());
        let err = api_error(StatusCode::TOO_MANY_REQUESTS, &h, b"{}");
        assert!(err.is_rate_limited());
        assert_eq!(err.retry_after(), Some(Duration::from_secs(30)));
    }
}
