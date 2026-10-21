//! Direct creator uploads (one-time upload URLs that let an end user upload
//! a video straight to Cloudflare without seeing your API token) and tus
//! resumable upload creation.

use std::time::{Duration, SystemTime};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::Map;

use crate::client::Client;
use crate::error::{Error, Result};
use crate::options::{VideoOptions, video_option_setters};
use crate::transport::ApiRequest;
use crate::types::{VideoId, WatermarkId};
use crate::watermarks::Watermark;

/// Direct upload endpoints. Created by [`Client::direct_uploads`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct DirectUploads<'a> {
    client: &'a Client,
}

/// A one-time upload URL.
///
/// The end user uploads with a single `multipart/form-data` `POST` to
/// `upload_url`, with the video (at most 200 MB) in a part named `file`. No
/// authentication is needed for that request. The URL also accepts tus
/// uploads for larger files.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DirectUpload {
    /// The ID the video will have once uploaded.
    pub uid: Option<VideoId>,
    /// The one-time URL to upload to.
    #[serde(rename = "uploadURL")]
    pub upload_url: Option<String>,
    /// When the video will be deleted automatically, if scheduled.
    pub scheduled_deletion: Option<String>,
    /// The watermark profile that will be applied.
    pub watermark: Option<Watermark>,
}

/// A resumable (tus) upload created by [`DirectUploads::create_tus`].
///
/// Send the file to [`location`](Self::location) with any
/// [tus 1.0](https://tus.io/protocols/resumable-upload) client: `PATCH`
/// requests with `Tus-Resumable: 1.0.0`, `Upload-Offset` and
/// `Content-Type: application/offset+octet-stream`, in chunks of at least
/// 5 MiB that are multiples of 256 KiB (except the last). Unless the upload
/// was created with [`direct_user`](TusUploadRequest::direct_user), every
/// request must also carry your API token as `Authorization: Bearer ...`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TusUpload {
    /// The upload URL (the `Location` response header). It is on a
    /// Cloudflare host, currently
    /// `edge-production.gateway.api.cloudflare.com` (observed 2026-10-21),
    /// not `api.cloudflare.com`.
    pub location: Url,
    /// The ID the video will have (the `stream-media-id` response header).
    pub video_id: Option<VideoId>,
}

impl<'a> DirectUploads<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Starts creating a one-time upload URL for a video of at most
    /// `max_duration_secs` seconds (1 to 36 000). The duration is reserved
    /// against the account's storage until the upload is processed or the
    /// URL expires. Configure the request with the returned builder and call
    /// [`send`](DirectUploadRequest::send).
    ///
    /// `POST /accounts/{account_id}/stream/direct_upload`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use std::time::Duration;
    ///
    /// let upload = client
    ///     .direct_uploads()
    ///     .create(3600)
    ///     .expires_in(Duration::from_secs(30 * 60))
    ///     .creator("user-42")
    ///     .send()
    ///     .await?;
    /// println!("POST the file to {:?}", upload.upload_url);
    /// # Ok(()) }
    /// ```
    pub fn create(&self, max_duration_secs: u32) -> DirectUploadRequest<'a> {
        DirectUploadRequest {
            client: self.client,
            max_duration_secs,
            expiry: None,
            options: VideoOptions::default(),
        }
    }

    /// Starts creating a resumable [tus](https://tus.io) upload of
    /// `upload_length` bytes, for files over 200 MB. This only creates the
    /// upload; send the data with any tus client (see [`TusUpload`]).
    ///
    /// `POST /accounts/{account_id}/stream` (tus creation)
    ///
    /// With [`direct_user(true)`](TusUploadRequest::direct_user) the
    /// returned URL is a one-time URL an end user can upload to without
    /// your API token, e.g. from tus-js-client or Uppy in a browser.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let upload = client
    ///     .direct_uploads()
    ///     .create_tus(1_500_000_000)
    ///     .name("keynote.mp4")
    ///     .max_duration_seconds(7200)
    ///     .send()
    ///     .await?;
    /// println!("PATCH chunks to {} for video {:?}", upload.location, upload.video_id);
    /// # Ok(()) }
    /// ```
    pub fn create_tus(&self, upload_length: u64) -> TusUploadRequest<'a> {
        TusUploadRequest {
            client: self.client,
            upload_length,
            direct_user: None,
            creator: None,
            name: None,
            require_signed_urls: None,
            max_duration_secs: None,
            expiry: None,
            allowed_origins: None,
            thumbnail_timestamp_pct: None,
            watermark: None,
            scheduled_deletion: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Direct upload URL

/// A pending direct upload URL request. Created by [`DirectUploads::create`].
#[derive(Clone, Debug)]
#[must_use = "a direct upload request does nothing until you call `send`"]
#[non_exhaustive]
pub struct DirectUploadRequest<'a> {
    client: &'a Client,
    max_duration_secs: u32,
    expiry: Option<SystemTime>,
    options: VideoOptions,
}

impl DirectUploadRequest<'_> {
    video_option_setters!("client\n    .direct_uploads()\n    .create(3600)");

    /// Sets when the upload URL stops working (the API default is 30
    /// minutes from now).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use std::time::{Duration, SystemTime};
    ///
    /// let at = SystemTime::now() + Duration::from_secs(3600);
    /// let upload = client.direct_uploads().create(600).expiry(at).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn expiry(mut self, at: SystemTime) -> Self {
        self.expiry = Some(at);
        self
    }

    /// Sets the expiry relative to now. See [`expiry`](Self::expiry).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let upload = client
    ///     .direct_uploads()
    ///     .create(600)
    ///     .expires_in(std::time::Duration::from_secs(600))
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn expires_in(self, ttl: Duration) -> Self {
        self.expiry(SystemTime::now() + ttl)
    }

    /// Creates the upload URL.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the maximum duration is out of range or a
    /// setting is invalid; otherwise as for any API call.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let upload = client.direct_uploads().create(600).send().await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(self) -> Result<DirectUpload> {
        let mut body = Map::new();
        body.insert(
            "maxDurationSeconds".into(),
            crate::util::max_duration(self.max_duration_secs)?.into(),
        );
        if let Some(at) = self.expiry {
            body.insert("expiry".into(), crate::util::rfc3339(at, "expiry")?.into());
        }
        self.options.write(&mut body)?;
        let request = ApiRequest::new(Method::POST, ["direct_upload"]).json(body.into());
        self.client.inner.json(request).await
    }
}

// ---------------------------------------------------------------------------
// tus creation

/// A pending tus upload creation. Created by [`DirectUploads::create_tus`].
#[derive(Clone, Debug)]
#[must_use = "a tus upload request does nothing until you call `send`"]
#[non_exhaustive]
pub struct TusUploadRequest<'a> {
    client: &'a Client,
    upload_length: u64,
    direct_user: Option<bool>,
    creator: Option<String>,
    name: Option<String>,
    require_signed_urls: Option<bool>,
    max_duration_secs: Option<u32>,
    expiry: Option<SystemTime>,
    allowed_origins: Option<Vec<String>>,
    thumbnail_timestamp_pct: Option<f64>,
    watermark: Option<WatermarkId>,
    scheduled_deletion: Option<SystemTime>,
}

impl TusUploadRequest<'_> {
    /// Creates a one-time URL for an end user (the `direct_user=true` query
    /// parameter). The upload to that URL needs no API token.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let upload = client
    ///     .direct_uploads()
    ///     .create_tus(1_500_000_000)
    ///     .direct_user(true)
    ///     .max_duration_seconds(3600)
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn direct_user(mut self, direct: bool) -> Self {
        self.direct_user = Some(direct);
        self
    }

    /// Sets the video's `creator` (the `Upload-Creator` header, at most 64
    /// characters).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let upload = client.direct_uploads().create_tus(1 << 30).creator("user-42").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn creator(mut self, creator: impl Into<String>) -> Self {
        self.creator = Some(creator.into());
        self
    }

    /// Names the video (`meta.name`).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let upload = client.direct_uploads().create_tus(1 << 30).name("talk.mp4").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Requires a signed token to play the video.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let upload = client
    ///     .direct_uploads()
    ///     .create_tus(1 << 30)
    ///     .require_signed_urls(true)
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn require_signed_urls(mut self, required: bool) -> Self {
        self.require_signed_urls = Some(required);
        self
    }

    /// Rejects videos longer than this (1 to 36 000 seconds). Recommended
    /// for end-user uploads.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let upload = client
    ///     .direct_uploads()
    ///     .create_tus(1 << 30)
    ///     .max_duration_seconds(600)
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn max_duration_seconds(mut self, secs: u32) -> Self {
        self.max_duration_secs = Some(secs);
        self
    }

    /// Sets when an end-user upload URL stops working.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use std::time::{Duration, SystemTime};
    ///
    /// let upload = client
    ///     .direct_uploads()
    ///     .create_tus(1 << 30)
    ///     .direct_user(true)
    ///     .expiry(SystemTime::now() + Duration::from_secs(3600))
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn expiry(mut self, at: SystemTime) -> Self {
        self.expiry = Some(at);
        self
    }

    /// Restricts the origins allowed to embed the video.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let upload = client
    ///     .direct_uploads()
    ///     .create_tus(1 << 30)
    ///     .allowed_origins(["example.com"])
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn allowed_origins<I>(mut self, origins: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.allowed_origins = Some(origins.into_iter().map(Into::into).collect());
        self
    }

    /// Takes the default thumbnail at this fraction of the duration (`0.0`
    /// to `1.0`).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let upload = client
    ///     .direct_uploads()
    ///     .create_tus(1 << 30)
    ///     .thumbnail_timestamp_pct(0.5)
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn thumbnail_timestamp_pct(mut self, pct: f64) -> Self {
        self.thumbnail_timestamp_pct = Some(pct);
        self
    }

    /// Applies a watermark profile.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let upload = client
    ///     .direct_uploads()
    ///     .create_tus(1 << 30)
    ///     .watermark("ea95132c15732412d22c1476fa83f27a")
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn watermark(mut self, id: impl Into<WatermarkId>) -> Self {
        self.watermark = Some(id.into());
        self
    }

    /// Deletes the video automatically at this time (at least 30 days after
    /// upload).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use std::time::{Duration, SystemTime};
    ///
    /// let upload = client
    ///     .direct_uploads()
    ///     .create_tus(1 << 30)
    ///     .scheduled_deletion(SystemTime::now() + Duration::from_secs(90 * 86_400))
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn scheduled_deletion(mut self, at: SystemTime) -> Self {
        self.scheduled_deletion = Some(at);
        self
    }

    /// The `Upload-Metadata` header value: comma-separated `key value`
    /// pairs with base64-encoded values, and bare keys for flags.
    fn upload_metadata(&self) -> Result<Option<String>> {
        let mut pairs: Vec<String> = Vec::new();
        let mut push = |key: &str, value: &str| {
            pairs.push(format!("{key} {}", BASE64.encode(value)));
        };
        if let Some(name) = &self.name {
            push("name", name);
        }
        if let Some(secs) = self.max_duration_secs {
            push(
                "maxdurationseconds",
                &crate::util::max_duration(secs)?.to_string(),
            );
        }
        if let Some(at) = self.expiry {
            push("expiry", &crate::util::rfc3339(at, "expiry")?);
        }
        if let Some(origins) = &self.allowed_origins {
            push("allowedorigins", &origins.join(","));
        }
        if let Some(pct) = self.thumbnail_timestamp_pct {
            let pct = crate::util::unit_interval("thumbnail timestamp", pct)?;
            push("thumbnailtimestamppct", &pct.to_string());
        }
        if let Some(watermark) = &self.watermark {
            crate::types::require_non_empty("watermark ID", watermark.as_str())?;
            push("watermark", watermark.as_str());
        }
        if let Some(at) = self.scheduled_deletion {
            push(
                "scheduleddeletion",
                &crate::util::rfc3339(at, "scheduled deletion")?,
            );
        }
        if self.require_signed_urls == Some(true) {
            pairs.push("requiresignedurls".into());
        }
        Ok((!pairs.is_empty()).then(|| pairs.join(",")))
    }

    /// Creates the upload.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the length is zero or a setting is
    /// invalid; [`Error::Decode`] if the response has no valid `Location`
    /// header; otherwise as for any API call.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let upload = client.direct_uploads().create_tus(1 << 30).send().await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(self) -> Result<TusUpload> {
        if self.upload_length == 0 {
            return Err(Error::InvalidInput(
                "upload length must be greater than zero".into(),
            ));
        }
        let mut request = ApiRequest::new(Method::POST, Vec::<String>::new())
            .header("tus-resumable", "1.0.0")
            .header("upload-length", self.upload_length.to_string())
            .query_opt("direct_user", self.direct_user.filter(|d| *d));
        if let Some(metadata) = self.upload_metadata()? {
            request = request.header("upload-metadata", metadata);
        }
        if let Some(creator) = &self.creator {
            request = request.header("upload-creator", creator.clone());
        }
        let headers = self.client.inner.headers(request).await?;
        parse_tus_headers(&headers)
    }
}

fn parse_tus_headers(headers: &reqwest::header::HeaderMap) -> Result<TusUpload> {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let location = header("location")
        .ok_or_else(|| Error::decode(b"", "tus response has no `Location` header"))?;
    let location = Url::parse(location).map_err(|e| {
        Error::decode(
            location.as_bytes(),
            format!("invalid `Location` header: {e}"),
        )
    })?;
    let video_id = header("stream-media-id")
        .filter(|id| !id.is_empty())
        .map(VideoId::new);
    Ok(TusUpload { location, video_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    fn request(client: &Client) -> TusUploadRequest<'_> {
        client.direct_uploads().create_tus(10)
    }

    fn decode(value: &str) -> String {
        String::from_utf8(BASE64.decode(value).unwrap()).unwrap()
    }

    #[test]
    fn metadata_encoding() {
        let client = Client::new("acc", "tok").unwrap();
        assert_eq!(request(&client).upload_metadata().unwrap(), None);

        let metadata = request(&client)
            .name("my video, final.mp4")
            .max_duration_seconds(600)
            .expiry(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
            .allowed_origins(["a.com", "b.com"])
            .require_signed_urls(true)
            .upload_metadata()
            .unwrap()
            .unwrap();
        assert!(!metadata.contains(", "), "{metadata}");
        let pairs: Vec<&str> = metadata.split(',').collect();
        assert_eq!(pairs.len(), 5, "{metadata}");
        let value = |key: &str| {
            pairs
                .iter()
                .find_map(|p| p.strip_prefix(&format!("{key} ")))
                .map(decode)
        };
        assert_eq!(value("name").unwrap(), "my video, final.mp4");
        assert_eq!(value("maxdurationseconds").unwrap(), "600");
        assert_eq!(value("expiry").unwrap(), "2023-11-14T22:13:20Z");
        assert_eq!(value("allowedorigins").unwrap(), "a.com,b.com");
        assert_eq!(pairs.last(), Some(&"requiresignedurls"));

        let without_flag = request(&client)
            .require_signed_urls(false)
            .upload_metadata()
            .unwrap();
        assert_eq!(without_flag, None);
        assert!(
            request(&client)
                .max_duration_seconds(0)
                .upload_metadata()
                .is_err()
        );
    }

    #[test]
    fn tus_headers() {
        let mut headers = reqwest::header::HeaderMap::new();
        assert!(matches!(
            parse_tus_headers(&headers),
            Err(Error::Decode { .. })
        ));
        headers.insert("location", "/relative".parse().unwrap());
        assert!(matches!(
            parse_tus_headers(&headers),
            Err(Error::Decode { .. })
        ));
        headers.insert(
            "location",
            "https://upload.cloudflarestream.com/tus/abc?tusv2=true"
                .parse()
                .unwrap(),
        );
        let upload = parse_tus_headers(&headers).unwrap();
        assert_eq!(upload.video_id, None);
        headers.insert("stream-media-id", "abc".parse().unwrap());
        let upload = parse_tus_headers(&headers).unwrap();
        assert_eq!(upload.video_id.unwrap(), "abc");
        assert_eq!(
            upload.location.host_str(),
            Some("upload.cloudflarestream.com")
        );
    }
}
