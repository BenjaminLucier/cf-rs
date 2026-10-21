//! Live inputs (RTMPS, SRT and WebRTC ingest endpoints) and their outputs
//! (restreaming destinations).

use std::fmt;

use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value, json};

use crate::client::Client;
use crate::delivery::Delivery;
use crate::error::Result;
use crate::transport::ApiRequest;
use crate::types::{LiveInputId, OutputId, VideoId};
use crate::util::{Metadata, Patch};
use crate::videos::Playback;

/// Live input endpoints. Created by [`Client::live_inputs`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct LiveInputs<'a> {
    client: &'a Client,
}

/// Live input output endpoints. Created by [`Client::live_outputs`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct LiveOutputs<'a> {
    client: &'a Client,
}

// ---------------------------------------------------------------------------
// Models

/// A live input: the endpoint a broadcaster streams to.
///
/// Stream keys, SRT stream IDs and passphrases, and the WebRTC publish URL
/// are secrets: they let anyone broadcast to the input. They're held as
/// [`SecretString`] and never appear in `Debug` output.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct LiveInput {
    /// The live input's unique identifier.
    pub uid: Option<LiveInputId>,
    /// User metadata. `meta.name` is the name shown in the dashboard.
    pub meta: Option<Value>,
    /// Whether the input accepts broadcasts.
    pub enabled: Option<bool>,
    /// When the input was created.
    pub created: Option<String>,
    /// When the input was last modified.
    pub modified: Option<String>,
    /// When the input's keys were last rotated.
    pub keys_rotated_at: Option<String>,
    /// Days after which recordings are deleted.
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub delete_recording_after_days: Option<u64>,
    /// The `creator` given to recordings.
    pub default_creator: Option<String>,
    /// Whether low-latency HLS is preferred.
    pub prefer_low_latency: Option<bool>,
    /// Recording settings.
    pub recording: Option<RecordingSettings>,
    /// The latest connection status. `None` when no broadcast has happened.
    #[serde(default, deserialize_with = "lenient_status")]
    pub status: Option<LiveInputStatus>,
    /// HLS and DASH manifest URLs for viewers.
    pub playback: Option<Playback>,
    /// RTMPS ingest details.
    pub rtmps: Option<RtmpsEndpoint>,
    /// RTMPS playback details.
    pub rtmps_playback: Option<RtmpsEndpoint>,
    /// SRT ingest details.
    pub srt: Option<SrtEndpoint>,
    /// SRT playback details.
    pub srt_playback: Option<SrtEndpoint>,
    /// WebRTC (WHIP) ingest details.
    #[serde(rename = "webRTC")]
    pub web_rtc: Option<WebRtcEndpoint>,
    /// WebRTC (WHEP) playback details.
    #[serde(rename = "webRTCPlayback")]
    pub web_rtc_playback: Option<WebRtcEndpoint>,
}

/// A live input as listed by [`LiveInputs::list`]: the connection details
/// are omitted.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct LiveInputSummary {
    /// The live input's unique identifier.
    pub uid: Option<LiveInputId>,
    /// User metadata.
    pub meta: Option<Value>,
    /// Whether the input accepts broadcasts.
    pub enabled: Option<bool>,
    /// When the input was created.
    pub created: Option<String>,
    /// When the input was last modified.
    pub modified: Option<String>,
    /// Days after which recordings are deleted.
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub delete_recording_after_days: Option<u64>,
}

/// The account's live inputs, from [`LiveInputs::list`].
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct LiveInputList {
    /// The live inputs.
    #[serde(default)]
    pub live_inputs: Vec<LiveInputSummary>,
    /// Remaining inputs, if reported.
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub range: Option<u64>,
    /// Total inputs, if reported.
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub total: Option<u64>,
}

/// The connection status of a live input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LiveInputStatus {
    /// A broadcaster is connected.
    Connected,
    /// A broadcaster reconnected.
    Reconnected,
    /// Waiting for a broadcaster to reconnect.
    Reconnecting,
    /// The broadcaster disconnected.
    ClientDisconnect,
    /// The broadcast exceeded the maximum duration.
    TtlExceeded,
    /// The broadcaster failed to connect.
    FailedToConnect,
    /// The broadcaster failed to reconnect.
    FailedToReconnect,
    /// New settings were accepted.
    NewConfigurationAccepted,
    /// A value (or shape) this version of the crate doesn't recognize.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// The status as a known string, `None` for `null`, and `Unknown` for any
/// other shape, so an unexpected status never fails the whole response.
///
/// Not verified live: the schema says a string enum or `null`, and only
/// `null` (no broadcast yet) has been observed. The value during a real
/// broadcast is untested, so other shapes are tolerated rather than trusted.
fn lenient_status<'de, D: Deserializer<'de>>(de: D) -> Result<Option<LiveInputStatus>, D::Error> {
    Ok(match Value::deserialize(de)? {
        Value::Null => None,
        value @ Value::String(_) => {
            Some(serde_json::from_value(value).unwrap_or(LiveInputStatus::Unknown))
        }
        _ => Some(LiveInputStatus::Unknown),
    })
}

/// RTMPS connection details.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RtmpsEndpoint {
    /// The server URL.
    pub url: Option<String>,
    /// The stream key (secret).
    pub stream_key: Option<SecretString>,
}

/// SRT connection details.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SrtEndpoint {
    /// The server URL.
    pub url: Option<String>,
    /// The stream ID (secret).
    pub stream_id: Option<SecretString>,
    /// The passphrase (secret).
    pub passphrase: Option<SecretString>,
}

/// WebRTC connection details.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct WebRtcEndpoint {
    /// The WHIP (ingest) or WHEP (playback) URL. Treated as secret because
    /// the ingest URL embeds the stream key.
    pub url: Option<SecretString>,
}

/// Whether and how broadcasts are recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum RecordingMode {
    /// Don't record (the API default).
    Off,
    /// Record every broadcast.
    Automatic,
    /// A value this version of the crate doesn't recognize. Can't be sent.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// Recording settings of a live input.
///
/// ```
/// use cf_stream::{RecordingMode, RecordingSettings};
///
/// let recording = RecordingSettings::new(RecordingMode::Automatic)
///     .require_signed_urls(true)
///     .timeout_seconds(60);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RecordingSettings {
    /// Whether broadcasts are recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<RecordingMode>,
    /// Origins allowed to embed the live stream and its recordings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_origins: Option<Vec<String>>,
    /// Whether playback requires a signed token.
    #[serde(rename = "requireSignedURLs", skip_serializing_if = "Option::is_none")]
    pub require_signed_urls: Option<bool>,
    /// Whether the player hides the live viewer count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_live_viewer_count: Option<bool>,
    /// How long to wait for a broadcaster to reconnect before ending the
    /// recording, in seconds (`0` uses the platform default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u32>,
}

impl RecordingSettings {
    /// Settings with a recording mode and API defaults for the rest.
    ///
    /// ```
    /// let r = cf_stream::RecordingSettings::new(cf_stream::RecordingMode::Off);
    /// ```
    pub fn new(mode: RecordingMode) -> Self {
        Self {
            mode: Some(mode),
            allowed_origins: None,
            require_signed_urls: None,
            hide_live_viewer_count: None,
            timeout_seconds: None,
        }
    }

    /// Origins allowed to embed the stream.
    ///
    /// ```
    /// use cf_stream::{RecordingMode, RecordingSettings};
    ///
    /// let r = RecordingSettings::new(RecordingMode::Automatic).allowed_origins(["example.com"]);
    /// ```
    pub fn allowed_origins<I>(mut self, origins: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.allowed_origins = Some(origins.into_iter().map(Into::into).collect());
        self
    }

    /// Requires a signed token for playback.
    ///
    /// ```
    /// use cf_stream::{RecordingMode, RecordingSettings};
    ///
    /// let r = RecordingSettings::new(RecordingMode::Automatic).require_signed_urls(true);
    /// ```
    pub fn require_signed_urls(mut self, required: bool) -> Self {
        self.require_signed_urls = Some(required);
        self
    }

    /// Hides the live viewer count in the player.
    ///
    /// ```
    /// use cf_stream::{RecordingMode, RecordingSettings};
    ///
    /// let r = RecordingSettings::new(RecordingMode::Automatic).hide_live_viewer_count(true);
    /// ```
    pub fn hide_live_viewer_count(mut self, hide: bool) -> Self {
        self.hide_live_viewer_count = Some(hide);
        self
    }

    /// Seconds to wait for a reconnect before ending the recording.
    ///
    /// ```
    /// use cf_stream::{RecordingMode, RecordingSettings};
    ///
    /// let r = RecordingSettings::new(RecordingMode::Automatic).timeout_seconds(30);
    /// ```
    pub fn timeout_seconds(mut self, secs: u32) -> Self {
        self.timeout_seconds = Some(secs);
        self
    }
}

/// A restreaming destination of a live input. Its `Debug` output never
/// contains the stream key.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Output {
    /// The output's unique identifier.
    pub uid: Option<OutputId>,
    /// The destination RTMP(S) or SRT URL.
    pub url: Option<String>,
    /// The destination stream key (secret).
    pub stream_key: Option<SecretString>,
    /// Whether the output is active.
    pub enabled: Option<bool>,
}

/// Whether a live input is broadcasting right now, from
/// [`LiveInputs::lifecycle`].
///
/// ```
/// let status: cf_stream::Lifecycle = serde_json::from_str(
///     r#"{"isInput": true, "videoUID": null, "live": false, "status": "disconnected"}"#,
/// ).unwrap();
/// assert_eq!(status.live, Some(false));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Lifecycle {
    /// `true` if the ID is a live input, `false` if it is a video.
    pub is_input: Option<bool>,
    /// The video recording the current (or, for a video ID, that) broadcast.
    /// `None` for a live input that isn't broadcasting.
    #[serde(rename = "videoUID")]
    pub video_uid: Option<VideoId>,
    /// `true` while a broadcast is in progress.
    pub live: Option<bool>,
    /// The connection status, e.g. `disconnected`. Not part of Cloudflare's
    /// documented response, so its values may change.
    pub status: Option<String>,
}

// ---------------------------------------------------------------------------
// Settings

/// Settings for [`LiveInputs::create`] and [`LiveInputs::update`]. Unset
/// fields keep their current (or default) values.
///
/// ```
/// use cf_stream::{LiveInputSettings, RecordingMode, RecordingSettings};
///
/// let settings = LiveInputSettings::new()
///     .meta(&serde_json::json!({ "name": "Studio A" }))
///     .recording(RecordingSettings::new(RecordingMode::Automatic))
///     .delete_recording_after_days(45);
/// ```
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct LiveInputSettings {
    meta: Option<Metadata>,
    default_creator: Option<String>,
    enabled: Option<bool>,
    prefer_low_latency: Option<bool>,
    delete_recording_after_days: Option<Patch<u32>>,
    recording: Option<RecordingSettings>,
}

impl LiveInputSettings {
    /// No settings; add them with the setters.
    ///
    /// ```
    /// let settings = cf_stream::LiveInputSettings::new();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// User metadata; `name` names the input in the dashboard.
    ///
    /// ```
    /// let s = cf_stream::LiveInputSettings::new().meta(&serde_json::json!({ "name": "Main stage" }));
    /// ```
    pub fn meta<T: Serialize + ?Sized>(mut self, meta: &T) -> Self {
        self.meta = Some(Metadata::capture(meta));
        self
    }

    /// The `creator` to set on recordings.
    ///
    /// ```
    /// let s = cf_stream::LiveInputSettings::new().default_creator("user-42");
    /// ```
    pub fn default_creator(mut self, creator: impl Into<String>) -> Self {
        self.default_creator = Some(creator.into());
        self
    }

    /// Enables or disables the input.
    ///
    /// ```
    /// let s = cf_stream::LiveInputSettings::new().enabled(false);
    /// ```
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Prefers low-latency HLS delivery.
    ///
    /// ```
    /// let s = cf_stream::LiveInputSettings::new().prefer_low_latency(true);
    /// ```
    pub fn prefer_low_latency(mut self, prefer: bool) -> Self {
        self.prefer_low_latency = Some(prefer);
        self
    }

    /// Deletes recordings this many days after the broadcast (at least 30).
    ///
    /// ```
    /// let s = cf_stream::LiveInputSettings::new().delete_recording_after_days(30);
    /// ```
    pub fn delete_recording_after_days(mut self, days: u32) -> Self {
        self.delete_recording_after_days = Some(Patch::Set(days));
        self
    }

    /// Keeps recordings indefinitely (sends `"deleteRecordingAfterDays": null`).
    ///
    /// ```
    /// let s = cf_stream::LiveInputSettings::new().clear_delete_recording_after_days();
    /// ```
    pub fn clear_delete_recording_after_days(mut self) -> Self {
        self.delete_recording_after_days = Some(Patch::Clear);
        self
    }

    /// Recording settings.
    ///
    /// ```
    /// use cf_stream::{LiveInputSettings, RecordingMode, RecordingSettings};
    ///
    /// let s = LiveInputSettings::new().recording(RecordingSettings::new(RecordingMode::Off));
    /// ```
    pub fn recording(mut self, recording: RecordingSettings) -> Self {
        self.recording = Some(recording);
        self
    }

    fn into_json(self) -> Result<Value> {
        let mut body = Map::new();
        if let Some(meta) = self.meta {
            body.insert("meta".into(), meta.into_value()?);
        }
        if let Some(creator) = self.default_creator {
            body.insert("defaultCreator".into(), creator.into());
        }
        if let Some(enabled) = self.enabled {
            body.insert("enabled".into(), enabled.into());
        }
        if let Some(prefer) = self.prefer_low_latency {
            body.insert("preferLowLatency".into(), prefer.into());
        }
        match self.delete_recording_after_days {
            Some(Patch::Set(days)) if days < 30 => {
                return Err(crate::Error::InvalidInput(format!(
                    "recordings can be deleted after 30 days at the earliest, got {days}"
                )));
            }
            Some(Patch::Set(days)) => {
                body.insert("deleteRecordingAfterDays".into(), days.into());
            }
            Some(Patch::Clear) => {
                body.insert("deleteRecordingAfterDays".into(), Value::Null);
            }
            None => {}
        }
        if let Some(recording) = self.recording {
            let recording = serde_json::to_value(&recording)
                .map_err(|e| crate::Error::InvalidInput(format!("recording settings: {e}")))?;
            body.insert("recording".into(), recording);
        }
        Ok(body.into())
    }
}

// ---------------------------------------------------------------------------
// Live input endpoints

impl<'a> LiveInputs<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Lists the account's live inputs, with counts.
    ///
    /// `GET /accounts/{account_id}/stream/live_inputs?include_counts=true`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let list = client.live_inputs().list().await?;
    /// println!("{} of {:?}", list.live_inputs.len(), list.total);
    /// # Ok(()) }
    /// ```
    pub async fn list(&self) -> Result<LiveInputList> {
        // `include_counts` is always sent: the counts are cheap, and a flag
        // parameter for them would only add API surface.
        let request = ApiRequest::get(["live_inputs"]).query("include_counts", true);
        self.client.inner.json(request).await
    }

    /// Fetches a live input, including its connection details.
    ///
    /// `GET /accounts/{account_id}/stream/live_inputs/{live_input_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let input = client.live_inputs().get(&"66be4bf738797e01e1fca35a7bdecdcd".into()).await?;
    /// println!("{:?}", input.status);
    /// # Ok(()) }
    /// ```
    pub async fn get(&self, id: &LiveInputId) -> Result<LiveInput> {
        let request = ApiRequest::get(["live_inputs", id.as_str()]);
        self.client.inner.json(request).await
    }

    /// Starts creating a live input. Call
    /// [`send`](CreateLiveInputRequest::send) on the returned request.
    ///
    /// `POST /accounts/{account_id}/stream/live_inputs`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use cf_stream::{LiveInputSettings, RecordingMode, RecordingSettings};
    ///
    /// let input = client
    ///     .live_inputs()
    ///     .create(
    ///         LiveInputSettings::new()
    ///             .meta(&serde_json::json!({ "name": "Studio A" }))
    ///             .recording(RecordingSettings::new(RecordingMode::Automatic)),
    ///     )
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn create(&self, settings: LiveInputSettings) -> CreateLiveInputRequest<'a> {
        CreateLiveInputRequest {
            client: self.client,
            settings,
            idempotency_key: None,
        }
    }

    /// Updates a live input's settings.
    ///
    /// `PUT /accounts/{account_id}/stream/live_inputs/{live_input_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use cf_stream::LiveInputSettings;
    ///
    /// let input = client
    ///     .live_inputs()
    ///     .update(&"66be4bf738797e01e1fca35a7bdecdcd".into(), LiveInputSettings::new().enabled(false))
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn update(&self, id: &LiveInputId, settings: LiveInputSettings) -> Result<LiveInput> {
        let body = settings.into_json()?;
        let request = ApiRequest::new(Method::PUT, ["live_inputs", id.as_str()]).json(body);
        self.client.inner.json(request).await
    }

    /// Deletes a live input. Its recordings are kept.
    ///
    /// `DELETE /accounts/{account_id}/stream/live_inputs/{live_input_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// client.live_inputs().delete(&"66be4bf738797e01e1fca35a7bdecdcd".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn delete(&self, id: &LiveInputId) -> Result<()> {
        let request = ApiRequest::new(Method::DELETE, ["live_inputs", id.as_str()]);
        self.client.inner.empty(request).await
    }

    /// Enables a live input so it accepts broadcasts. Cloudflare doesn't
    /// return the updated input; use [`get`](Self::get) if you need it.
    ///
    /// `POST /accounts/{account_id}/stream/live_inputs/{live_input_id}/enable`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// client.live_inputs().enable(&"66be4bf738797e01e1fca35a7bdecdcd".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn enable(&self, id: &LiveInputId) -> Result<()> {
        // API quirk (observed 2026-10-21): the API reference says this
        // returns the live input, but it returns `"result": "ok"`. Call
        // `get` for the updated input.
        let request = ApiRequest::new(Method::POST, ["live_inputs", id.as_str(), "enable"]);
        self.client.inner.empty(request).await
    }

    /// Disables a live input; broadcasts to it are rejected. Cloudflare
    /// doesn't return the updated input; use [`get`](Self::get) if you need
    /// it.
    ///
    /// `POST /accounts/{account_id}/stream/live_inputs/{live_input_id}/disable`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// client.live_inputs().disable(&"66be4bf738797e01e1fca35a7bdecdcd".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn disable(&self, id: &LiveInputId) -> Result<()> {
        // API quirk: returns `"result": "ok"`, not the input; see `enable`.
        let request = ApiRequest::new(Method::POST, ["live_inputs", id.as_str(), "disable"]);
        self.client.inner.empty(request).await
    }

    /// Replaces a live input's stream keys and passphrases. The old ones
    /// stop working. The returned input only has its ID and the new
    /// connection details.
    ///
    /// `POST /accounts/{account_id}/stream/live_inputs/{live_input_id}/rotate_keys`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let input = client.live_inputs().rotate_keys(&"66be4bf738797e01e1fca35a7bdecdcd".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn rotate_keys(&self, id: &LiveInputId) -> Result<LiveInput> {
        let request = ApiRequest::new(Method::POST, ["live_inputs", id.as_str(), "rotate_keys"]);
        self.client.inner.json(request).await
    }

    /// Checks whether a live input is broadcasting right now, and which
    /// video holds the broadcast. `id` is a live input ID, a video ID, or a
    /// signed token for either.
    ///
    /// `GET https://customer-{code}.cloudflarestream.com/{id}/lifecycle`
    ///
    /// This calls the delivery domain, not the API, and sends **no** API
    /// token. Browsers can call the same URL
    /// ([`Delivery::lifecycle`]) directly. For a video that requires signed
    /// URLs, pass a signed token instead of its ID.
    ///
    /// # Errors
    ///
    /// An error for which [`is_not_found`](crate::Error::is_not_found) is
    /// `true` if the ID doesn't exist, or
    /// [`is_unauthorized`](crate::Error::is_unauthorized) if a signed token
    /// is required; otherwise as for any request.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use cf_stream::{CustomerCode, Delivery};
    ///
    /// let delivery = Delivery::new(CustomerCode::new("m033z5x00ks6nunl"))?;
    /// let status = client
    ///     .live_inputs()
    ///     .lifecycle(&delivery, "66be4bf738797e01e1fca35a7bdecdcd")
    ///     .await?;
    /// if status.live == Some(true) {
    ///     println!("live now, recording to {:?}", status.video_uid);
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn lifecycle(&self, delivery: &Delivery, id: impl AsRef<str>) -> Result<Lifecycle> {
        let id = id.as_ref();
        crate::types::require_non_empty("live input or video ID", id)?;
        // API quirk (observed 2026-10-21): this endpoint isn't in the API
        // reference. Unknown IDs get a plain-text `404 not found`, which
        // `public_json` turns into an `Error::Api` with status 404; the
        // response also has undocumented `status`, `chunked` and `unstable`
        // fields.
        self.client.inner.public_json(delivery.lifecycle(id)).await
    }
}

/// A pending live input. Created by [`LiveInputs::create`].
#[must_use = "a live input request does nothing until you call `send`"]
#[non_exhaustive]
pub struct CreateLiveInputRequest<'a> {
    client: &'a Client,
    settings: LiveInputSettings,
    idempotency_key: Option<String>,
}

impl fmt::Debug for CreateLiveInputRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateLiveInputRequest")
            .field("settings", &self.settings)
            .field("idempotency_key", &self.idempotency_key)
            .finish_non_exhaustive()
    }
}

impl CreateLiveInputRequest<'_> {
    /// Sets an `Idempotency-Key` (at most 255 characters), so retrying a
    /// request whose response was lost doesn't create a second input.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use cf_stream::LiveInputSettings;
    ///
    /// let input = client
    ///     .live_inputs()
    ///     .create(LiveInputSettings::new())
    ///     .idempotency_key("studio-a-2024-06-01")
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    /// Creates the live input.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`](crate::Error::InvalidInput) for invalid
    /// settings or an invalid idempotency key; otherwise as for any API
    /// call.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let input = client.live_inputs().create(cf_stream::LiveInputSettings::new()).send().await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(self) -> Result<LiveInput> {
        let body = self.settings.into_json()?;
        let mut request = ApiRequest::new(Method::POST, ["live_inputs"]).json(body);
        if let Some(key) = self.idempotency_key {
            crate::types::require_non_empty("idempotency key", &key)?;
            request = request.header("idempotency-key", key);
        }
        self.client.inner.json(request).await
    }
}

// ---------------------------------------------------------------------------
// Output endpoints

impl<'a> LiveOutputs<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Lists a live input's outputs.
    ///
    /// `GET /accounts/{account_id}/stream/live_inputs/{live_input_id}/outputs`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let outputs = client.live_outputs().list(&"66be4bf738797e01e1fca35a7bdecdcd".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn list(&self, input: &LiveInputId) -> Result<Vec<Output>> {
        let request = ApiRequest::get(["live_inputs", input.as_str(), "outputs"]);
        let list: Option<Vec<Output>> = self.client.inner.json(request).await?;
        Ok(list.unwrap_or_default())
    }

    /// Adds an output that restreams the input to `url` (RTMP(S) or SRT)
    /// with `stream_key`. New outputs are enabled.
    ///
    /// Cloudflare checks that the destination responds when the output is
    /// created, and rejects unreachable URLs with error code 10010.
    ///
    /// `POST /accounts/{account_id}/stream/live_inputs/{live_input_id}/outputs`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let output = client
    ///     .live_outputs()
    ///     .create(
    ///         &"66be4bf738797e01e1fca35a7bdecdcd".into(),
    ///         "rtmp://a.rtmp.youtube.com/live2",
    ///         "youtube-stream-key",
    ///     )
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn create(
        &self,
        input: &LiveInputId,
        url: &str,
        stream_key: impl Into<SecretString>,
    ) -> Result<Output> {
        crate::types::require_non_empty("output URL", url)?;
        let stream_key: SecretString = stream_key.into();
        let body = json!({ "url": url, "streamKey": stream_key.expose_secret() });
        let request =
            ApiRequest::new(Method::POST, ["live_inputs", input.as_str(), "outputs"]).json(body);
        self.client.inner.json(request).await
    }

    /// Enables or disables an output.
    ///
    /// `PUT /accounts/{account_id}/stream/live_inputs/{live_input_id}/outputs/{output_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let output = client
    ///     .live_outputs()
    ///     .update(&"66be4bf738797e01e1fca35a7bdecdcd".into(), &"baea4d9c515887b80289d5c33cf01145".into(), false)
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn update(
        &self,
        input: &LiveInputId,
        output: &OutputId,
        enabled: bool,
    ) -> Result<Output> {
        let request = ApiRequest::new(
            Method::PUT,
            ["live_inputs", input.as_str(), "outputs", output.as_str()],
        )
        .json(json!({ "enabled": enabled }));
        self.client.inner.json(request).await
    }

    /// Deletes an output.
    ///
    /// `DELETE /accounts/{account_id}/stream/live_inputs/{live_input_id}/outputs/{output_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// client
    ///     .live_outputs()
    ///     .delete(&"66be4bf738797e01e1fca35a7bdecdcd".into(), &"baea4d9c515887b80289d5c33cf01145".into())
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn delete(&self, input: &LiveInputId, output: &OutputId) -> Result<()> {
        let request = ApiRequest::new(
            Method::DELETE,
            ["live_inputs", input.as_str(), "outputs", output.as_str()],
        );
        self.client.inner.empty(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_is_lenient() {
        let parse = |json: &str| -> Option<LiveInputStatus> {
            serde_json::from_str::<LiveInput>(json).unwrap().status
        };
        assert_eq!(parse(r#"{"status": null}"#), None);
        assert_eq!(parse(r#"{}"#), None);
        assert_eq!(
            parse(r#"{"status": "connected"}"#),
            Some(LiveInputStatus::Connected)
        );
        assert_eq!(
            parse(r#"{"status": "brand_new"}"#),
            Some(LiveInputStatus::Unknown)
        );
        assert_eq!(
            parse(r#"{"status": {"current": {"state": "connected"}}}"#),
            Some(LiveInputStatus::Unknown)
        );
    }

    #[test]
    fn settings_json() {
        let body = LiveInputSettings::new()
            .enabled(true)
            .clear_delete_recording_after_days()
            .recording(RecordingSettings::new(RecordingMode::Automatic).timeout_seconds(10))
            .into_json()
            .unwrap();
        assert_eq!(
            body,
            json!({
                "enabled": true,
                "deleteRecordingAfterDays": null,
                "recording": {"mode": "automatic", "timeoutSeconds": 10},
            })
        );
        assert!(
            LiveInputSettings::new()
                .delete_recording_after_days(29)
                .into_json()
                .is_err()
        );
        assert!(
            LiveInputSettings::new()
                .recording(RecordingSettings::new(RecordingMode::Unknown))
                .into_json()
                .is_err()
        );
    }
}
