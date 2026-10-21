//! The account's video webhook: Cloudflare POSTs a [`Video`](crate::Video)
//! to it when a video finishes processing (successfully or not).

use std::fmt;

use reqwest::{Method, Url};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::json;

use crate::client::Client;
use crate::error::Result;
use crate::transport::ApiRequest;

/// Webhook endpoints. Created by [`Client::webhooks`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Webhooks<'a> {
    client: &'a Client,
}

/// The account's webhook subscription. Its `Debug` output never contains
/// the secret.
#[derive(Clone, Deserialize)]
#[serde(from = "RawWebhook")]
#[non_exhaustive]
pub struct Webhook {
    /// Where notifications are sent.
    pub notification_url: Option<String>,
    /// When the subscription was last changed.
    pub modified: Option<String>,
    /// The secret for verifying notifications. Use
    /// [`secret`](Self::secret) to read it, or pass it to
    /// `WebhookVerifier` (feature `webhooks`).
    pub secret: Option<SecretString>,
}

/// API quirk: the API sends the URL as `notificationUrl`,
/// `notification_url`, or both (the reference example shows both), so a
/// plain `#[serde(alias)]` would fail with a duplicate-field error.
#[derive(Deserialize)]
struct RawWebhook {
    #[serde(rename = "notificationUrl")]
    camel: Option<String>,
    #[serde(rename = "notification_url")]
    snake: Option<String>,
    modified: Option<String>,
    secret: Option<SecretString>,
}

impl From<RawWebhook> for Webhook {
    fn from(raw: RawWebhook) -> Self {
        Self {
            notification_url: raw.camel.or(raw.snake),
            modified: raw.modified,
            secret: raw.secret,
        }
    }
}

impl Webhook {
    /// Exposes the signing secret.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let webhook = client.webhooks().get().await?;
    /// let secret = webhook.secret().unwrap_or_default();
    /// # Ok(()) }
    /// ```
    pub fn secret(&self) -> Option<&str> {
        self.secret.as_ref().map(|s| s.expose_secret())
    }
}

impl fmt::Debug for Webhook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Webhook")
            .field("notification_url", &self.notification_url)
            .field("modified", &self.modified)
            .field("secret", &self.secret.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl<'a> Webhooks<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Fetches the webhook subscription.
    ///
    /// With no subscription, Cloudflare answers 404 ("No active webhook
    /// found"), so the error's [`is_not_found`](crate::Error::is_not_found)
    /// is `true`.
    ///
    /// `GET /accounts/{account_id}/stream/webhook`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let webhook = client.webhooks().get().await?;
    /// println!("{:?}", webhook.notification_url);
    /// # Ok(()) }
    /// ```
    pub async fn get(&self) -> Result<Webhook> {
        self.client.inner.json(ApiRequest::get(["webhook"])).await
    }

    /// Subscribes `url` to notifications, replacing any existing
    /// subscription (an account has one). The URL must be publicly
    /// reachable over HTTP(S).
    ///
    /// `PUT /accounts/{account_id}/stream/webhook`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let url = "https://example.com/stream-webhook".parse().expect("valid URL");
    /// let webhook = client.webhooks().set(url).await?;
    /// // Store webhook.secret() to verify notifications.
    /// # Ok(()) }
    /// ```
    pub async fn set(&self, url: Url) -> Result<Webhook> {
        let request = ApiRequest::new(Method::PUT, ["webhook"])
            .json(json!({ "notificationUrl": url.as_str() }));
        self.client.inner.json(request).await
    }

    /// Removes the webhook subscription.
    ///
    /// `DELETE /accounts/{account_id}/stream/webhook`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// client.webhooks().delete().await?;
    /// # Ok(()) }
    /// ```
    pub async fn delete(&self) -> Result<()> {
        let request = ApiRequest::new(Method::DELETE, ["webhook"]);
        self.client.inner.empty(request).await
    }
}
