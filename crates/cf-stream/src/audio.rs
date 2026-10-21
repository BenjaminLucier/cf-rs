//! Additional audio tracks, e.g. dubbed languages.

use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Map, json};

use crate::client::Client;
use crate::error::Result;
use crate::transport::ApiRequest;
use crate::types::{AudioTrackId, VideoId};

/// Audio track endpoints. Created by [`Client::audio_tracks`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct AudioTracks<'a> {
    client: &'a Client,
}

/// An additional audio track of a video.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub struct AudioTrack {
    /// The track's unique identifier.
    pub uid: Option<AudioTrackId>,
    /// The track's name, unique per video.
    pub label: Option<String>,
    /// Whether this track plays by default.
    pub default: Option<bool>,
    /// Processing status.
    pub status: Option<AudioTrackStatus>,
}

/// Processing status of an audio track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum AudioTrackStatus {
    /// Queued for processing.
    Queued,
    /// Available.
    Ready,
    /// Processing failed.
    Error,
    /// A value this version of the crate doesn't recognize.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// Changes to apply with [`AudioTracks::update`]. Unset fields are left
/// unchanged.
///
/// ```
/// let update = cf_stream::UpdateAudioTrack::new().label("English").default(true);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct UpdateAudioTrack {
    label: Option<String>,
    default: Option<bool>,
}

impl UpdateAudioTrack {
    /// An update that changes nothing; add fields with the setters.
    ///
    /// ```
    /// let update = cf_stream::UpdateAudioTrack::new();
    /// ```
    pub fn new() -> Self {
        <Self as Default>::default()
    }

    /// Renames the track.
    ///
    /// ```
    /// let update = cf_stream::UpdateAudioTrack::new().label("Deutsch");
    /// ```
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Makes this the default track (unsetting the others), or not.
    ///
    /// ```
    /// let update = cf_stream::UpdateAudioTrack::new().default(true);
    /// ```
    pub fn default(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }
}

#[derive(Deserialize)]
struct AudioList {
    #[serde(default)]
    audio: Vec<AudioTrack>,
}

impl<'a> AudioTracks<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Lists a video's additional audio tracks (not the original audio).
    ///
    /// `GET /accounts/{account_id}/stream/{video_id}/audio`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let tracks = client.audio_tracks().list(&"ea95132c15732412d22c1476fa83f27a".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn list(&self, video: &VideoId) -> Result<Vec<AudioTrack>> {
        let request = ApiRequest::get([video.as_str(), "audio"]);
        let list: Option<AudioList> = self.client.inner.json(request).await?;
        Ok(list.map(|l| l.audio).unwrap_or_default())
    }

    /// Adds an audio track that Cloudflare downloads from `url`. The label
    /// must be unique for the video.
    ///
    /// `POST /accounts/{account_id}/stream/{video_id}/audio/copy`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let url = "https://example.com/german.m4a".parse().expect("valid URL");
    /// let track = client
    ///     .audio_tracks()
    ///     .copy(&"ea95132c15732412d22c1476fa83f27a".into(), "Deutsch", url)
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn copy(&self, video: &VideoId, label: &str, url: Url) -> Result<AudioTrack> {
        crate::types::require_non_empty("audio track label", label)?;
        let body = json!({ "label": label, "url": url.as_str() });
        let request = ApiRequest::new(Method::POST, [video.as_str(), "audio", "copy"]).json(body);
        self.client.inner.json(request).await
    }

    /// Renames a track or changes the default track.
    ///
    /// `PATCH /accounts/{account_id}/stream/{video_id}/audio/{audio_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use cf_stream::UpdateAudioTrack;
    ///
    /// let track = client
    ///     .audio_tracks()
    ///     .update(
    ///         &"ea95132c15732412d22c1476fa83f27a".into(),
    ///         &"f3f0b5c2a2b14f2a9a0a6d2b6c1d9e8f".into(),
    ///         UpdateAudioTrack::new().default(true),
    ///     )
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn update(
        &self,
        video: &VideoId,
        track: &AudioTrackId,
        update: UpdateAudioTrack,
    ) -> Result<AudioTrack> {
        let mut body = Map::new();
        if let Some(label) = update.label {
            body.insert("label".into(), label.into());
        }
        if let Some(default) = update.default {
            body.insert("default".into(), default.into());
        }
        let request = ApiRequest::new(Method::PATCH, [video.as_str(), "audio", track.as_str()])
            .json(body.into());
        self.client.inner.json(request).await
    }

    /// Deletes a track. The default track can't be deleted.
    ///
    /// `DELETE /accounts/{account_id}/stream/{video_id}/audio/{audio_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// client
    ///     .audio_tracks()
    ///     .delete(&"ea95132c15732412d22c1476fa83f27a".into(), &"f3f0b5c2a2b14f2a9a0a6d2b6c1d9e8f".into())
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn delete(&self, video: &VideoId, track: &AudioTrackId) -> Result<()> {
        let request = ApiRequest::new(Method::DELETE, [video.as_str(), "audio", track.as_str()]);
        self.client.inner.empty(request).await
    }
}
