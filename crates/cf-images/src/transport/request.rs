//! Description of a single API call, independent of how it is executed.

use reqwest::Method;
use reqwest::multipart::Form;

/// The body of an API request.
pub(crate) enum Body {
    Empty,
    Json(serde_json::Value),
    Multipart(Form),
}

/// One API call. `segments` are relative to `accounts/{account_id}/images/`
/// and are percent-encoded individually, so an ID containing `/` stays one
/// path segment.
pub(crate) struct ApiRequest {
    pub(crate) method: Method,
    pub(crate) segments: Vec<String>,
    pub(crate) query: Vec<(String, String)>,
    pub(crate) body: Body,
}

impl ApiRequest {
    pub(crate) fn new<I, S>(method: Method, segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            method,
            segments: segments.into_iter().map(Into::into).collect(),
            query: Vec::new(),
            body: Body::Empty,
        }
    }

    pub(crate) fn get<I: IntoIterator<Item = S>, S: Into<String>>(segments: I) -> Self {
        Self::new(Method::GET, segments)
    }

    pub(crate) fn query(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.query.push((key.into(), value.to_string()));
        self
    }

    pub(crate) fn query_opt(self, key: &str, value: Option<impl ToString>) -> Self {
        match value {
            Some(v) => self.query(key, v),
            None => self,
        }
    }

    pub(crate) fn json(mut self, body: serde_json::Value) -> Self {
        self.body = Body::Json(body);
        self
    }

    pub(crate) fn multipart(mut self, form: Form) -> Self {
        self.body = Body::Multipart(form);
        self
    }
}
