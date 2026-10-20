//! The API client and its builder.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use secrecy::SecretString;

use crate::direct_upload::DirectUploads;
use crate::error::{Error, Result};
use crate::images::Images;
use crate::keys::SigningKeys;
use crate::transport::Transport;
use crate::types::AccountId;
use crate::variants::Variants;

/// The default API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.cloudflare.com/client/v4/";

const USER_AGENT: &str = concat!("cf-images/", env!("CARGO_PKG_VERSION"));

/// A Cloudflare Images API client for one account.
///
/// `Client` is cheap to clone (it is an `Arc` inside) and can be shared
/// across tasks. Endpoints are grouped by resource:
/// [`images`](Self::images), [`variants`](Self::variants),
/// [`signing_keys`](Self::signing_keys),
/// [`direct_uploads`](Self::direct_uploads) and [`stats`](Self::stats).
///
/// ```no_run
/// # async fn run() -> cf_images::Result<()> {
/// let client = cf_images::Client::new("023e105f4ecef8ad9ca31a8372d0c353", "api-token")?;
/// let stats = client.stats().await?;
/// # Ok(()) }
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub struct Client {
    pub(crate) inner: Arc<Transport>,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("account_id", self.inner.account_id())
            .field("base_url", &self.inner.base_url().as_str())
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Starts building a client for `account_id`, authenticating with an API
    /// token that has the **Account > Cloudflare Images > Edit** (or Read)
    /// permission.
    ///
    /// ```
    /// # fn run() -> cf_images::Result<()> {
    /// let client = cf_images::Client::builder("023e105f4ecef8ad9ca31a8372d0c353", "api-token")
    ///     .timeout(std::time::Duration::from_secs(30))
    ///     .build()?;
    /// # Ok(()) }
    /// ```
    pub fn builder(
        account_id: impl Into<AccountId>,
        api_token: impl Into<SecretString>,
    ) -> ClientBuilder {
        ClientBuilder {
            account_id: account_id.into(),
            token: api_token.into(),
            base_url: None,
            timeout: None,
        }
    }

    /// Builds a client with default settings. Shorthand for
    /// `Client::builder(account_id, api_token).build()`.
    ///
    /// ```
    /// # fn run() -> cf_images::Result<()> {
    /// let client = cf_images::Client::new("023e105f4ecef8ad9ca31a8372d0c353", "api-token")?;
    /// # Ok(()) }
    /// ```
    pub fn new(
        account_id: impl Into<AccountId>,
        api_token: impl Into<SecretString>,
    ) -> Result<Self> {
        Self::builder(account_id, api_token).build()
    }

    /// The account this client operates on.
    ///
    /// ```
    /// # fn run() -> cf_images::Result<()> {
    /// let client = cf_images::Client::new("023e105f4ecef8ad9ca31a8372d0c353", "api-token")?;
    /// assert_eq!(client.account_id(), "023e105f4ecef8ad9ca31a8372d0c353");
    /// # Ok(()) }
    /// ```
    pub fn account_id(&self) -> &AccountId {
        self.inner.account_id()
    }

    /// Image endpoints: upload, get, update, delete, list and download.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let page = client.images().list().send().await?;
    /// # Ok(()) }
    /// ```
    pub fn images(&self) -> Images<'_> {
        Images::new(self)
    }

    /// Variant endpoints.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let variants = client.variants().list().await?;
    /// # Ok(()) }
    /// ```
    pub fn variants(&self) -> Variants<'_> {
        Variants::new(self)
    }

    /// Signing key endpoints.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let keys = client.signing_keys().list().await?;
    /// # Ok(()) }
    /// ```
    pub fn signing_keys(&self) -> SigningKeys<'_> {
        SigningKeys::new(self)
    }

    /// Direct creator upload endpoints.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let upload = client.direct_uploads().create().send().await?;
    /// # Ok(()) }
    /// ```
    pub fn direct_uploads(&self) -> DirectUploads<'_> {
        DirectUploads::new(self)
    }
}

/// Configures and builds a [`Client`]. Created by [`Client::builder`].
#[must_use]
#[non_exhaustive]
pub struct ClientBuilder {
    account_id: AccountId,
    token: SecretString,
    base_url: Option<String>,
    timeout: Option<Duration>,
}

impl fmt::Debug for ClientBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientBuilder")
            .field("account_id", &self.account_id)
            .field("token", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl ClientBuilder {
    /// Overrides the API base URL (default [`DEFAULT_BASE_URL`]). Useful for
    /// tests against a mock server. A missing trailing slash is added.
    ///
    /// ```
    /// # fn run() -> cf_images::Result<()> {
    /// let client = cf_images::Client::builder("account-id", "api-token")
    ///     .base_url("http://127.0.0.1:8080/client/v4")
    ///     .build()?;
    /// # Ok(()) }
    /// ```
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Sets a timeout for each request, covering connecting, sending the
    /// request and reading the whole response body. No timeout is set by
    /// default.
    ///
    /// ```
    /// # fn run() -> cf_images::Result<()> {
    /// let client = cf_images::Client::builder("account-id", "api-token")
    ///     .timeout(std::time::Duration::from_secs(60))
    ///     .build()?;
    /// # Ok(()) }
    /// ```
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Builds the client.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the account ID is empty or the base URL is
    /// invalid; [`Error::Http`] if the HTTP client can't be built.
    ///
    /// ```
    /// # fn run() -> cf_images::Result<()> {
    /// let client = cf_images::Client::builder("account-id", "api-token").build()?;
    /// # Ok(()) }
    /// ```
    pub fn build(self) -> Result<Client> {
        crate::types::require_non_empty("account ID", self.account_id.as_str())?;
        let base_url = parse_base_url(self.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL))?;
        let mut http = reqwest::Client::builder().user_agent(USER_AGENT);
        if let Some(timeout) = self.timeout {
            http = http.timeout(timeout);
        }
        let http = http.build()?;
        Ok(Client {
            inner: Arc::new(Transport::new(http, base_url, self.account_id, self.token)),
        })
    }
}

fn parse_base_url(raw: &str) -> Result<Url> {
    let mut url = Url::parse(raw)
        .map_err(|e| Error::InvalidInput(format!("invalid base URL {raw:?}: {e}")))?;
    if url.cannot_be_a_base() || !matches!(url.scheme(), "http" | "https") {
        return Err(Error::InvalidInput(format!(
            "base URL must be an http(s) URL, got {raw:?}"
        )));
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_gets_trailing_slash() {
        assert_eq!(
            parse_base_url("http://localhost:1/client/v4")
                .unwrap()
                .as_str(),
            "http://localhost:1/client/v4/"
        );
        assert!(parse_base_url("mailto:x@y").is_err());
        assert!(parse_base_url("not a url").is_err());
    }

    #[test]
    fn debug_never_prints_token() {
        let builder = Client::builder("acc", "super-secret-token");
        assert!(!format!("{builder:?}").contains("super-secret-token"));
        let client = builder.build().unwrap();
        let out = format!("{client:?} {:?}", client.inner);
        assert!(!out.contains("super-secret-token"), "{out}");
    }

    #[test]
    fn empty_account_id_is_rejected() {
        assert!(matches!(
            Client::new("", "t").unwrap_err(),
            Error::InvalidInput(_)
        ));
    }
}
