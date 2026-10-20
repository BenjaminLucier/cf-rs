//! Image endpoints: upload, details, update, delete, listing and download.

use std::collections::VecDeque;
use std::fmt;

use bytes::Bytes;
use futures_core::Stream;
use reqwest::multipart::{Form, Part};
use reqwest::{Method, Url};
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::{Error, Result};
use crate::transport::ApiRequest;
use crate::types::ImageId;

/// Image endpoints. Created by [`Client::images`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Images<'a> {
    client: &'a Client,
}

/// An image stored in Cloudflare Images.
///
/// All fields are optional because the API marks them optional; in practice
/// `id`, `uploaded` and `variants` are always present for a finished upload.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[non_exhaustive]
pub struct Image {
    /// Image identifier.
    pub id: Option<ImageId>,
    /// Original file name, if known.
    pub filename: Option<String>,
    /// Internal user ID set with `creator` at upload or update time.
    pub creator: Option<String>,
    /// User metadata (at most 1024 bytes), as set at upload or update time.
    pub meta: Option<serde_json::Value>,
    /// Whether the image can only be delivered through signed URLs.
    #[serde(rename = "requireSignedURLs")]
    pub require_signed_urls: Option<bool>,
    /// Upload timestamp as returned by the API (an RFC 3339 / ISO 8601
    /// date-time string, e.g. `2014-01-02T02:20:00.123Z`).
    pub uploaded: Option<String>,
    /// Delivery URLs, one per variant.
    pub variants: Option<Vec<String>>,
    /// `true` while a direct creator upload has been created but not yet
    /// completed. Not in the OpenAPI schema; documented on the direct creator
    /// upload guide.
    pub draft: Option<bool>,
}

/// The metadata keys in use across the account's images, for building
/// [`MetaFilter`]s.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct MetadataKeys {
    /// Key names (each at most 100 characters; at most 1000 are returned).
    #[serde(default)]
    pub keys: Vec<String>,
    /// `true` if there are more keys than were returned.
    pub truncated: Option<bool>,
}

impl<'a> Images<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Starts an upload. Configure it with the returned builder and call
    /// [`send`](UploadRequest::send).
    ///
    /// `POST /accounts/{account_id}/images/v1` (multipart/form-data)
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// use cf_images::UploadSource;
    ///
    /// let data = std::fs::read("cat.png").expect("read file");
    /// let image = client
    ///     .images()
    ///     .upload(UploadSource::bytes(data, "cat.png"))
    ///     .metadata(&serde_json::json!({ "owner": "user-1" }))
    ///     .creator("user-1")
    ///     .send()
    ///     .await?;
    /// println!("{:?}", image.id);
    /// # Ok(()) }
    /// ```
    pub fn upload(&self, source: UploadSource) -> UploadRequest<'a> {
        UploadRequest {
            client: self.client,
            source,
            id: None,
            metadata: None,
            require_signed_urls: None,
            creator: None,
        }
    }

    /// Fetches an image's details.
    ///
    /// `GET /accounts/{account_id}/images/v1/{image_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let image = client.images().get(&"my-image-id".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn get(&self, id: &ImageId) -> Result<Image> {
        let request = ApiRequest::get(["v1", id.as_str()]);
        self.client.inner.json(request).await
    }

    /// Updates an image's metadata, creator or signed-URL requirement.
    ///
    /// `PATCH /accounts/{account_id}/images/v1/{image_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// use cf_images::UpdateImage;
    ///
    /// let image = client
    ///     .images()
    ///     .update(&"my-image-id".into(), UpdateImage::new().creator("user-2"))
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn update(&self, id: &ImageId, update: UpdateImage) -> Result<Image> {
        let body = update.into_json()?;
        let request = ApiRequest::new(Method::PATCH, ["v1", id.as_str()]).json(body);
        self.client.inner.json(request).await
    }

    /// Deletes an image and all its variants.
    ///
    /// `DELETE /accounts/{account_id}/images/v1/{image_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// client.images().delete(&"my-image-id".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn delete(&self, id: &ImageId) -> Result<()> {
        let request = ApiRequest::new(Method::DELETE, ["v1", id.as_str()]);
        self.client
            .inner
            .json::<IgnoredAny>(request)
            .await
            .map(drop)
    }

    /// Downloads the original image bytes.
    ///
    /// `GET /accounts/{account_id}/images/v1/{image_id}/blob`
    ///
    /// This endpoint returns raw image data rather than a JSON envelope; API
    /// errors are still reported as [`Error::Api`].
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let bytes = client.images().blob(&"my-image-id".into()).await?;
    /// std::fs::write("original.png", &bytes).expect("write file");
    /// # Ok(()) }
    /// ```
    pub async fn blob(&self, id: &ImageId) -> Result<Bytes> {
        let request = ApiRequest::get(["v1", id.as_str(), "blob"]);
        self.client.inner.raw(request).await
    }

    /// Lists one page of images with the v2 listing API. Configure the
    /// request with the returned builder; follow
    /// [`ImagePage::continuation_token`] or use
    /// [`list_all`](Self::list_all) / [`ListImagesRequest::into_stream`] to
    /// get every page.
    ///
    /// `GET /accounts/{account_id}/images/v2`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// use cf_images::SortOrder;
    ///
    /// let page = client
    ///     .images()
    ///     .list()
    ///     .per_page(100)
    ///     .sort_order(SortOrder::Desc)
    ///     .send()
    ///     .await?;
    /// for image in &page.images {
    ///     println!("{:?}", image.id);
    /// }
    /// # Ok(()) }
    /// ```
    pub fn list(&self) -> ListImagesRequest<'a> {
        ListImagesRequest {
            client: self.client,
            continuation_token: None,
            per_page: None,
            sort_order: None,
            creator: None,
            meta_filters: Vec::new(),
        }
    }

    /// Streams every image, following continuation tokens, with default list
    /// options. To set options, build the request with [`list`](Self::list)
    /// and call [`ListImagesRequest::into_stream`].
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// use futures_util::TryStreamExt;
    ///
    /// let all: Vec<cf_images::Image> = client.images().list_all().try_collect().await?;
    /// # Ok(()) }
    /// ```
    pub fn list_all(&self) -> impl Stream<Item = Result<Image>> + Send + 'a {
        self.list().into_stream()
    }

    /// Lists images with the deprecated v1 page-number API. Prefer
    /// [`list`](Self::list).
    ///
    /// `GET /accounts/{account_id}/images/v1`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// #[allow(deprecated)]
    /// let images = client.images().list_v1().page(2).per_page(50).send().await?;
    /// # Ok(()) }
    /// ```
    #[deprecated(note = "Cloudflare deprecated the v1 list endpoint; use `list()` (v2)")]
    pub fn list_v1(&self) -> ListImagesV1Request<'a> {
        ListImagesV1Request {
            client: self.client,
            page: None,
            per_page: None,
            creator: None,
        }
    }

    /// Lists the metadata keys used by the account's images.
    ///
    /// `GET /accounts/{account_id}/images/v2/metadata/keys`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let keys = client.images().metadata_keys().await?;
    /// println!("{:?} (truncated: {:?})", keys.keys, keys.truncated);
    /// # Ok(()) }
    /// ```
    pub async fn metadata_keys(&self) -> Result<MetadataKeys> {
        let request = ApiRequest::get(["v2", "metadata", "keys"]);
        self.client.inner.json(request).await
    }
}

// ---------------------------------------------------------------------------
// Upload

/// What to upload: file data, a byte stream, or a URL for Cloudflare to fetch.
///
/// Exactly one source is sent with each upload, which is why this is a single
/// value rather than separate `file`/`url` options.
#[non_exhaustive]
pub struct UploadSource {
    kind: SourceKind,
}

enum SourceKind {
    Bytes {
        data: Bytes,
        filename: String,
        content_type: Option<String>,
    },
    Body {
        body: reqwest::Body,
        filename: String,
        length: Option<u64>,
        content_type: Option<String>,
    },
    Url(Url),
}

impl fmt::Debug for UploadSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            SourceKind::Bytes { data, filename, .. } => f
                .debug_struct("UploadSource::Bytes")
                .field("filename", filename)
                .field("len", &data.len())
                .finish(),
            SourceKind::Body {
                filename, length, ..
            } => f
                .debug_struct("UploadSource::Stream")
                .field("filename", filename)
                .field("length", length)
                .finish(),
            SourceKind::Url(url) => f
                .debug_tuple("UploadSource::Url")
                .field(&url.as_str())
                .finish(),
        }
    }
}

impl UploadSource {
    /// Uploads in-memory data as the `file` part, named `filename`.
    ///
    /// ```
    /// let source = cf_images::UploadSource::bytes(vec![0u8; 16], "cat.png");
    /// ```
    pub fn bytes(data: impl Into<Bytes>, filename: impl Into<String>) -> Self {
        Self {
            kind: SourceKind::Bytes {
                data: data.into(),
                filename: filename.into(),
                content_type: None,
            },
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
    ///     Ok::<_, std::io::Error>(Bytes::from_static(b"\x89PNG")),
    /// ]);
    /// let source = cf_images::UploadSource::stream(chunks, "cat.png").with_length(4);
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
    /// let source = cf_images::UploadSource::body(cf_images::reqwest::Body::from("data"), "a.png");
    /// ```
    pub fn body(body: reqwest::Body, filename: impl Into<String>) -> Self {
        Self {
            kind: SourceKind::Body {
                body,
                filename: filename.into(),
                length: None,
                content_type: None,
            },
        }
    }

    /// Asks Cloudflare to fetch the image from `url` (the `url` form field).
    ///
    /// ```
    /// # fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let url = "https://example.com/logo.png".parse()?;
    /// let source = cf_images::UploadSource::url(url);
    /// # Ok(()) }
    /// ```
    pub fn url(url: Url) -> Self {
        Self {
            kind: SourceKind::Url(url),
        }
    }

    /// Sets the MIME type of the file part (e.g. `image/png`). Ignored for URL
    /// uploads. Without it no part content type is sent and Cloudflare
    /// detects the format from the data.
    ///
    /// ```
    /// let source = cf_images::UploadSource::bytes(vec![0u8; 16], "cat.png").with_content_type("image/png");
    /// ```
    pub fn with_content_type(mut self, mime: impl Into<String>) -> Self {
        match &mut self.kind {
            SourceKind::Bytes { content_type, .. } | SourceKind::Body { content_type, .. } => {
                *content_type = Some(mime.into());
            }
            SourceKind::Url(_) => {}
        }
        self
    }

    /// Declares the exact length of a streamed body so it can be sent with a
    /// `Content-Length` instead of chunked encoding. Ignored for other sources.
    ///
    /// ```
    /// let source = cf_images::UploadSource::body(cf_images::reqwest::Body::from("data"), "a.png").with_length(4);
    /// ```
    pub fn with_length(mut self, len: u64) -> Self {
        if let SourceKind::Body { length, .. } = &mut self.kind {
            *length = Some(len);
        }
        self
    }

    fn add_to(self, form: Form) -> Result<Form> {
        fn finish(part: Part, filename: String, content_type: Option<String>) -> Result<Part> {
            let part = part.file_name(filename);
            match content_type {
                Some(ct) => part
                    .mime_str(&ct)
                    .map_err(|e| Error::InvalidInput(format!("invalid content type {ct:?}: {e}"))),
                None => Ok(part),
            }
        }
        Ok(match self.kind {
            SourceKind::Bytes {
                data,
                filename,
                content_type,
            } => {
                let len = data.len() as u64;
                let part = Part::stream_with_length(data, len);
                form.part("file", finish(part, filename, content_type)?)
            }
            SourceKind::Body {
                body,
                filename,
                length,
                content_type,
            } => {
                let part = match length {
                    Some(len) => Part::stream_with_length(body, len),
                    None => Part::stream(body),
                };
                form.part("file", finish(part, filename, content_type)?)
            }
            SourceKind::Url(url) => form.text("url", url.to_string()),
        })
    }
}

/// A pending image upload. Created by [`Images::upload`].
#[must_use = "an upload does nothing until you call `send`"]
#[non_exhaustive]
pub struct UploadRequest<'a> {
    client: &'a Client,
    source: UploadSource,
    id: Option<ImageId>,
    metadata: Option<Metadata>,
    require_signed_urls: Option<bool>,
    creator: Option<String>,
}

impl fmt::Debug for UploadRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadRequest")
            .field("source", &self.source)
            .field("id", &self.id)
            .field("metadata", &self.metadata)
            .field("require_signed_urls", &self.require_signed_urls)
            .field("creator", &self.creator)
            .finish_non_exhaustive()
    }
}

impl UploadRequest<'_> {
    /// Sets a custom image ID instead of a generated one. Custom IDs may
    /// contain `/` but can't be combined with `require_signed_urls(true)`.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// # let source = cf_images::UploadSource::bytes(vec![], "a.png");
    /// client.images().upload(source).id("products/42/front").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn id(mut self, id: impl Into<ImageId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Attaches user metadata (any JSON-serializable value, at most 1024
    /// bytes once serialized). Serialization errors are reported by `send`.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// # let source = cf_images::UploadSource::bytes(vec![], "a.png");
    /// client.images().upload(source).metadata(&serde_json::json!({"sku": 42})).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn metadata<T: Serialize + ?Sized>(mut self, metadata: &T) -> Self {
        self.metadata = Some(Metadata::capture(metadata));
        self
    }

    /// Requires signed URLs to deliver this image. See
    /// `UrlSigner` (feature `signing`).
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// # let source = cf_images::UploadSource::bytes(vec![], "a.png");
    /// client.images().upload(source).require_signed_urls(true).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn require_signed_urls(mut self, required: bool) -> Self {
        self.require_signed_urls = Some(required);
        self
    }

    /// Sets the `creator` field to an internal user ID (at most 1024 characters).
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// # let source = cf_images::UploadSource::bytes(vec![], "a.png");
    /// client.images().upload(source).creator("user-1").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn creator(mut self, creator: impl Into<String>) -> Self {
        self.creator = Some(creator.into());
        self
    }

    /// Uploads the image and returns its details.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the ID is empty, the metadata can't be
    /// serialized or the content type is invalid; otherwise as for any API
    /// call.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let url = "https://example.com/logo.png".parse().expect("valid URL");
    /// let image = client.images().upload(cf_images::UploadSource::url(url)).send().await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(self) -> Result<Image> {
        let mut form = self.source.add_to(Form::new())?;
        if let Some(id) = self.id {
            crate::types::require_non_empty("image ID", id.as_str())?;
            form = form.text("id", id.into_inner());
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
        let request = ApiRequest::new(Method::POST, ["v1"]).multipart(form);
        self.client.inner.json(request).await
    }
}

/// User metadata captured by a builder. Serialization happens when the
/// setter is called, so the builder needn't hold a borrow; any error is
/// reported when the request is sent.
#[derive(Clone, Debug)]
pub(crate) struct Metadata(Result<serde_json::Value, String>);

impl Metadata {
    pub(crate) fn capture<T: Serialize + ?Sized>(metadata: &T) -> Self {
        Self(serde_json::to_value(metadata).map_err(|e| e.to_string()))
    }

    pub(crate) fn into_value(self) -> Result<serde_json::Value> {
        self.0
            .map_err(|e| Error::InvalidInput(format!("metadata: {e}")))
    }

    /// The metadata as JSON text, as multipart form fields require.
    pub(crate) fn into_json_string(self) -> Result<String> {
        Ok(self.into_value()?.to_string())
    }
}

// ---------------------------------------------------------------------------
// Update

/// Changes to apply with [`Images::update`]. Unset fields are left unchanged.
///
/// ```
/// let update = cf_images::UpdateImage::new()
///     .metadata(&serde_json::json!({"reviewed": true}))
///     .require_signed_urls(false);
/// ```
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct UpdateImage {
    metadata: Option<Metadata>,
    require_signed_urls: Option<bool>,
    creator: Option<String>,
}

impl UpdateImage {
    /// An update that changes nothing; add fields with the setters.
    ///
    /// ```
    /// let update = cf_images::UpdateImage::new();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the image's metadata. Serialization errors are reported when
    /// the update is sent.
    ///
    /// ```
    /// let update = cf_images::UpdateImage::new().metadata(&serde_json::json!({"k": "v"}));
    /// ```
    pub fn metadata<T: Serialize + ?Sized>(mut self, metadata: &T) -> Self {
        self.metadata = Some(Metadata::capture(metadata));
        self
    }

    /// Changes whether signed URLs are required. Cloudflare documents that
    /// changing this "returns a new UID", so use the ID in the returned image.
    ///
    /// ```
    /// let update = cf_images::UpdateImage::new().require_signed_urls(true);
    /// ```
    pub fn require_signed_urls(mut self, required: bool) -> Self {
        self.require_signed_urls = Some(required);
        self
    }

    /// Changes the `creator` field.
    ///
    /// ```
    /// let update = cf_images::UpdateImage::new().creator("user-2");
    /// ```
    pub fn creator(mut self, creator: impl Into<String>) -> Self {
        self.creator = Some(creator.into());
        self
    }

    fn into_json(self) -> Result<serde_json::Value> {
        let mut body = serde_json::Map::new();
        if let Some(metadata) = self.metadata {
            body.insert("metadata".into(), metadata.into_value()?);
        }
        if let Some(required) = self.require_signed_urls {
            body.insert("requireSignedURLs".into(), required.into());
        }
        if let Some(creator) = self.creator {
            body.insert("creator".into(), creator.into());
        }
        Ok(body.into())
    }
}

// ---------------------------------------------------------------------------
// List v2

/// Sort order by upload time for [`Images::list`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum SortOrder {
    /// Oldest first.
    Asc,
    /// Newest first (the API default).
    Desc,
    /// A value this version of the crate doesn't recognize. Can't be sent.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// Operators for metadata filters (`meta.<field>[<operator>]=<value>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum MetaOperator {
    /// Exact match, type inferred.
    Eq,
    /// Exact string match.
    #[serde(rename = "eq:string")]
    EqString,
    /// Exact numeric match.
    #[serde(rename = "eq:number")]
    EqNumber,
    /// Exact boolean match.
    #[serde(rename = "eq:boolean")]
    EqBoolean,
    /// Greater than (numbers only).
    Gt,
    /// Greater than or equal (numbers only).
    Gte,
    /// Less than (numbers only).
    Lt,
    /// Less than or equal (numbers only).
    Lte,
    /// Any of a `|`-separated list, type inferred.
    In,
    /// Any of a list of strings.
    #[serde(rename = "in:string")]
    InString,
    /// Any of a list of numbers.
    #[serde(rename = "in:number")]
    InNumber,
    /// A value this version of the crate doesn't recognize. Can't be sent.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// A metadata filter for [`ListImagesRequest::meta_filter`].
///
/// Cloudflare allows at most 5 filters per request, 5 levels of nesting in
/// `field` (`a.b.c`), and 10 values for `in` operators. Filters are combined
/// with AND.
///
/// ```
/// use cf_images::{MetaFilter, MetaOperator};
///
/// let by_status = MetaFilter::eq("status", "active");
/// let recent = MetaFilter::gte("year", 2024);
/// let tagged = MetaFilter::any_of("tag", ["cat", "dog"]);
/// let custom = MetaFilter::new("owner.id", MetaOperator::EqString, "42");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct MetaFilter {
    field: String,
    operator: MetaOperator,
    value: String,
}

impl MetaFilter {
    /// A filter with an explicit operator and pre-formatted value.
    ///
    /// ```
    /// let f = cf_images::MetaFilter::new("flags.public", cf_images::MetaOperator::EqBoolean, "true");
    /// ```
    pub fn new(field: impl Into<String>, operator: MetaOperator, value: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            operator,
            value: value.into(),
        }
    }

    /// `field` equals `value`.
    ///
    /// ```
    /// let f = cf_images::MetaFilter::eq("status", "active");
    /// ```
    pub fn eq(field: impl Into<String>, value: impl fmt::Display) -> Self {
        Self::new(field, MetaOperator::Eq, value.to_string())
    }

    /// `field` > `value`.
    ///
    /// ```
    /// let f = cf_images::MetaFilter::gt("size", 100);
    /// ```
    pub fn gt(field: impl Into<String>, value: impl fmt::Display) -> Self {
        Self::new(field, MetaOperator::Gt, value.to_string())
    }

    /// `field` >= `value`.
    ///
    /// ```
    /// let f = cf_images::MetaFilter::gte("size", 100);
    /// ```
    pub fn gte(field: impl Into<String>, value: impl fmt::Display) -> Self {
        Self::new(field, MetaOperator::Gte, value.to_string())
    }

    /// `field` < `value`.
    ///
    /// ```
    /// let f = cf_images::MetaFilter::lt("size", 100);
    /// ```
    pub fn lt(field: impl Into<String>, value: impl fmt::Display) -> Self {
        Self::new(field, MetaOperator::Lt, value.to_string())
    }

    /// `field` <= `value`.
    ///
    /// ```
    /// let f = cf_images::MetaFilter::lte("size", 100.5);
    /// ```
    pub fn lte(field: impl Into<String>, value: impl fmt::Display) -> Self {
        Self::new(field, MetaOperator::Lte, value.to_string())
    }

    /// `field` is any of `values` (joined with `|`).
    ///
    /// ```
    /// let f = cf_images::MetaFilter::any_of("state", ["pending", "flagged"]);
    /// ```
    pub fn any_of<I>(field: impl Into<String>, values: I) -> Self
    where
        I: IntoIterator,
        I::Item: fmt::Display,
    {
        let joined = values
            .into_iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join("|");
        Self::new(field, MetaOperator::In, joined)
    }

    fn query_pair(&self) -> Result<(String, String)> {
        let op = crate::types::wire_str(&self.operator)
            .ok_or_else(|| Error::InvalidInput("unknown metadata filter operator".into()))?;
        crate::types::require_non_empty("metadata filter field", &self.field)?;
        Ok((format!("meta.{}[{op}]", self.field), self.value.clone()))
    }
}

/// One page of images from the v2 listing API.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct ImagePage {
    /// The images on this page.
    #[serde(default)]
    pub images: Vec<Image>,
    /// Pass to [`ListImagesRequest::continuation_token`] to get the next
    /// page. `None` on the last page.
    pub continuation_token: Option<String>,
}

/// A v2 image listing request. Created by [`Images::list`].
#[derive(Clone, Debug)]
#[must_use = "a list request does nothing until you call `send` or `into_stream`"]
#[non_exhaustive]
pub struct ListImagesRequest<'a> {
    client: &'a Client,
    continuation_token: Option<String>,
    per_page: Option<u32>,
    sort_order: Option<SortOrder>,
    creator: Option<String>,
    meta_filters: Vec<MetaFilter>,
}

impl<'a> ListImagesRequest<'a> {
    /// Resumes listing from a previous page's continuation token.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let first = client.images().list().send().await?;
    /// if let Some(token) = first.continuation_token {
    ///     let second = client.images().list().continuation_token(token).send().await?;
    /// }
    /// # Ok(()) }
    /// ```
    pub fn continuation_token(mut self, token: impl Into<String>) -> Self {
        self.continuation_token = Some(token.into());
        self
    }

    /// Images per page (10 to 10 000; the API default is 1000).
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let page = client.images().list().per_page(50).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn per_page(mut self, per_page: u32) -> Self {
        self.per_page = Some(per_page);
        self
    }

    /// Sort order by upload time.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let page = client.images().list().sort_order(cf_images::SortOrder::Asc).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn sort_order(mut self, order: SortOrder) -> Self {
        self.sort_order = Some(order);
        self
    }

    /// Only images with this `creator`. An empty string selects images with
    /// no creator.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let page = client.images().list().creator("user-1").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn creator(mut self, creator: impl Into<String>) -> Self {
        self.creator = Some(creator.into());
        self
    }

    /// Adds a metadata filter. Multiple filters are combined with AND.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// use cf_images::MetaFilter;
    ///
    /// let page = client
    ///     .images()
    ///     .list()
    ///     .meta_filter(MetaFilter::eq("status", "published"))
    ///     .meta_filter(MetaFilter::gte("width", 1024))
    ///     .send()
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub fn meta_filter(mut self, filter: MetaFilter) -> Self {
        self.meta_filters.push(filter);
        self
    }

    fn build(&self) -> Result<ApiRequest> {
        let sort_order = match &self.sort_order {
            Some(order) => Some(
                crate::types::wire_str(order)
                    .ok_or_else(|| Error::InvalidInput("unknown sort order".into()))?,
            ),
            None => None,
        };
        let mut request = ApiRequest::get(["v2"])
            .query_opt("continuation_token", self.continuation_token.as_deref())
            .query_opt("per_page", self.per_page)
            .query_opt("sort_order", sort_order)
            .query_opt("creator", self.creator.as_deref());
        for filter in &self.meta_filters {
            let (key, value) = filter.query_pair()?;
            request = request.query(key, value);
        }
        Ok(request)
    }

    /// Fetches one page.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let page = client.images().list().send().await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(&self) -> Result<ImagePage> {
        let request = self.build()?;
        self.client.inner.json(request).await
    }

    /// Streams every image from this request onwards, fetching pages lazily
    /// and following continuation tokens. The stream ends after the first
    /// error.
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// use futures_util::TryStreamExt;
    ///
    /// let mut images = std::pin::pin!(client.images().list().per_page(1000).into_stream());
    /// while let Some(image) = images.try_next().await? {
    ///     println!("{:?}", image.id);
    /// }
    /// # Ok(()) }
    /// ```
    pub fn into_stream(self) -> impl Stream<Item = Result<Image>> + Send + 'a {
        struct State<'a> {
            request: ListImagesRequest<'a>,
            buffer: VecDeque<Image>,
            done: bool,
        }

        let state = State {
            request: self,
            buffer: VecDeque::new(),
            done: false,
        };

        futures_util::stream::unfold(state, |mut state| async move {
            loop {
                if let Some(image) = state.buffer.pop_front() {
                    return Some((Ok(image), state));
                }
                if state.done {
                    return None;
                }
                match state.request.send().await {
                    Ok(page) => {
                        state.buffer.extend(page.images);
                        match page.continuation_token.filter(|t| !t.is_empty()) {
                            // Stop if the API ever hands back the token we just
                            // used, rather than looping forever.
                            Some(token)
                                if state.request.continuation_token.as_deref()
                                    != Some(token.as_str()) =>
                            {
                                state.request.continuation_token = Some(token);
                            }
                            _ => state.done = true,
                        }
                    }
                    Err(err) => {
                        state.done = true;
                        return Some((Err(err), state));
                    }
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// List v1 (deprecated)

#[derive(Deserialize)]
struct ImageListV1 {
    #[serde(default)]
    images: Vec<Image>,
}

/// A v1 (page-number) image listing request. Created by
/// [`Images::list_v1`].
#[derive(Clone, Debug)]
#[must_use = "a list request does nothing until you call `send`"]
#[non_exhaustive]
pub struct ListImagesV1Request<'a> {
    client: &'a Client,
    page: Option<u32>,
    per_page: Option<u32>,
    creator: Option<String>,
}

impl ListImagesV1Request<'_> {
    /// Page number, starting at 1.
    ///
    /// ```no_run
    /// # #![allow(deprecated)]
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let images = client.images().list_v1().page(3).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    /// Images per page (10 to 10 000).
    ///
    /// ```no_run
    /// # #![allow(deprecated)]
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let images = client.images().list_v1().per_page(100).send().await?;
    /// # Ok(()) }
    /// ```
    pub fn per_page(mut self, per_page: u32) -> Self {
        self.per_page = Some(per_page);
        self
    }

    /// Only images with this `creator`.
    ///
    /// ```no_run
    /// # #![allow(deprecated)]
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let images = client.images().list_v1().creator("user-1").send().await?;
    /// # Ok(()) }
    /// ```
    pub fn creator(mut self, creator: impl Into<String>) -> Self {
        self.creator = Some(creator.into());
        self
    }

    /// Fetches the page.
    ///
    /// ```no_run
    /// # #![allow(deprecated)]
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let images = client.images().list_v1().send().await?;
    /// # Ok(()) }
    /// ```
    pub async fn send(&self) -> Result<Vec<Image>> {
        let request = ApiRequest::get(["v1"])
            .query_opt("page", self.page)
            .query_opt("per_page", self.per_page)
            .query_opt("creator", self.creator.as_deref());
        let list: ImageListV1 = self.client.inner.json(request).await?;
        Ok(list.images)
    }
}
