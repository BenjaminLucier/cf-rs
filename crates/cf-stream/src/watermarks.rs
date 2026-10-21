//! Watermark profiles: images overlaid on videos at upload time.

use std::fmt;

use bytes::Bytes;
use reqwest::multipart::{Form, Part};
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::Map;

use crate::client::Client;
use crate::error::Result;
use crate::transport::ApiRequest;
use crate::types::WatermarkId;

/// Watermark profile endpoints. Created by [`Client::watermarks`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Watermarks<'a> {
    client: &'a Client,
}

/// A watermark profile. Apply it to new videos with the `watermark` setter
/// of copy, clip and direct upload requests.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Watermark {
    /// The profile's unique identifier.
    pub uid: Option<WatermarkId>,
    /// A short description of the profile.
    pub name: Option<String>,
    /// When the profile was created.
    pub created: Option<String>,
    /// The URL the image was downloaded from; `None` for uploaded images.
    pub downloaded_from: Option<String>,
    /// Image size in bytes.
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub size: Option<u64>,
    /// Image width in pixels.
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub width: Option<u64>,
    /// Image height in pixels.
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub height: Option<u64>,
    /// Opacity from `0.0` (transparent) to `1.0` (opaque).
    pub opacity: Option<f64>,
    /// Padding from the video's edges, as a fraction of its size.
    pub padding: Option<f64>,
    /// Size relative to the video, from `0.0` (image as-is) to `1.0`.
    pub scale: Option<f64>,
    /// Where the image is placed.
    pub position: Option<WatermarkPosition>,
}

/// Where a watermark is placed on the video.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum WatermarkPosition {
    /// Top right (the API default).
    UpperRight,
    /// Top left.
    UpperLeft,
    /// Bottom left.
    LowerLeft,
    /// Bottom right.
    LowerRight,
    /// Centered; padding is ignored.
    Center,
    /// A value this version of the crate doesn't recognize. Can't be sent.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl<'a> Watermarks<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Starts creating a watermark profile from a PNG (at most 2 MiB).
    /// Configure it with the returned builder and call
    /// [`send`](CreateWatermarkRequest::send).
    ///
    /// `POST /accounts/{account_id}/stream/watermarks` (multipart/form-data
    /// for uploaded files, JSON for URLs)
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// use cf_stream::{WatermarkPosition, WatermarkSource};
    ///
    /// let logo = std::fs::read("logo.png").expect("read file");
    /// let watermark = client
    ///     .watermarks()
    ///     .create(WatermarkSource::bytes(logo, "logo.png"))
    ///     .name("Logo")
    ///     .position(WatermarkPosition::LowerRight)
    ///     .opacity(0.75)
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn create(&self, source: WatermarkSource) -> CreateWatermarkRequest<'a> {
        CreateWatermarkRequest {
            client: self.client,
            source,
            name: None,
            opacity: None,
            padding: None,
            scale: None,
            position: None,
        }
    }

    /// Lists the account's watermark profiles.
    ///
    /// `GET /accounts/{account_id}/stream/watermarks`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// for watermark in client.watermarks().list().await? {
    ///     println!("{:?} {:?}", watermark.uid, watermark.name);
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn list(&self) -> Result<Vec<Watermark>> {
        let list: Option<Vec<Watermark>> = self
            .client
            .inner
            .json(ApiRequest::get(["watermarks"]))
            .await?;
        Ok(list.unwrap_or_default())
    }

    /// Fetches a watermark profile.
    ///
    /// `GET /accounts/{account_id}/stream/watermarks/{watermark_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let watermark = client.watermarks().get(&"ea95132c15732412d22c1476fa83f27a".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn get(&self, id: &WatermarkId) -> Result<Watermark> {
        let request = ApiRequest::get(["watermarks", id.as_str()]);
        self.client.inner.json(request).await
    }

    /// Deletes a watermark profile. Videos already watermarked keep it.
    ///
    /// `DELETE /accounts/{account_id}/stream/watermarks/{watermark_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// client.watermarks().delete(&"ea95132c15732412d22c1476fa83f27a".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn delete(&self, id: &WatermarkId) -> Result<()> {
        let request = ApiRequest::new(Method::DELETE, ["watermarks", id.as_str()]);
        self.client.inner.empty(request).await
    }
}

/// The image for a watermark profile: uploaded data, or a URL for
/// Cloudflare to download.
#[non_exhaustive]
pub struct WatermarkSource {
    kind: SourceKind,
}

enum SourceKind {
    Bytes { data: Bytes, filename: String },
    Url(Url),
}

impl fmt::Debug for WatermarkSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            SourceKind::Bytes { data, filename } => f
                .debug_struct("WatermarkSource::Bytes")
                .field("filename", filename)
                .field("len", &data.len())
                .finish(),
            SourceKind::Url(url) => f
                .debug_tuple("WatermarkSource::Url")
                .field(&url.as_str())
                .finish(),
        }
    }
}

impl WatermarkSource {
    /// Uploads a PNG from memory.
    ///
    /// ```
    /// let source = cf_stream::WatermarkSource::bytes(vec![0u8; 16], "logo.png");
    /// ```
    pub fn bytes(data: impl Into<Bytes>, filename: impl Into<String>) -> Self {
        Self {
            kind: SourceKind::Bytes {
                data: data.into(),
                filename: filename.into(),
            },
        }
    }

    /// Asks Cloudflare to download the PNG from `url`.
    ///
    /// ```
    /// # fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let source = cf_stream::WatermarkSource::url("https://example.com/logo.png".parse()?);
    /// # Ok(()) }
    /// ```
    pub fn url(url: Url) -> Self {
        Self {
            kind: SourceKind::Url(url),
        }
    }
}

/// A pending watermark profile. Created by [`Watermarks::create`].
#[derive(Debug)]
#[must_use = "a watermark request does nothing until you call `send`"]
#[non_exhaustive]
pub struct CreateWatermarkRequest<'a> {
    client: &'a Client,
    source: WatermarkSource,
    name: Option<String>,
    opacity: Option<f64>,
    padding: Option<f64>,
    scale: Option<f64>,
    position: Option<WatermarkPosition>,
}

impl CreateWatermarkRequest<'_> {
    /// A short description of the profile.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client, source: cf_stream::WatermarkSource) -> cf_stream::Result<()> {
    /// let watermark = client.watermarks().create(source).name("Marketing").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Opacity from `0.0` (transparent) to `1.0` (opaque, the default).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client, source: cf_stream::WatermarkSource) -> cf_stream::Result<()> {
    /// let watermark = client.watermarks().create(source).opacity(0.5).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// Padding from the video's edges, from `0.0` to `1.0` (default `0.05`).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client, source: cf_stream::WatermarkSource) -> cf_stream::Result<()> {
    /// let watermark = client.watermarks().create(source).padding(0.1).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn padding(mut self, padding: f64) -> Self {
        self.padding = Some(padding);
        self
    }

    /// Size relative to the video, from `0.0` (image as-is) to `1.0`
    /// (default `0.15`).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client, source: cf_stream::WatermarkSource) -> cf_stream::Result<()> {
    /// let watermark = client.watermarks().create(source).scale(0.2).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn scale(mut self, scale: f64) -> Self {
        self.scale = Some(scale);
        self
    }

    /// Where the image is placed (default upper right).
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client, source: cf_stream::WatermarkSource) -> cf_stream::Result<()> {
    /// use cf_stream::WatermarkPosition;
    ///
    /// let watermark = client
    ///     .watermarks()
    ///     .create(source)
    ///     .position(WatermarkPosition::Center)
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn position(mut self, position: WatermarkPosition) -> Self {
        self.position = Some(position);
        self
    }

    /// Creates the profile.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`](crate::Error::InvalidInput) if a value is out
    /// of range or the position is
    /// `Unknown`; otherwise as for any API call.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let url = "https://example.com/logo.png".parse().expect("valid URL");
    /// let watermark = client
    ///     .watermarks()
    ///     .create(cf_stream::WatermarkSource::url(url))
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(self) -> Result<Watermark> {
        let mut fields: Vec<(&str, serde_json::Value)> = Vec::new();
        if let Some(name) = self.name {
            fields.push(("name", name.into()));
        }
        for (key, value) in [
            ("opacity", self.opacity),
            ("padding", self.padding),
            ("scale", self.scale),
        ] {
            if let Some(value) = value {
                fields.push((key, crate::util::unit_interval(key, value)?.into()));
            }
        }
        if let Some(position) = &self.position {
            fields.push((
                "position",
                crate::util::wire("watermark position", position)?.into(),
            ));
        }

        let request = ApiRequest::new(Method::POST, ["watermarks"]);
        let request = match self.source.kind {
            SourceKind::Bytes { data, filename } => {
                let len = data.len() as u64;
                let mut form = Form::new().part(
                    "file",
                    Part::stream_with_length(data, len).file_name(filename),
                );
                for (key, value) in fields {
                    let text = match value {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    };
                    form = form.text(key, text);
                }
                request.multipart(form)
            }
            SourceKind::Url(url) => {
                let mut body = Map::new();
                body.insert("url".into(), url.as_str().into());
                for (key, value) in fields {
                    body.insert(key.into(), value);
                }
                request.json(body.into())
            }
        };
        self.client.inner.json(request).await
    }
}
