//! Settings shared by the requests that create a video: copy from a URL,
//! clip, and direct creator uploads.

use std::time::SystemTime;

use serde_json::{Map, Value, json};

use crate::error::Result;
use crate::types::WatermarkId;
use crate::util::Metadata;

/// The optional video settings that every create request accepts.
#[derive(Clone, Debug, Default)]
pub(crate) struct VideoOptions {
    pub(crate) meta: Option<Metadata>,
    pub(crate) creator: Option<String>,
    pub(crate) require_signed_urls: Option<bool>,
    pub(crate) allowed_origins: Option<Vec<String>>,
    pub(crate) thumbnail_timestamp_pct: Option<f64>,
    pub(crate) scheduled_deletion: Option<SystemTime>,
    pub(crate) watermark: Option<WatermarkId>,
}

impl VideoOptions {
    /// Writes the set options into a JSON request body.
    pub(crate) fn write(self, body: &mut Map<String, Value>) -> Result<()> {
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
        if let Some(at) = self.scheduled_deletion {
            let at = crate::util::rfc3339(at, "scheduled deletion")?;
            body.insert("scheduledDeletion".into(), at.into());
        }
        if let Some(watermark) = self.watermark {
            crate::types::require_non_empty("watermark ID", watermark.as_str())?;
            body.insert("watermark".into(), json!({ "uid": watermark }));
        }
        Ok(())
    }
}

/// Generates the setters for [`VideoOptions`] on a request builder with an
/// `options: VideoOptions` field. `$example` is an expression that creates
/// the builder from a `client: cf_stream::Client`, used in the doctests.
macro_rules! video_option_setters {
    ($example:literal) => {
        /// Attaches user metadata (any JSON-serializable value). Setting a
        /// `name` key names the video in the dashboard. Serialization errors
        /// are reported by `send`.
        ///
        #[doc = concat!(
            "```no_run\n# async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {\nlet video = ",
            $example,
            "\n    .meta(&serde_json::json!({ \"name\": \"intro.mp4\" }))\n    .send()\n    .await?;\n# Ok(()) }\n```"
        )]
        pub fn meta<T: serde::Serialize + ?Sized>(mut self, meta: &T) -> Self {
            self.options.meta = Some(crate::util::Metadata::capture(meta));
            self
        }

        /// Sets the `creator` field, a user-defined identifier for the
        /// media creator (at most 64 characters).
        ///
        #[doc = concat!(
            "```no_run\n# async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {\nlet video = ",
            $example,
            "\n    .creator(\"user-42\")\n    .send()\n    .await?;\n# Ok(()) }\n```"
        )]
        pub fn creator(mut self, creator: impl Into<String>) -> Self {
            self.options.creator = Some(creator.into());
            self
        }

        /// Requires a signed token to play the video. See
        /// [`Videos::create_token`](crate::Videos::create_token) and
        /// `TokenSigner` (feature `signing`).
        ///
        #[doc = concat!(
            "```no_run\n# async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {\nlet video = ",
            $example,
            "\n    .require_signed_urls(true)\n    .send()\n    .await?;\n# Ok(()) }\n```"
        )]
        pub fn require_signed_urls(mut self, required: bool) -> Self {
            self.options.require_signed_urls = Some(required);
            self
        }

        /// Restricts the origins allowed to embed the video, e.g.
        /// `example.com` or `*.example.com`. An empty list allows any origin.
        ///
        #[doc = concat!(
            "```no_run\n# async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {\nlet video = ",
            $example,
            "\n    .allowed_origins([\"example.com\", \"*.example.com\"])\n    .send()\n    .await?;\n# Ok(()) }\n```"
        )]
        pub fn allowed_origins<I>(mut self, origins: I) -> Self
        where
            I: IntoIterator,
            I::Item: Into<String>,
        {
            self.options.allowed_origins = Some(origins.into_iter().map(Into::into).collect());
            self
        }

        /// Takes the default thumbnail at this fraction of the video's
        /// duration (`0.0` to `1.0`). Out-of-range values are reported by
        /// `send`.
        ///
        #[doc = concat!(
            "```no_run\n# async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {\nlet video = ",
            $example,
            "\n    .thumbnail_timestamp_pct(0.5)\n    .send()\n    .await?;\n# Ok(()) }\n```"
        )]
        pub fn thumbnail_timestamp_pct(mut self, pct: f64) -> Self {
            self.options.thumbnail_timestamp_pct = Some(pct);
            self
        }

        /// Deletes the video automatically at this time. Cloudflare requires
        /// it to be at least 30 days after upload.
        ///
        #[doc = concat!(
            "```no_run\n# async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {\nuse std::time::{Duration, SystemTime};\n\nlet video = ",
            $example,
            "\n    .scheduled_deletion(SystemTime::now() + Duration::from_secs(90 * 86_400))\n    .send()\n    .await?;\n# Ok(()) }\n```"
        )]
        pub fn scheduled_deletion(mut self, at: std::time::SystemTime) -> Self {
            self.options.scheduled_deletion = Some(at);
            self
        }

        /// Applies a watermark profile (see [`Watermarks`](crate::Watermarks)).
        ///
        #[doc = concat!(
            "```no_run\n# async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {\nlet video = ",
            $example,
            "\n    .watermark(\"ea95132c15732412d22c1476fa83f27a\")\n    .send()\n    .await?;\n# Ok(()) }\n```"
        )]
        pub fn watermark(mut self, id: impl Into<crate::types::WatermarkId>) -> Self {
            self.options.watermark = Some(id.into());
            self
        }
    };
}

pub(crate) use video_option_setters;
