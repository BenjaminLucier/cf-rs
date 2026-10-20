//! Error types.

use std::fmt;
use std::time::Duration;

use serde::Deserialize;

/// A `Result` alias using this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Boxed error used as the source of [`Error::Decode`].
type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Everything that can go wrong when calling the Cloudflare Images API.
///
/// The enum and its struct variants are `#[non_exhaustive]`, so matches need a
/// wildcard arm and `..` in struct patterns:
///
/// ```
/// # fn handle(err: cf_images::Error) {
/// match err {
///     cf_images::Error::Api { status, ref errors, .. } => {
///         eprintln!("Cloudflare said {status}: {errors:?}");
///     }
///     other => eprintln!("{other}"),
/// }
/// # }
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The request couldn't be sent or the response couldn't be read
    /// (connection failure, timeout, TLS error, ...).
    #[error("HTTP request failed")]
    Http(#[source] reqwest::Error),

    /// Cloudflare returned an error: a non-2xx status, or a response
    /// envelope with `"success": false`.
    #[error("Cloudflare API error (HTTP {status}): {}", DisplayErrors(errors))]
    #[non_exhaustive]
    Api {
        /// The HTTP status code.
        status: u16,
        /// The entries of the envelope's `errors` array. Empty if the body
        /// wasn't a Cloudflare error envelope.
        errors: Vec<ApiError>,
        /// The parsed `Retry-After` header, if present.
        retry_after: Option<Duration>,
    },

    /// The response body didn't have the expected shape.
    #[error("failed to decode Cloudflare response (body: {body:?})")]
    #[non_exhaustive]
    Decode {
        /// The first bytes of the response body (truncated, lossy UTF-8).
        body: String,
        /// The underlying deserialization error.
        #[source]
        source: BoxError,
    },

    /// The request was rejected before being sent, for example because of an
    /// empty identifier, unserializable metadata or an invalid base URL.
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

impl Error {
    /// The HTTP status code, if the error came from an API response.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) {
    /// if let Err(err) = client.stats().await {
    ///     eprintln!("status: {:?}", err.status());
    /// }
    /// # }
    /// ```
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Api { status, .. } => Some(*status),
            Self::Http(e) => e.status().map(|s| s.as_u16()),
            _ => None,
        }
    }

    /// The Cloudflare error entries, if any.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) {
    /// if let Err(err) = client.stats().await {
    ///     for e in err.api_errors() { eprintln!("{}: {}", e.code(), e.message()); }
    /// }
    /// # }
    /// ```
    pub fn api_errors(&self) -> &[ApiError] {
        match self {
            Self::Api { errors, .. } => errors,
            _ => &[],
        }
    }

    /// `true` for HTTP 404 (for example, an image or variant that doesn't exist).
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) {
    /// if let Err(err) = client.stats().await {
    ///     if err.is_not_found() { /* treat as already deleted */ }
    /// }
    /// # }
    /// ```
    pub fn is_not_found(&self) -> bool {
        self.status() == Some(404)
    }

    /// `true` for HTTP 429. See [`retry_after`](Self::retry_after).
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) {
    /// if let Err(err) = client.stats().await {
    ///     if err.is_rate_limited() { /* back off */ }
    /// }
    /// # }
    /// ```
    pub fn is_rate_limited(&self) -> bool {
        self.status() == Some(429)
    }

    /// `true` for HTTP 401 or 403. Cloudflare returns 403 for an invalid
    /// token as well as for a token lacking the Images permission.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) {
    /// if let Err(err) = client.stats().await {
    ///     if err.is_unauthorized() { eprintln!("check the API token"); }
    /// }
    /// # }
    /// ```
    pub fn is_unauthorized(&self) -> bool {
        matches!(self.status(), Some(401 | 403))
    }

    /// How long to wait before retrying, from the `Retry-After` header.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) {
    /// if let Err(err) = client.stats().await {
    ///     let wait = err.retry_after().unwrap_or(std::time::Duration::from_secs(1));
    /// }
    /// # }
    /// ```
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Api { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    pub(crate) fn decode(body: &[u8], source: impl Into<BoxError>) -> Self {
        Self::Decode {
            body: snippet(body),
            source: source.into(),
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::Http(err)
    }
}

/// One entry of the `errors` array in a Cloudflare error response, e.g.
/// `{"code": 5404, "message": "Image not found"}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct ApiError {
    #[serde(default)]
    code: u32,
    #[serde(default)]
    message: String,
}

impl ApiError {
    /// The Cloudflare error code.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) {
    /// if let Err(err) = client.stats().await {
    ///     let codes: Vec<u32> = err.api_errors().iter().map(|e| e.code()).collect();
    /// }
    /// # }
    /// ```
    pub fn code(&self) -> u32 {
        self.code
    }

    /// The human-readable message.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) {
    /// if let Err(err) = client.stats().await {
    ///     let first = err.api_errors().first().map(|e| e.message());
    /// }
    /// # }
    /// ```
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

struct DisplayErrors<'a>(&'a [ApiError]);

impl fmt::Display for DisplayErrors<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return f.write_str("no error details");
        }
        for (i, e) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}

const SNIPPET_LEN: usize = 512;

fn snippet(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(&body[..body.len().min(SNIPPET_LEN)]);
    if body.len() > SNIPPET_LEN {
        format!("{text}…")
    } else {
        text.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_lists_errors() {
        let err = Error::Api {
            status: 404,
            errors: vec![ApiError {
                code: 5404,
                message: "Image not found".into(),
            }],
            retry_after: None,
        };
        assert_eq!(
            err.to_string(),
            "Cloudflare API error (HTTP 404): Image not found (5404)"
        );
        assert!(err.is_not_found());
        assert!(!err.is_unauthorized());
    }

    #[test]
    fn snippet_truncates() {
        let long = vec![b'a'; 2000];
        let s = snippet(&long);
        assert!(s.ends_with('…'));
        assert_eq!(s.chars().count(), SNIPPET_LEN + 1);
    }
}
