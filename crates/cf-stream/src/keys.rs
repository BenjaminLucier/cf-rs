//! Signing key endpoints. Signing keys sign playback tokens for videos that
//! require signed URLs (see `TokenSigner`, feature `signing`).

use std::fmt;

use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;
use crate::transport::ApiRequest;
use crate::types::SigningKeyId;

/// Signing key endpoints. Created by [`Client::signing_keys`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct SigningKeys<'a> {
    client: &'a Client,
}

/// A newly created signing key, including its private key material.
///
/// Cloudflare returns the key material **only once**, when the key is
/// created; store it securely. Its `Debug` output never contains it.
#[derive(Clone, Deserialize)]
#[non_exhaustive]
pub struct SigningKey {
    /// The key ID (the `kid` of tokens signed with it).
    pub id: Option<SigningKeyId>,
    /// When the key was created.
    pub created: Option<String>,
    /// The RSA private key as base64-encoded PEM. Use [`pem`](Self::pem)
    /// to read it.
    pub pem: Option<SecretString>,
    /// The RSA private key as a base64-encoded JWK. Use [`jwk`](Self::jwk)
    /// to read it.
    pub jwk: Option<SecretString>,
}

impl SigningKey {
    /// Exposes the base64-encoded PEM private key, e.g. to store it or to
    /// build a `TokenSigner` (feature `signing`).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let key = client.signing_keys().create().await?;
    /// let pem = key.pem().unwrap_or_default();
    /// # Ok(()) }
    /// ```
    pub fn pem(&self) -> Option<&str> {
        self.pem.as_ref().map(|v| v.expose_secret())
    }

    /// Exposes the base64-encoded JWK private key.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let key = client.signing_keys().create().await?;
    /// let jwk = key.jwk().unwrap_or_default();
    /// # Ok(()) }
    /// ```
    pub fn jwk(&self) -> Option<&str> {
        self.jwk.as_ref().map(|v| v.expose_secret())
    }
}

impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningKey")
            .field("id", &self.id)
            .field("created", &self.created)
            .field("pem", &self.pem.as_ref().map(|_| "[REDACTED]"))
            .field("jwk", &self.jwk.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

/// A signing key as listed (without key material).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub struct SigningKeyInfo {
    /// The key ID.
    pub id: Option<SigningKeyId>,
    /// When the key was created.
    pub created: Option<String>,
}

impl<'a> SigningKeys<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Creates a signing key. The returned key material can't be retrieved
    /// again. An account can have up to 1000 keys.
    ///
    /// `POST /accounts/{account_id}/stream/keys`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let key = client.signing_keys().create().await?;
    /// println!("created key {:?}", key.id);
    /// # Ok(()) }
    /// ```
    pub async fn create(&self) -> Result<SigningKey> {
        let request = ApiRequest::new(Method::POST, ["keys"]);
        self.client.inner.json(request).await
    }

    /// Lists the account's signing keys.
    ///
    /// `GET /accounts/{account_id}/stream/keys`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// for key in client.signing_keys().list().await? {
    ///     println!("{:?} {:?}", key.id, key.created);
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn list(&self) -> Result<Vec<SigningKeyInfo>> {
        // API quirk (observed 2026-10-21): with no keys, `result` is `null`
        // rather than `[]`.
        let list: Option<Vec<SigningKeyInfo>> =
            self.client.inner.json(ApiRequest::get(["keys"])).await?;
        Ok(list.unwrap_or_default())
    }

    /// Deletes a signing key. Tokens signed with it stop working.
    ///
    /// `DELETE /accounts/{account_id}/stream/keys/{key_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// client.signing_keys().delete(&"ee15edd5e5e8bb0a7c0b1a0c3a8b1c35".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn delete(&self, id: &SigningKeyId) -> Result<()> {
        let request = ApiRequest::new(Method::DELETE, ["keys", id.as_str()]);
        self.client.inner.empty(request).await
    }
}
