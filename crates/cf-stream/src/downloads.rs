//! Downloadable MP4 (video) and M4A (audio) renditions.

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;
use crate::transport::ApiRequest;
use crate::types::VideoId;

/// Download endpoints. Created by [`Client::downloads`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Downloads<'a> {
    client: &'a Client,
}

/// Which download rendition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DownloadType {
    /// An MP4 of the video (`downloads/default.mp4`).
    Default,
    /// An M4A of the audio (`downloads/audio.m4a`).
    Audio,
}

impl DownloadType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Audio => "audio",
        }
    }
}

/// A video's download renditions, keyed by type.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[non_exhaustive]
pub struct VideoDownloads {
    /// The MP4 rendition, if created.
    pub default: Option<Download>,
    /// The M4A rendition, if created.
    pub audio: Option<Download>,
}

/// One download rendition.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Download {
    /// Generation status.
    pub status: Option<DownloadStatus>,
    /// Generation progress from `0` to `100`.
    pub percent_complete: Option<f64>,
    /// The download URL. For videos that require signed URLs, replace the
    /// video ID in it with a token created with `downloadable` set.
    pub url: Option<String>,
}

/// Generation status of a download.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DownloadStatus {
    /// Available.
    Ready,
    /// Being generated.
    #[serde(rename = "inprogress")]
    InProgress,
    /// Generation failed.
    Error,
    /// A value this version of the crate doesn't recognize.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl<'a> Downloads<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Lists a video's download renditions.
    ///
    /// `GET /accounts/{account_id}/stream/{video_id}/downloads`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let downloads = client.downloads().get(&"ea95132c15732412d22c1476fa83f27a".into()).await?;
    /// if let Some(mp4) = downloads.default {
    ///     println!("{:?} {:?}", mp4.status, mp4.url);
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn get(&self, video: &VideoId) -> Result<VideoDownloads> {
        let request = ApiRequest::get([video.as_str(), "downloads"]);
        self.client.inner.json(request).await
    }

    /// Starts generating a download rendition.
    ///
    /// `POST /accounts/{account_id}/stream/{video_id}/downloads/{download_type}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use cf_stream::DownloadType;
    ///
    /// let downloads = client
    ///     .downloads()
    ///     .create(&"ea95132c15732412d22c1476fa83f27a".into(), DownloadType::Default)
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn create(&self, video: &VideoId, kind: DownloadType) -> Result<VideoDownloads> {
        let request = ApiRequest::new(Method::POST, [video.as_str(), "downloads", kind.as_str()]);
        self.client.inner.json(request).await
    }

    /// Deletes a download rendition.
    ///
    /// `DELETE /accounts/{account_id}/stream/{video_id}/downloads/{download_type}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use cf_stream::DownloadType;
    ///
    /// client
    ///     .downloads()
    ///     .delete(&"ea95132c15732412d22c1476fa83f27a".into(), DownloadType::Audio)
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn delete(&self, video: &VideoId, kind: DownloadType) -> Result<()> {
        let request = ApiRequest::new(Method::DELETE, [video.as_str(), "downloads", kind.as_str()]);
        self.client.inner.empty(request).await
    }
}
