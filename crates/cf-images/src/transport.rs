//! Request execution: URL building, authentication and response parsing.
//!
//! Nothing here is part of the public API.

mod envelope;
mod request;

use std::fmt;

use bytes::Bytes;
use reqwest::Url;
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;

pub(crate) use request::ApiRequest;
use request::Body;

use crate::error::{Error, Result};
use crate::types::AccountId;

/// Executes [`ApiRequest`]s for one account: builds the URL, authenticates
/// and parses the response.
pub(crate) struct Transport {
    http: reqwest::Client,
    base_url: Url,
    account_id: AccountId,
    token: SecretString,
}

impl fmt::Debug for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transport")
            .field("base_url", &self.base_url.as_str())
            .field("account_id", &self.account_id)
            .field("token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl Transport {
    pub(crate) fn new(
        http: reqwest::Client,
        base_url: Url,
        account_id: AccountId,
        token: SecretString,
    ) -> Self {
        Self {
            http,
            base_url,
            account_id,
            token,
        }
    }

    pub(crate) fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    pub(crate) fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Sends a request and returns the envelope's `result`.
    pub(crate) async fn json<T: DeserializeOwned>(&self, request: ApiRequest) -> Result<T> {
        let (status, headers, body) = self.send(request).await?;
        envelope::parse(status, &headers, &body)
    }

    /// Sends a request whose successful response is raw bytes, not an envelope.
    pub(crate) async fn raw(&self, request: ApiRequest) -> Result<Bytes> {
        let (status, headers, body) = self.send(request).await?;
        if !status.is_success() {
            return Err(envelope::api_error(status, &headers, &body));
        }
        match envelope::embedded_error(status, &headers, &body) {
            Some(err) => Err(err),
            None => Ok(body),
        }
    }

    /// Sends a request and returns the raw status, headers and body.
    async fn send(
        &self,
        request: ApiRequest,
    ) -> Result<(reqwest::StatusCode, reqwest::header::HeaderMap, Bytes)> {
        let url = self.url(&request)?;
        #[cfg(feature = "tracing")]
        let span = tracing::debug_span!(
            "cf_images.request",
            http.method = %request.method,
            url.path = %url.path(),
            http.status = tracing::field::Empty,
        );

        let mut builder = self
            .http
            .request(request.method, url)
            .bearer_auth(self.token.expose_secret());
        builder = match request.body {
            Body::Empty => builder,
            Body::Json(value) => builder.json(&value),
            Body::Multipart(form) => builder.multipart(form),
        };

        let fut = async move {
            let response = builder.send().await?;
            let status = response.status();
            let headers = response.headers().clone();
            let body = response.bytes().await?;
            Ok::<_, Error>((status, headers, body))
        };

        #[cfg(feature = "tracing")]
        let fut = {
            use tracing::Instrument as _;
            async move {
                let out = fut.await;
                match &out {
                    Ok((status, _, _)) => {
                        tracing::Span::current().record("http.status", status.as_u16());
                    }
                    Err(err) => tracing::debug!(error = %err, "request failed"),
                }
                out
            }
            .instrument(span)
        };

        fut.await
    }

    /// The full URL: `{base}/accounts/{account_id}/images/{segments}?{query}`.
    fn url(&self, request: &ApiRequest) -> Result<Url> {
        for segment in &request.segments {
            crate::types::require_non_empty("identifier in the request path", segment)?;
        }
        let mut url = self.base_url.clone();
        {
            let mut path = url
                .path_segments_mut()
                // `ClientBuilder::build` only accepts http(s) URLs, which are always bases.
                .map_err(|()| Error::InvalidInput("base URL cannot be a base".into()))?;
            path.pop_if_empty()
                .extend(["accounts", self.account_id.as_str(), "images"])
                .extend(&request.segments);
        }
        if !request.query.is_empty() {
            url.query_pairs_mut().extend_pairs(&request.query);
        }
        Ok(url)
    }
}
