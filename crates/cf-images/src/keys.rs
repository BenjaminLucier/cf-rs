//! Signing key endpoints. Signing keys sign delivery URLs for private images
//! (see `UrlSigner`, feature `signing`).

use std::fmt;

use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use crate::client::Client;
use crate::error::Result;
use crate::transport::ApiRequest;
use crate::types::SigningKeyName;

/// Signing key endpoints. Created by [`Client::signing_keys`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct SigningKeys<'a> {
    client: &'a Client,
}

/// A signing key. Its `Debug` output never contains the key value.
#[derive(Clone, Deserialize)]
#[non_exhaustive]
pub struct SigningKey {
    /// Key name.
    pub name: Option<SigningKeyName>,
    /// Key value (secret). Use [`value`](Self::value) to read it.
    pub value: Option<SecretString>,
}

impl SigningKey {
    /// Exposes the secret key value, e.g. to build a `UrlSigner` (feature
    /// `signing`).
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let keys = client.signing_keys().list().await?;
    /// let secret = keys[0].value().unwrap_or_default();
    /// # Ok(()) }
    /// ```
    pub fn value(&self) -> Option<&str> {
        self.value.as_ref().map(|v| v.expose_secret())
    }
}

impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningKey")
            .field("name", &self.name)
            .field("value", &self.value.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

#[derive(Deserialize)]
struct KeyList {
    #[serde(default)]
    keys: Vec<SigningKey>,
}

impl<'a> SigningKeys<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Lists the account's signing keys.
    ///
    /// `GET /accounts/{account_id}/images/v1/keys`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// for key in client.signing_keys().list().await? {
    ///     println!("{:?}", key.name);
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn list(&self) -> Result<Vec<SigningKey>> {
        let list: KeyList = self
            .client
            .inner
            .json(ApiRequest::get(["v1", "keys"]))
            .await?;
        Ok(list.keys)
    }

    /// Creates a signing key (or rotates it, if one with this name exists).
    /// Returns all of the account's keys afterwards.
    ///
    /// `PUT /accounts/{account_id}/images/v1/keys/{signing_key_name}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let keys = client.signing_keys().create(&"backend".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn create(&self, name: &SigningKeyName) -> Result<Vec<SigningKey>> {
        let request = ApiRequest::new(Method::PUT, ["v1", "keys", name.as_str()]);
        let list: KeyList = self.client.inner.json(request).await?;
        Ok(list.keys)
    }

    /// Deletes a signing key. Returns the remaining keys.
    ///
    /// `DELETE /accounts/{account_id}/images/v1/keys/{signing_key_name}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let remaining = client.signing_keys().delete(&"backend".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn delete(&self, name: &SigningKeyName) -> Result<Vec<SigningKey>> {
        let request = ApiRequest::new(Method::DELETE, ["v1", "keys", name.as_str()]);
        let list: KeyList = self.client.inner.json(request).await?;
        Ok(list.keys)
    }
}
