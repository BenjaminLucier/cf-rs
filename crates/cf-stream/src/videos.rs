//! Video endpoints: details, update, delete, listing, uploads from files and
//! URLs, clipping, storage usage and signed tokens.

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use futures_core::Stream;
use reqwest::multipart::{Form, Part};
use reqwest::{Method, Url};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::access::AccessRule;
use crate::client::Client;
use crate::error::{Error, Result};
use crate::options::{VideoOptions, video_option_setters};
use crate::transport::ApiRequest;
use crate::types::{LiveInputId, VideoId};
use crate::util::{Metadata, Patch};
use crate::watermarks::Watermark;

/// Video endpoints. Created by [`Client::videos`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Videos<'a> {
    client: &'a Client,
}

// ---------------------------------------------------------------------------
// Models

/// A video stored in Cloudflare Stream.
///
/// All fields are optional because the API marks them optional. Timestamps
/// are RFC 3339 strings as returned by the API.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Video {
    /// The video's unique identifier.
    pub uid: Option<VideoId>,
    /// User metadata. `meta.name` is the name shown in the dashboard.
    pub meta: Option<Value>,
    /// User-defined identifier of the media creator.
    pub creator: Option<String>,
    /// Processing status.
    pub status: Option<VideoStatus>,
    /// `true` once the video can be played. Lower quality levels can still be
    /// encoding; see [`VideoStatus::pct_complete`].
    pub ready_to_stream: Option<bool>,
    /// When the video became playable.
    pub ready_to_stream_at: Option<String>,
    /// Duration in seconds, or `-1` while unknown. See
    /// [`duration`](Self::duration).
    pub duration: Option<f64>,
    /// Dimensions of the uploaded file.
    pub input: Option<VideoInput>,
    /// Size of the uploaded file in bytes.
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub size: Option<u64>,
    /// HLS and DASH manifest URLs.
    pub playback: Option<Playback>,
    /// The preview page URL. Omitted until encoding is complete.
    pub preview: Option<String>,
    /// The default thumbnail URL. Omitted until encoding is complete.
    pub thumbnail: Option<String>,
    /// Where the default thumbnail is taken, as a fraction of the duration.
    pub thumbnail_timestamp_pct: Option<f64>,
    /// Whether playback requires a signed token.
    #[serde(rename = "requireSignedURLs")]
    pub require_signed_urls: Option<bool>,
    /// Origins allowed to embed the video; empty means any origin.
    pub allowed_origins: Option<Vec<String>>,
    /// When the video was created.
    pub created: Option<String>,
    /// When the video was last modified.
    pub modified: Option<String>,
    /// When the upload finished.
    pub uploaded: Option<String>,
    /// When a direct upload URL for this video expires.
    pub upload_expiry: Option<String>,
    /// When the video will be deleted automatically.
    pub scheduled_deletion: Option<String>,
    /// The upload size limit for a pending direct upload.
    #[serde(default, deserialize_with = "crate::de::lenient_i64")]
    pub max_size_bytes: Option<i64>,
    /// The duration limit for a pending upload, in seconds.
    #[serde(default, deserialize_with = "crate::de::lenient_i64")]
    pub max_duration_seconds: Option<i64>,
    /// The video this one was clipped from.
    pub clipped_from: Option<VideoId>,
    /// The live input that recorded this video.
    pub live_input: Option<LiveInputId>,
    /// The watermark profile applied to the video.
    pub watermark: Option<Watermark>,
    /// Public details shown by the Stream player.
    pub public_details: Option<PublicDetails>,
}

impl Video {
    /// The duration, or `None` while it is unknown (`-1`).
    ///
    /// ```
    /// let video: cf_stream::Video = serde_json::from_str(r#"{"duration": 5.5}"#).unwrap();
    /// assert_eq!(video.duration(), Some(std::time::Duration::from_millis(5500)));
    /// let video: cf_stream::Video = serde_json::from_str(r#"{"duration": -1}"#).unwrap();
    /// assert_eq!(video.duration(), None);
    /// ```
    pub fn duration(&self) -> Option<Duration> {
        self.duration
            .filter(|d| d.is_finite() && *d >= 0.0)
            .map(Duration::from_secs_f64)
    }
}

/// Processing status of a [`Video`].
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct VideoStatus {
    /// The processing state.
    pub state: Option<VideoState>,
    /// Encoding progress from `0` to `100`, as sent by the API (a string).
    /// See [`percent_complete`](Self::percent_complete).
    pub pct_complete: Option<String>,
    /// A machine-readable reason when `state` is `error`, such as
    /// `ERR_NON_VIDEO`. The API sends an empty string otherwise; see
    /// [`error_reason`](Self::error_reason).
    pub error_reason_code: Option<String>,
    /// A human-readable (English) reason when `state` is `error`.
    pub error_reason_text: Option<String>,
}

impl VideoStatus {
    /// Encoding progress from `0.0` to `100.0`, parsed from
    /// [`pct_complete`](Self::pct_complete).
    ///
    /// ```
    /// let status: cf_stream::VideoStatus =
    ///     serde_json::from_str(r#"{"pctComplete": "39.000000"}"#).unwrap();
    /// assert_eq!(status.percent_complete(), Some(39.0));
    /// ```
    pub fn percent_complete(&self) -> Option<f64> {
        self.pct_complete
            .as_deref()?
            .trim()
            .parse()
            .ok()
            .filter(|p: &f64| p.is_finite())
    }

    /// The error reason code, or `None` if there is none.
    ///
    /// ```
    /// let status: cf_stream::VideoStatus =
    ///     serde_json::from_str(r#"{"state": "ready", "errorReasonCode": ""}"#).unwrap();
    /// assert_eq!(status.error_reason(), None);
    /// ```
    pub fn error_reason(&self) -> Option<&str> {
        self.error_reason_code.as_deref().filter(|c| !c.is_empty())
    }
}

/// The processing state of a video.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[non_exhaustive]
pub enum VideoState {
    /// Waiting for an upload (e.g. a direct upload URL not used yet).
    #[serde(rename = "pendingupload")]
    PendingUpload,
    /// Cloudflare is downloading the video from a URL.
    #[serde(rename = "downloading")]
    Downloading,
    /// Queued for encoding.
    #[serde(rename = "queued")]
    Queued,
    /// Encoding.
    #[serde(rename = "inprogress")]
    InProgress,
    /// Ready to stream.
    #[serde(rename = "ready")]
    Ready,
    /// Processing failed; see [`VideoStatus::error_reason`].
    #[serde(rename = "error")]
    Error,
    /// A live recording that is still being broadcast.
    #[serde(rename = "live-inprogress")]
    LiveInProgress,
    /// A value this version of the crate doesn't recognize. Can't be sent.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// Whether a video was uploaded or recorded from a live stream, for
/// [`ListVideosRequest::kind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum VideoKind {
    /// Video on demand.
    Vod,
    /// A live recording.
    Live,
    /// A value this version of the crate doesn't recognize. Can't be sent.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// Dimensions of the uploaded file; `-1` while unknown.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub struct VideoInput {
    /// Width in pixels.
    #[serde(default, deserialize_with = "crate::de::lenient_i64")]
    pub width: Option<i64>,
    /// Height in pixels.
    #[serde(default, deserialize_with = "crate::de::lenient_i64")]
    pub height: Option<i64>,
}

/// Manifest URLs for adaptive playback.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub struct Playback {
    /// HLS manifest (`.../manifest/video.m3u8`).
    pub hls: Option<String>,
    /// DASH manifest (`.../manifest/video.mpd`).
    pub dash: Option<String>,
}

/// Public details shown by the Stream player.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub struct PublicDetails {
    /// Title.
    pub title: Option<String>,
    /// Share link.
    pub share_link: Option<String>,
    /// Channel link.
    pub channel_link: Option<String>,
    /// Logo URL.
    pub logo: Option<String>,
    /// Media ID.
    #[serde(default, deserialize_with = "crate::de::lenient_i64")]
    pub media_id: Option<i64>,
}

/// Storage used by the account (or one creator).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct StorageUsage {
    /// The creator the numbers are filtered by, if any.
    pub creator: Option<String>,
    /// Minutes of video stored.
    pub total_storage_minutes: Option<f64>,
    /// The account's storage limit in minutes.
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub total_storage_minutes_limit: Option<u64>,
    /// Number of videos.
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub video_count: Option<u64>,
}

#[derive(Deserialize)]
struct TokenResponse {
    token: SecretString,
}

// ---------------------------------------------------------------------------
// Endpoints

impl<'a> Videos<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Fetches a video's details.
    ///
    /// `GET /accounts/{account_id}/stream/{video_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let video = client.videos().get(&"ea95132c15732412d22c1476fa83f27a".into()).await?;
    /// println!("{:?}", video.status);
    /// # Ok(()) }
    /// ```
    pub async fn get(&self, id: &VideoId) -> Result<Video> {
        let request = ApiRequest::get([id.as_str()]);
        self.client.inner.json(request).await
    }

    /// Updates a video's settings.
    ///
    /// `POST /accounts/{account_id}/stream/{video_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use cf_stream::UpdateVideo;
    ///
    /// let video = client
    ///     .videos()
    ///     .update(
    ///         &"ea95132c15732412d22c1476fa83f27a".into(),
    ///         UpdateVideo::new().require_signed_urls(true),
    ///     )
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn update(&self, id: &VideoId, update: UpdateVideo) -> Result<Video> {
        let body = update.into_json()?;
        let request = ApiRequest::new(Method::POST, [id.as_str()]).json(body);
        self.client.inner.json(request).await
    }

    /// Deletes a video and its captions, downloads and audio tracks.
    ///
    /// `DELETE /accounts/{account_id}/stream/{video_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// client.videos().delete(&"ea95132c15732412d22c1476fa83f27a".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn delete(&self, id: &VideoId) -> Result<()> {
        // API quirk: answers 200 with an empty body (no envelope); see
        // `envelope::check`.
        let request = ApiRequest::new(Method::DELETE, [id.as_str()]);
        self.client.inner.empty(request).await
    }

    /// Lists videos, newest first, up to 1000 per request. Configure the
    /// request with the returned builder; use
    /// [`list_all`](Self::list_all) or [`ListVideosRequest::into_stream`] to
    /// get every video.
    ///
    /// `GET /accounts/{account_id}/stream`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use cf_stream::VideoState;
    ///
    /// let page = client
    ///     .videos()
    ///     .list()
    ///     .status(VideoState::Ready)
    ///     .limit(100)
    ///     .include_counts(true)
    ///     .send()
    ///     .await?;
    /// println!("{} of {:?}", page.videos.len(), page.total);
    /// # Ok(()) }
    /// ```
    pub fn list(&self) -> ListVideosRequest<'a> {
        ListVideosRequest {
            client: self.client,
            after: None,
            before: None,
            asc: None,
            creator: None,
            ids: Vec::new(),
            names: Vec::new(),
            live_input: None,
            search: None,
            video_name: None,
            status: None,
            kind: None,
            limit: None,
            include_counts: None,
        }
    }

    /// Streams every video, newest first. To set filters, build the request
    /// with [`list`](Self::list) and call [`ListVideosRequest::into_stream`].
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use futures_util::TryStreamExt;
    ///
    /// let all: Vec<cf_stream::Video> = client.videos().list_all().try_collect().await?;
    /// # Ok(()) }
    /// ```
    pub fn list_all(&self) -> impl Stream<Item = Result<Video>> + Send + 'a {
        self.list().into_stream()
    }

    /// Starts a basic upload of a video file (at most 200 MB). Larger files
    /// need a resumable [tus upload](crate::DirectUploads::create_tus).
    ///
    /// `POST /accounts/{account_id}/stream` (multipart/form-data)
    ///
    /// Basic uploads don't accept settings; apply them afterwards with
    /// [`update`](Self::update).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use cf_stream::UploadSource;
    ///
    /// let data = std::fs::read("intro.mp4").expect("read file");
    /// let video = client.videos().upload(UploadSource::bytes(data, "intro.mp4")).send().await?;
    /// println!("{:?}", video.uid);
    /// # Ok(()) }
    /// ```
    pub fn upload(&self, source: UploadSource) -> UploadRequest<'a> {
        UploadRequest {
            client: self.client,
            source,
        }
    }

    /// Asks Cloudflare to download a video from a URL. The server must be
    /// publicly reachable and support `HEAD` and range requests.
    ///
    /// `POST /accounts/{account_id}/stream/copy`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let url = "https://example.com/intro.mp4".parse().expect("valid URL");
    /// let video = client
    ///     .videos()
    ///     .copy(url)
    ///     .meta(&serde_json::json!({ "name": "intro" }))
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn copy(&self, url: Url) -> CopyRequest<'a> {
        CopyRequest {
            client: self.client,
            url,
            options: VideoOptions::default(),
        }
    }

    /// Creates a new video from part of an existing one, between
    /// `start_secs` and `end_secs`.
    ///
    /// `POST /accounts/{account_id}/stream/clip`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let clip = client
    ///     .videos()
    ///     .clip(&"ea95132c15732412d22c1476fa83f27a".into(), 10, 25)
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn clip(&self, source: &VideoId, start_secs: u32, end_secs: u32) -> ClipRequest<'a> {
        ClipRequest {
            client: self.client,
            source: source.clone(),
            start_secs,
            end_secs,
            options: VideoOptions::default(),
        }
    }

    /// Reports the storage used by the account's videos.
    ///
    /// `GET /accounts/{account_id}/stream/storage-usage`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let usage = client.videos().storage_usage().send().await?;
    /// println!("{:?} minutes", usage.total_storage_minutes);
    /// # Ok(()) }
    /// ```
    pub fn storage_usage(&self) -> StorageUsageRequest<'a> {
        StorageUsageRequest {
            client: self.client,
            creator: None,
        }
    }

    /// Asks Cloudflare to create a signed playback token for a video that
    /// requires signed URLs. The token replaces the video ID in playback
    /// URLs (see [`Delivery`](crate::Delivery)).
    ///
    /// `POST /accounts/{account_id}/stream/{video_id}/token`
    ///
    /// This endpoint is rate-limited and meant for testing or low volumes
    /// (fewer than 1000 tokens a day). For production, sign tokens locally
    /// with `TokenSigner` (feature `signing`).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use secrecy::ExposeSecret;
    /// use std::time::Duration;
    ///
    /// let token = client
    ///     .videos()
    ///     .create_token(&"ea95132c15732412d22c1476fa83f27a".into())
    ///     .expires_in(Duration::from_secs(3600))
    ///     .send()
    ///     .await?;
    /// println!("{}", token.expose_secret());
    /// # Ok(()) }
    /// ```
    pub fn create_token(&self, id: &VideoId) -> TokenRequest<'a> {
        TokenRequest {
            client: self.client,
            id: id.clone(),
            expires_at: None,
            not_before: None,
            downloadable: None,
            original: None,
            access_rules: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Update

/// Changes to apply with [`Videos::update`]. Unset fields are left unchanged.
///
/// ```
/// let update = cf_stream::UpdateVideo::new()
///     .meta(&serde_json::json!({ "name": "Launch keynote" }))
///     .require_signed_urls(true);
/// ```
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct UpdateVideo {
    meta: Option<Metadata>,
    creator: Option<String>,
    require_signed_urls: Option<bool>,
    allowed_origins: Option<Vec<String>>,
    thumbnail_timestamp_pct: Option<f64>,
    max_duration_seconds: Option<u32>,
    scheduled_deletion: Option<Patch<SystemTime>>,
    upload_expiry: Option<SystemTime>,
}

impl UpdateVideo {
    /// An update that changes nothing; add fields with the setters.
    ///
    /// ```
    /// let update = cf_stream::UpdateVideo::new();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the video's metadata. Serialization errors are reported when
    /// the update is sent.
    ///
    /// ```
    /// let update = cf_stream::UpdateVideo::new().meta(&serde_json::json!({ "name": "v2" }));
    /// ```
    pub fn meta<T: Serialize + ?Sized>(mut self, meta: &T) -> Self {
        self.meta = Some(Metadata::capture(meta));
        self
    }

    /// Changes the `creator` field.
    ///
    /// ```
    /// let update = cf_stream::UpdateVideo::new().creator("user-2");
    /// ```
    pub fn creator(mut self, creator: impl Into<String>) -> Self {
        self.creator = Some(creator.into());
        self
    }

    /// Changes whether playback requires a signed token.
    ///
    /// ```
    /// let update = cf_stream::UpdateVideo::new().require_signed_urls(true);
    /// ```
    pub fn require_signed_urls(mut self, required: bool) -> Self {
        self.require_signed_urls = Some(required);
        self
    }

    /// Replaces the origins allowed to embed the video. An empty list allows
    /// any origin.
    ///
    /// ```
    /// let update = cf_stream::UpdateVideo::new().allowed_origins(["example.com"]);
    /// ```
    pub fn allowed_origins<I>(mut self, origins: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.allowed_origins = Some(origins.into_iter().map(Into::into).collect());
        self
    }

    /// Moves the default thumbnail to this fraction of the duration (`0.0`
    /// to `1.0`).
    ///
    /// ```
    /// let update = cf_stream::UpdateVideo::new().thumbnail_timestamp_pct(0.25);
    /// ```
    pub fn thumbnail_timestamp_pct(mut self, pct: f64) -> Self {
        self.thumbnail_timestamp_pct = Some(pct);
        self
    }

    /// Limits the duration of a video that hasn't been uploaded yet (1 to
    /// 36 000 seconds).
    ///
    /// ```
    /// let update = cf_stream::UpdateVideo::new().max_duration_seconds(600);
    /// ```
    pub fn max_duration_seconds(mut self, secs: u32) -> Self {
        self.max_duration_seconds = Some(secs);
        self
    }

    /// Deletes the video automatically at this time (at least 30 days after
    /// upload).
    ///
    /// ```
    /// use std::time::{Duration, SystemTime};
    ///
    /// let update = cf_stream::UpdateVideo::new()
    ///     .scheduled_deletion(SystemTime::now() + Duration::from_secs(60 * 86_400));
    /// ```
    pub fn scheduled_deletion(mut self, at: SystemTime) -> Self {
        self.scheduled_deletion = Some(Patch::Set(at));
        self
    }

    /// Cancels a scheduled deletion (sends `"scheduledDeletion": null`).
    ///
    /// ```
    /// let update = cf_stream::UpdateVideo::new().clear_scheduled_deletion();
    /// ```
    pub fn clear_scheduled_deletion(mut self) -> Self {
        self.scheduled_deletion = Some(Patch::Clear);
        self
    }

    /// Changes when a pending direct upload URL expires.
    ///
    /// ```
    /// use std::time::{Duration, SystemTime};
    ///
    /// let update = cf_stream::UpdateVideo::new().upload_expiry(SystemTime::now() + Duration::from_secs(3600));
    /// ```
    pub fn upload_expiry(mut self, at: SystemTime) -> Self {
        self.upload_expiry = Some(at);
        self
    }

    fn into_json(self) -> Result<Value> {
        let mut body = Map::new();
        if let Some(meta) = self.meta {
            body.insert("meta".into(), meta.into_value()?);
        }
        if let Some(creator) = self.creator {
            body.insert("creator".into(), creator.into());
        }
        if let Some(required) = self.require_signed_urls {
            body.insert("requireSignedURLs".into(), required.into());
        }
        if let Some(origins) = self.allowed_origins {
            body.insert("allowedOrigins".into(), origins.into());
        }
        if let Some(pct) = self.thumbnail_timestamp_pct {
            let pct = crate::util::unit_interval("thumbnail timestamp", pct)?;
            body.insert("thumbnailTimestampPct".into(), pct.into());
        }
        if let Some(secs) = self.max_duration_seconds {
            body.insert(
                "maxDurationSeconds".into(),
                crate::util::max_duration(secs)?.into(),
            );
        }
        match self.scheduled_deletion {
            Some(Patch::Set(at)) => {
                let at = crate::util::rfc3339(at, "scheduled deletion")?;
                body.insert("scheduledDeletion".into(), at.into());
            }
            Some(Patch::Clear) => {
                body.insert("scheduledDeletion".into(), Value::Null);
            }
            None => {}
        }
        if let Some(at) = self.upload_expiry {
            let at = crate::util::rfc3339(at, "upload expiry")?;
            body.insert("uploadExpiry".into(), at.into());
        }
        Ok(body.into())
    }
}

// ---------------------------------------------------------------------------
// List

/// One page of videos.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct VideoPage {
    /// The videos on this page.
    pub videos: Vec<Video>,
    /// How many more videos match after this page, if reported.
    pub range: Option<u64>,
    /// How many videos match in total. Only reported with
    /// [`ListVideosRequest::include_counts`].
    pub total: Option<u64>,
}

/// The list endpoint's `result`: a bare array, or (with `include_counts`)
/// an object holding the videos and the counts.
///
/// API quirk (observed 2026-10-21): the API reference documents a bare
/// array with `range`/`total` beside `result`; with `include_counts` the
/// live API nests them as `{videos, total, range}` instead. Both shapes are
/// accepted, and the top-level counts are read as a fallback.
#[derive(Deserialize)]
#[serde(untagged)]
enum ListResult {
    Plain(Vec<Video>),
    Counted {
        #[serde(default)]
        videos: Vec<Video>,
        #[serde(default, deserialize_with = "crate::de::lenient_u64")]
        range: Option<u64>,
        #[serde(default, deserialize_with = "crate::de::lenient_u64")]
        total: Option<u64>,
    },
}

/// A creation-time bound for a list request.
#[derive(Clone, Debug)]
enum Bound {
    At(SystemTime),
    /// A `created` value from a previous page, passed through unchanged to
    /// keep its full precision.
    Raw(String),
}

impl Bound {
    fn render(&self, what: &str) -> Result<String> {
        match self {
            Self::At(at) => crate::util::rfc3339(*at, what),
            Self::Raw(raw) => Ok(raw.clone()),
        }
    }
}

/// A video listing request. Created by [`Videos::list`].
#[derive(Clone, Debug)]
#[must_use = "a list request does nothing until you call `send` or `into_stream`"]
#[non_exhaustive]
pub struct ListVideosRequest<'a> {
    client: &'a Client,
    after: Option<Bound>,
    before: Option<Bound>,
    asc: Option<bool>,
    creator: Option<String>,
    ids: Vec<VideoId>,
    names: Vec<String>,
    live_input: Option<LiveInputId>,
    search: Option<String>,
    video_name: Option<String>,
    status: Option<VideoState>,
    kind: Option<VideoKind>,
    limit: Option<u32>,
    include_counts: Option<bool>,
}

/// The page size the API uses when no limit is set.
const DEFAULT_LIMIT: u32 = 1000;

impl<'a> ListVideosRequest<'a> {
    /// Only videos created at or after this time (the `after` parameter).
    /// The bound is **inclusive**: a video created exactly at `at` is
    /// returned.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use std::time::{Duration, SystemTime};
    ///
    /// let since = SystemTime::now() - Duration::from_secs(7 * 86_400);
    /// let page = client.videos().list().after(since).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn after(mut self, at: SystemTime) -> Self {
        self.after = Some(Bound::At(at));
        self
    }

    /// Only videos created before this time (the `before` parameter). The
    /// bound is **exclusive**: a video created exactly at `at` is not
    /// returned.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use std::time::{Duration, SystemTime};
    ///
    /// let until = SystemTime::now() - Duration::from_secs(86_400);
    /// let page = client.videos().list().before(until).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn before(mut self, at: SystemTime) -> Self {
        self.before = Some(Bound::At(at));
        self
    }

    /// Oldest first instead of newest first.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let page = client.videos().list().asc(true).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn asc(mut self, asc: bool) -> Self {
        self.asc = Some(asc);
        self
    }

    /// Only videos with this `creator`.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let page = client.videos().list().creator("user-42").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn creator(mut self, creator: impl Into<String>) -> Self {
        self.creator = Some(creator.into());
        self
    }

    /// Only these videos (the `id` parameter, comma-separated).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let page = client
    ///     .videos()
    ///     .list()
    ///     .ids(["ea95132c15732412d22c1476fa83f27a", "fc0a8dc887b16759bfd9ad922230a014"])
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn ids<I>(mut self, ids: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<VideoId>,
    {
        self.ids.extend(ids.into_iter().map(Into::into));
        self
    }

    /// Only videos with these names or UIDs (the `name` parameter,
    /// comma-separated).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let page = client.videos().list().names(["intro.mp4"]).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn names<I>(mut self, names: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.names.extend(names.into_iter().map(Into::into));
        self
    }

    /// Only recordings of this live input.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let page = client
    ///     .videos()
    ///     .list()
    ///     .live_input("66be4bf738797e01e1fca35a7bdecdcd")
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn live_input(mut self, id: impl Into<LiveInputId>) -> Self {
        self.live_input = Some(id.into());
        self
    }

    /// Partial match on `meta.name`. Slow on large libraries and may be
    /// unavailable for very large ones; prefer
    /// [`video_name`](Self::video_name).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let page = client.videos().list().search("keynote").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn search(mut self, text: impl Into<String>) -> Self {
        self.search = Some(text.into());
        self
    }

    /// Exact (and fast) match on `meta.name`.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let page = client.videos().list().video_name("keynote.mp4").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn video_name(mut self, name: impl Into<String>) -> Self {
        self.video_name = Some(name.into());
        self
    }

    /// Only videos in this processing state.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let page = client.videos().list().status(cf_stream::VideoState::Error).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn status(mut self, state: VideoState) -> Self {
        self.status = Some(state);
        self
    }

    /// Only uploaded videos or only live recordings (the `type` parameter).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let page = client.videos().list().kind(cf_stream::VideoKind::Live).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn kind(mut self, kind: VideoKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Videos per request, from 1 to 1000 (the API default is 1000).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let page = client.videos().list().limit(50).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Asks the API to report [`VideoPage::total`].
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let page = client.videos().list().include_counts(true).send().await?;
    /// println!("{:?} videos", page.total);
    /// # Ok(()) }
    /// ```
    pub fn include_counts(mut self, include: bool) -> Self {
        self.include_counts = Some(include);
        self
    }

    fn build(&self) -> Result<ApiRequest> {
        if let Some(limit) = self.limit {
            if !(1..=DEFAULT_LIMIT).contains(&limit) {
                return Err(Error::InvalidInput(format!(
                    "limit must be between 1 and {DEFAULT_LIMIT}, got {limit}"
                )));
            }
        }
        let status = self
            .status
            .as_ref()
            .map(|s| crate::util::wire("video state", s))
            .transpose()?;
        let kind = self
            .kind
            .as_ref()
            .map(|k| crate::util::wire("video kind", k))
            .transpose()?;
        let after = self.after.as_ref().map(|b| b.render("after")).transpose()?;
        let before = self
            .before
            .as_ref()
            .map(|b| b.render("before"))
            .transpose()?;
        let ids = join("video ID", self.ids.iter().map(VideoId::as_str))?;
        let names = join("video name", self.names.iter().map(String::as_str))?;
        Ok(ApiRequest::get(Vec::<String>::new())
            .query_opt("after", after)
            .query_opt("before", before)
            .query_opt("asc", self.asc)
            .query_opt("creator", self.creator.as_deref())
            .query_opt("id", ids)
            .query_opt("name", names)
            .query_opt(
                "live_input_id",
                self.live_input.as_ref().map(LiveInputId::as_str),
            )
            .query_opt("search", self.search.as_deref())
            .query_opt("video_name", self.video_name.as_deref())
            .query_opt("status", status)
            .query_opt("type", kind)
            .query_opt("limit", self.limit)
            .query_opt("include_counts", self.include_counts))
    }

    /// Fetches one page.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let page = client.videos().list().send().await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(&self) -> Result<VideoPage> {
        let request = self.build()?;
        let (result, info): (Option<ListResult>, _) = self.client.inner.json_page(request).await?;
        Ok(match result {
            Some(ListResult::Counted {
                videos,
                range,
                total,
            }) => VideoPage {
                videos,
                range: range.or(info.range),
                total: total.or(info.total),
            },
            Some(ListResult::Plain(videos)) => VideoPage {
                videos,
                range: info.range,
                total: info.total,
            },
            None => VideoPage {
                videos: Vec::new(),
                range: info.range,
                total: info.total,
            },
        })
    }

    /// Streams every matching video, fetching pages lazily. The stream ends
    /// after the first error.
    ///
    /// The API has no page cursor, so each request after the first narrows
    /// the creation-time window to the last video seen: `before` (newest
    /// first, the default) or `after` (with [`asc`](Self::asc)). Creation
    /// times have microsecond precision, so videos rarely share one.
    ///
    /// - Newest first: `before` is exclusive, so pages don't overlap. A video
    ///   sharing the exact microsecond of the last one on a page would be
    ///   skipped.
    /// - Oldest first: `after` is inclusive, so the next page starts one
    ///   microsecond after the last video, with the same caveat. If a
    ///   creation time can't be parsed, the boundary is repeated instead and
    ///   deduplicated; if more videos than [`limit`](Self::limit) then share
    ///   one creation time, the stream yields an [`Error::InvalidInput`]
    ///   instead of looping.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use futures_util::TryStreamExt;
    ///
    /// let mut videos = std::pin::pin!(client.videos().list().creator("user-42").into_stream());
    /// while let Some(video) = videos.try_next().await? {
    ///     println!("{:?}", video.uid);
    /// }
    /// # Ok(()) }
    /// ```
    pub fn into_stream(self) -> impl Stream<Item = Result<Video>> + Send + 'a {
        struct State<'a> {
            request: ListVideosRequest<'a>,
            buffer: VecDeque<Video>,
            /// IDs of the videos seen at the current boundary time.
            seen: HashSet<VideoId>,
            done: bool,
        }

        let state = State {
            request: self,
            buffer: VecDeque::new(),
            seen: HashSet::new(),
            done: false,
        };

        impl State<'_> {
            async fn next_page(&mut self) -> Result<()> {
                let page = self.request.send().await?;
                let limit = self.request.limit.unwrap_or(DEFAULT_LIMIT) as usize;
                let full = page.videos.len() >= limit;
                if page.videos.is_empty() || !full || page.range == Some(0) {
                    self.done = true;
                }

                let cursor = page.videos.last().map(|v| v.created.clone());
                let previous = match self.ascending() {
                    true => self.request.after.as_ref(),
                    false => self.request.before.as_ref(),
                };
                let previous = match previous {
                    Some(Bound::Raw(raw)) => Some(raw.clone()),
                    _ => None,
                };

                let fresh: Vec<Video> = page
                    .videos
                    .iter()
                    .filter(|v| v.uid.as_ref().is_none_or(|id| !self.seen.contains(id)))
                    .cloned()
                    .collect();
                if fresh.is_empty() && !self.done {
                    self.done = true;
                    return Err(Error::InvalidInput(format!(
                        "can't paginate past more than {limit} videos created at {}",
                        previous.as_deref().unwrap_or("the same time")
                    )));
                }
                self.buffer.extend(fresh);
                if self.done {
                    return Ok(());
                }

                let Some(Some(cursor)) = cursor else {
                    self.done = true;
                    return Err(Error::InvalidInput(
                        "can't paginate: a listed video has no `created` time".into(),
                    ));
                };
                // Remember every video at the new boundary time, so a request
                // that includes the boundary doesn't repeat them.
                //
                // API quirk (observed 2026-10-21, `list_date_bounds` live
                // test): the reference doesn't say, but `before` (`end`) is
                // exclusive and `after` (`start`) is inclusive. Descending
                // pages therefore don't overlap; ascending pages step one
                // microsecond past the boundary (below). This dedup only
                // matters if that step isn't possible.
                if previous.as_deref() != Some(cursor.as_str())
                    && previous.as_deref() != crate::util::next_microsecond(&cursor).as_deref()
                {
                    self.seen.clear();
                }
                self.seen.extend(
                    page.videos
                        .iter()
                        .filter(|v| v.created.as_deref() == Some(cursor.as_str()))
                        .filter_map(|v| v.uid.clone()),
                );
                if self.ascending() {
                    // `after` is inclusive, so step just past the boundary;
                    // otherwise a full page could be the boundary video
                    // again (always, with `limit(1)`). If the time can't be
                    // parsed, fall back to the inclusive bound and rely on
                    // the dedup above.
                    let next = crate::util::next_microsecond(&cursor).unwrap_or(cursor);
                    self.request.after = Some(Bound::Raw(next));
                } else {
                    self.request.before = Some(Bound::Raw(cursor));
                }
                Ok(())
            }

            fn ascending(&self) -> bool {
                self.request.asc == Some(true)
            }
        }

        futures_util::stream::unfold(state, |mut state| async move {
            loop {
                if let Some(video) = state.buffer.pop_front() {
                    return Some((Ok(video), state));
                }
                if state.done {
                    return None;
                }
                match state.next_page().await {
                    Ok(()) => {}
                    Err(err) => {
                        state.done = true;
                        return Some((Err(err), state));
                    }
                }
            }
        })
    }
}

fn join<'s>(what: &str, values: impl Iterator<Item = &'s str>) -> Result<Option<String>> {
    let values: Vec<&str> = values.collect();
    if values.is_empty() {
        return Ok(None);
    }
    for value in &values {
        crate::types::require_non_empty(what, value)?;
        if value.contains(',') {
            return Err(Error::InvalidInput(format!(
                "{what} {value:?} must not contain a comma"
            )));
        }
    }
    Ok(Some(values.join(",")))
}

// ---------------------------------------------------------------------------
// Basic upload

/// A video file to upload: in-memory data, a byte stream, or a prepared
/// body.
#[non_exhaustive]
pub struct UploadSource {
    body: reqwest::Body,
    filename: String,
    length: Option<u64>,
    content_type: Option<String>,
}

impl fmt::Debug for UploadSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadSource")
            .field("filename", &self.filename)
            .field("length", &self.length)
            .field("content_type", &self.content_type)
            .finish_non_exhaustive()
    }
}

impl UploadSource {
    /// Uploads in-memory data as the `file` part, named `filename`.
    ///
    /// ```
    /// let source = cf_stream::UploadSource::bytes(vec![0u8; 16], "intro.mp4");
    /// ```
    pub fn bytes(data: impl Into<Bytes>, filename: impl Into<String>) -> Self {
        let data: Bytes = data.into();
        let length = Some(data.len() as u64);
        Self {
            body: reqwest::Body::from(data),
            filename: filename.into(),
            length,
            content_type: None,
        }
    }

    /// Uploads a stream of chunks without buffering it in memory.
    ///
    /// Without [`with_length`](Self::with_length) the multipart body is sent
    /// with chunked transfer encoding.
    ///
    /// ```
    /// use bytes::Bytes;
    ///
    /// let chunks = futures_util::stream::iter(vec![
    ///     Ok::<_, std::io::Error>(Bytes::from_static(b"\x00\x00\x00\x18ftyp")),
    /// ]);
    /// let source = cf_stream::UploadSource::stream(chunks, "intro.mp4").with_length(8);
    /// ```
    pub fn stream<S>(stream: S, filename: impl Into<String>) -> Self
    where
        S: futures_core::TryStream + Send + 'static,
        Bytes: From<S::Ok>,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        Self::body(reqwest::Body::wrap_stream(stream), filename)
    }

    /// Uploads a prepared [`reqwest::Body`].
    ///
    /// ```
    /// let source = cf_stream::UploadSource::body(cf_stream::reqwest::Body::from("data"), "a.mp4");
    /// ```
    pub fn body(body: reqwest::Body, filename: impl Into<String>) -> Self {
        Self {
            body,
            filename: filename.into(),
            length: None,
            content_type: None,
        }
    }

    /// Sets the MIME type of the file part (e.g. `video/mp4`).
    ///
    /// ```
    /// let source = cf_stream::UploadSource::bytes(vec![0u8; 16], "a.mp4").with_content_type("video/mp4");
    /// ```
    pub fn with_content_type(mut self, mime: impl Into<String>) -> Self {
        self.content_type = Some(mime.into());
        self
    }

    /// Declares the exact length of a streamed body so it is sent with a
    /// `Content-Length` instead of chunked encoding.
    ///
    /// ```
    /// let source = cf_stream::UploadSource::body(cf_stream::reqwest::Body::from("data"), "a.mp4").with_length(4);
    /// ```
    pub fn with_length(mut self, len: u64) -> Self {
        self.length = Some(len);
        self
    }

    pub(crate) fn into_part(self) -> Result<Part> {
        let part = match self.length {
            Some(len) => Part::stream_with_length(self.body, len),
            None => Part::stream(self.body),
        }
        .file_name(self.filename);
        match self.content_type {
            Some(ct) => part
                .mime_str(&ct)
                .map_err(|e| Error::InvalidInput(format!("invalid content type {ct:?}: {e}"))),
            None => Ok(part),
        }
    }
}

/// A pending basic upload. Created by [`Videos::upload`].
#[derive(Debug)]
#[must_use = "an upload does nothing until you call `send`"]
#[non_exhaustive]
pub struct UploadRequest<'a> {
    client: &'a Client,
    source: UploadSource,
}

impl UploadRequest<'_> {
    /// Uploads the video and returns its details. Processing continues
    /// after this returns; poll [`Videos::get`] or use a webhook to learn
    /// when it is ready.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the content type is invalid; otherwise as
    /// for any API call.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let source = cf_stream::UploadSource::bytes(std::fs::read("a.mp4").expect("read"), "a.mp4");
    /// let video = client.videos().upload(source).send().await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(self) -> Result<Video> {
        let form = Form::new().part("file", self.source.into_part()?);
        let request = ApiRequest::new(Method::POST, Vec::<String>::new()).multipart(form);
        self.client.inner.json(request).await
    }
}

// ---------------------------------------------------------------------------
// Copy and clip

/// A pending upload from a URL. Created by [`Videos::copy`].
#[derive(Clone, Debug)]
#[must_use = "a copy request does nothing until you call `send`"]
#[non_exhaustive]
pub struct CopyRequest<'a> {
    client: &'a Client,
    url: Url,
    options: VideoOptions,
}

impl CopyRequest<'_> {
    video_option_setters!(
        "client\n    .videos()\n    .copy(\"https://example.com/intro.mp4\".parse().expect(\"valid URL\"))"
    );

    /// Starts the copy and returns the new video, initially in the
    /// [`Downloading`](VideoState::Downloading) state.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for invalid settings; otherwise as for any
    /// API call.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let url = "https://example.com/intro.mp4".parse().expect("valid URL");
    /// let video = client.videos().copy(url).send().await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(self) -> Result<Video> {
        let mut body = Map::new();
        body.insert("input".into(), self.url.as_str().into());
        self.options.write(&mut body)?;
        let request = ApiRequest::new(Method::POST, ["copy"]).json(body.into());
        self.client.inner.json(request).await
    }
}

/// A pending clip. Created by [`Videos::clip`].
#[derive(Clone, Debug)]
#[must_use = "a clip request does nothing until you call `send`"]
#[non_exhaustive]
pub struct ClipRequest<'a> {
    client: &'a Client,
    source: VideoId,
    start_secs: u32,
    end_secs: u32,
    options: VideoOptions,
}

impl ClipRequest<'_> {
    video_option_setters!(
        "client\n    .videos()\n    .clip(&\"ea95132c15732412d22c1476fa83f27a\".into(), 0, 30)"
    );

    /// Starts clipping and returns the new video.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the source ID is empty, the end isn't after
    /// the start, or a setting is invalid; otherwise as for any API call.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let clip = client
    ///     .videos()
    ///     .clip(&"ea95132c15732412d22c1476fa83f27a".into(), 5, 15)
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(self) -> Result<Video> {
        crate::types::require_non_empty("video ID", self.source.as_str())?;
        if self.end_secs <= self.start_secs {
            return Err(Error::InvalidInput(format!(
                "clip end ({}s) must be after its start ({}s)",
                self.end_secs, self.start_secs
            )));
        }
        let mut body = Map::new();
        body.insert("clippedFromVideoUID".into(), self.source.as_str().into());
        body.insert("startTimeSeconds".into(), self.start_secs.into());
        body.insert("endTimeSeconds".into(), self.end_secs.into());
        self.options.write(&mut body)?;
        let request = ApiRequest::new(Method::POST, ["clip"]).json(body.into());
        self.client.inner.json(request).await
    }
}

// ---------------------------------------------------------------------------
// Storage usage

/// A storage usage request. Created by [`Videos::storage_usage`].
#[derive(Clone, Debug)]
#[must_use = "a storage usage request does nothing until you call `send`"]
#[non_exhaustive]
pub struct StorageUsageRequest<'a> {
    client: &'a Client,
    creator: Option<String>,
}

impl StorageUsageRequest<'_> {
    /// Only counts videos with this `creator`.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let usage = client.videos().storage_usage().creator("user-42").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn creator(mut self, creator: impl Into<String>) -> Self {
        self.creator = Some(creator.into());
        self
    }

    /// Fetches the usage.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let usage = client.videos().storage_usage().send().await?;
    /// println!("{:?} videos", usage.video_count);
    /// # Ok(()) }
    /// ```
    pub async fn send(&self) -> Result<StorageUsage> {
        let request =
            ApiRequest::get(["storage-usage"]).query_opt("creator", self.creator.as_deref());
        self.client.inner.json(request).await
    }
}

// ---------------------------------------------------------------------------
// Token

/// A request for a signed playback token. Created by
/// [`Videos::create_token`].
#[derive(Clone, Debug)]
#[must_use = "a token request does nothing until you call `send`"]
#[non_exhaustive]
pub struct TokenRequest<'a> {
    client: &'a Client,
    id: VideoId,
    expires_at: Option<SystemTime>,
    not_before: Option<SystemTime>,
    downloadable: Option<bool>,
    original: Option<bool>,
    access_rules: Vec<AccessRule>,
}

impl TokenRequest<'_> {
    /// When the token expires (at most 24 hours from now; the API default is
    /// one hour).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use std::time::{Duration, SystemTime};
    ///
    /// let token = client
    ///     .videos()
    ///     .create_token(&"ea95132c15732412d22c1476fa83f27a".into())
    ///     .expires_at(SystemTime::now() + Duration::from_secs(600))
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn expires_at(mut self, at: SystemTime) -> Self {
        self.expires_at = Some(at);
        self
    }

    /// Sets the expiry relative to now. See [`expires_at`](Self::expires_at).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let token = client
    ///     .videos()
    ///     .create_token(&"ea95132c15732412d22c1476fa83f27a".into())
    ///     .expires_in(std::time::Duration::from_secs(600))
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn expires_in(self, ttl: Duration) -> Self {
        self.expires_at(SystemTime::now() + ttl)
    }

    /// The token isn't valid before this time (the API default is one hour
    /// ago).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let token = client
    ///     .videos()
    ///     .create_token(&"ea95132c15732412d22c1476fa83f27a".into())
    ///     .not_before(std::time::SystemTime::now())
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn not_before(mut self, at: SystemTime) -> Self {
        self.not_before = Some(at);
        self
    }

    /// Allows the token to be used for MP4 downloads.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let token = client
    ///     .videos()
    ///     .create_token(&"ea95132c15732412d22c1476fa83f27a".into())
    ///     .downloadable(true)
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn downloadable(mut self, downloadable: bool) -> Self {
        self.downloadable = Some(downloadable);
        self
    }

    /// Allows the token to access the original uploaded file (the
    /// `flags.original` option).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let token = client
    ///     .videos()
    ///     .create_token(&"ea95132c15732412d22c1476fa83f27a".into())
    ///     .original(true)
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn original(mut self, original: bool) -> Self {
        self.original = Some(original);
        self
    }

    /// Adds an access rule (at most 5). See [`AccessRule`].
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use cf_stream::{AccessAction, AccessRule};
    ///
    /// let token = client
    ///     .videos()
    ///     .create_token(&"ea95132c15732412d22c1476fa83f27a".into())
    ///     .access_rule(AccessRule::country(AccessAction::Allow, ["US"]))
    ///     .access_rule(AccessRule::any(AccessAction::Block))
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn access_rule(mut self, rule: AccessRule) -> Self {
        self.access_rules.push(rule);
        self
    }

    /// Creates the token.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the video ID is empty, a time is before
    /// 1970, or there are too many or empty access rules; otherwise as for
    /// any API call.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let token = client
    ///     .videos()
    ///     .create_token(&"ea95132c15732412d22c1476fa83f27a".into())
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(self) -> Result<SecretString> {
        let mut body = Map::new();
        if let Some(at) = self.expires_at {
            body.insert("exp".into(), crate::util::unix_secs(at, "expiry")?.into());
        }
        if let Some(at) = self.not_before {
            body.insert(
                "nbf".into(),
                crate::util::unix_secs(at, "not-before time")?.into(),
            );
        }
        if let Some(downloadable) = self.downloadable {
            body.insert("downloadable".into(), downloadable.into());
        }
        if let Some(original) = self.original {
            body.insert("flags".into(), json!({ "original": original }));
        }
        if !self.access_rules.is_empty() {
            body.insert(
                "accessRules".into(),
                crate::access::rules_json(&self.access_rules)?,
            );
        }
        let request = ApiRequest::new(Method::POST, [self.id.as_str(), "token"]).json(body.into());
        let response: TokenResponse = self.client.inner.json(request).await?;
        Ok(response.token)
    }
}
