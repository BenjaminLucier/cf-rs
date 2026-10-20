//! Direct creator uploads: one-time URLs that let an end user upload an image
//! straight to Cloudflare without seeing your API token.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::Method;
use reqwest::multipart::Form;
use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::{Error, Result};
use crate::images::Metadata;
use crate::transport::ApiRequest;
use crate::types::ImageId;

/// Direct upload endpoints. Created by [`Client::direct_uploads`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct DirectUploads<'a> {
    client: &'a Client,
}

/// A one-time upload URL.
///
/// The end user uploads with a single `multipart/form-data` `POST` to
/// `upload_url`, with the image in a part named `file`. No authentication is
/// needed for that request.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub struct DirectUpload {
    /// The ID the image will have once uploaded.
    pub id: Option<ImageId>,
    /// The one-time URL to upload to.
    #[serde(rename = "uploadURL")]
    pub upload_url: Option<String>,
}

impl<'a> DirectUploads<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Starts creating a one-time upload URL. Configure it with the returned
    /// builder and call [`send`](DirectUploadRequest::send).
    ///
    /// `POST /accounts/{account_id}/images/v2/direct_upload` (multipart/form-data)
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// use std::time::Duration;
    ///
    /// let upload = client
    ///     .direct_uploads()
    ///     .create()
    ///     .expires_in(Duration::from_secs(15 * 60))
    ///     .metadata(&serde_json::json!({ "user": 42 }))
    ///     .send()
    ///     .await?;
    /// println!("POST the file to {:?}", upload.upload_url);
    /// # Ok(()) }
    /// ```
    pub fn create(&self) -> DirectUploadRequest<'a> {
        DirectUploadRequest {
            client: self.client,
            id: None,
            expiry: None,
            metadata: None,
            require_signed_urls: None,
            creator: None,
        }
    }
}

/// A pending direct upload URL request. Created by [`DirectUploads::create`].
#[must_use = "a direct upload request does nothing until you call `send`"]
#[non_exhaustive]
pub struct DirectUploadRequest<'a> {
    client: &'a Client,
    id: Option<ImageId>,
    expiry: Option<SystemTime>,
    metadata: Option<Metadata>,
    require_signed_urls: Option<bool>,
    creator: Option<String>,
}

impl fmt::Debug for DirectUploadRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DirectUploadRequest")
            .field("id", &self.id)
            .field("expiry", &self.expiry)
            .field("metadata", &self.metadata)
            .field("require_signed_urls", &self.require_signed_urls)
            .field("creator", &self.creator)
            .finish_non_exhaustive()
    }
}

impl DirectUploadRequest<'_> {
    /// Sets a custom ID for the future image (at most 1024 characters, may
    /// contain `/`, can't be a UUID, can't be combined with signed URLs).
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// client.direct_uploads().create().id("avatars/user-42").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn id(mut self, id: impl Into<ImageId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets when the upload URL stops working. Cloudflare accepts between 2
    /// minutes and 6 hours from now; the default is 30 minutes.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// use std::time::{Duration, SystemTime};
    ///
    /// let at = SystemTime::now() + Duration::from_secs(3600);
    /// client.direct_uploads().create().expiry(at).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn expiry(mut self, at: SystemTime) -> Self {
        self.expiry = Some(at);
        self
    }

    /// Sets the expiry relative to now. See [`expiry`](Self::expiry).
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// client
    ///     .direct_uploads()
    ///     .create()
    ///     .expires_in(std::time::Duration::from_secs(600))
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn expires_in(self, ttl: Duration) -> Self {
        self.expiry(SystemTime::now() + ttl)
    }

    /// Attaches user metadata to the future image.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// client.direct_uploads().create().metadata(&serde_json::json!({"k": "v"})).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn metadata<T: Serialize + ?Sized>(mut self, metadata: &T) -> Self {
        self.metadata = Some(Metadata::capture(metadata));
        self
    }

    /// Requires signed URLs to deliver the uploaded image.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// client.direct_uploads().create().require_signed_urls(true).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn require_signed_urls(mut self, required: bool) -> Self {
        self.require_signed_urls = Some(required);
        self
    }

    /// Sets the `creator` field of the future image.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// client.direct_uploads().create().creator("user-42").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn creator(mut self, creator: impl Into<String>) -> Self {
        self.creator = Some(creator.into());
        self
    }

    /// Creates the upload URL.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the ID is empty, the metadata can't be
    /// serialized or the expiry is before 1970; otherwise as for any API call.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let upload = client.direct_uploads().create().send().await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(self) -> Result<DirectUpload> {
        let mut form = Form::new();
        if let Some(id) = self.id {
            crate::types::require_non_empty("image ID", id.as_str())?;
            form = form.text("id", id.into_inner());
        }
        if let Some(at) = self.expiry {
            form = form.text("expiry", rfc3339(at)?);
        }
        if let Some(metadata) = self.metadata {
            form = form.text("metadata", metadata.into_json_string()?);
        }
        if let Some(required) = self.require_signed_urls {
            form = form.text("requireSignedURLs", required.to_string());
        }
        if let Some(creator) = self.creator {
            form = form.text("creator", creator);
        }
        let request = ApiRequest::new(Method::POST, ["v2", "direct_upload"]).multipart(form);
        self.client.inner.json(request).await
    }
}

/// Formats a time as RFC 3339 in UTC with whole seconds, e.g.
/// `2021-09-14T16:00:00Z`.
fn rfc3339(at: SystemTime) -> Result<String> {
    let secs = at
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::InvalidInput("expiry is before 1970".into()))?
        .as_secs();
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    Ok(format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    ))
}

/// Days since 1970-01-01 to (year, month, day) in the proleptic Gregorian
/// calendar. Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_rfc3339() {
        let at = |s| UNIX_EPOCH + Duration::from_secs(s);
        assert_eq!(rfc3339(at(0)).unwrap(), "1970-01-01T00:00:00Z");
        // 2021-09-14T16:00:00Z, the example in Cloudflare's docs.
        assert_eq!(rfc3339(at(1_631_635_200)).unwrap(), "2021-09-14T16:00:00Z");
        // Leap day.
        assert_eq!(rfc3339(at(951_782_400)).unwrap(), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(at(4_102_444_799)).unwrap(), "2099-12-31T23:59:59Z");
        // Sub-second precision is dropped.
        assert_eq!(
            rfc3339(at(0) + Duration::from_millis(999)).unwrap(),
            "1970-01-01T00:00:00Z"
        );
    }
}
