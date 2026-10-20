//! Unofficial async Rust client for the [Cloudflare Images] API.
//!
//! > **Unofficial.** This crate is not affiliated with, endorsed by, or
//! > supported by Cloudflare, Inc. "Cloudflare" is a trademark of
//! > Cloudflare, Inc.
//!
//! It covers the whole Images REST API (uploads, image management, variants,
//! signing keys, usage statistics and direct creator uploads) and adds two
//! offline helpers: a [delivery URL builder](Delivery) and
//! [signed URL generation][`UrlSigner`] for private images.
//!
//! [Cloudflare Images]: https://developers.cloudflare.com/images/
//!
//! # Quickstart
//!
//! ```no_run
//! use cf_images::{Client, UploadSource};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Client::new(
//!     std::env::var("CF_ACCOUNT_ID")?,
//!     std::env::var("CF_API_TOKEN")?,
//! )?;
//!
//! // Upload
//! let data = std::fs::read("cat.png")?;
//! let image = client
//!     .images()
//!     .upload(UploadSource::bytes(data, "cat.png"))
//!     .metadata(&serde_json::json!({ "owner": "user-1" }))
//!     .send()
//!     .await?;
//! let id = image.id.expect("uploaded images have an ID");
//!
//! // Inspect and list
//! let image = client.images().get(&id).await?;
//! let page = client.images().list().per_page(100).send().await?;
//!
//! // Clean up
//! client.images().delete(&id).await?;
//! # Ok(()) }
//! ```
//!
//! # Authentication
//!
//! Create an [API token] with the **Account > Cloudflare Images > Edit**
//! permission (**Read** is enough for read-only use) and pass it to
//! [`Client::new`] or [`Client::builder`]. The token is held in a
//! [`SecretString`](secrecy::SecretString) and never appears in `Debug`
//! output.
//!
//! [API token]: https://developers.cloudflare.com/fundamentals/api/get-started/create-token/
//!
//! # Account ID vs. account hash
//!
//! Cloudflare Images uses two different account identifiers:
//!
//! | | Used for | Example |
//! |---|---|---|
//! | [`AccountId`] | API calls: `api.cloudflare.com/client/v4/accounts/{account_id}/images/...` | `023e105f4ecef8ad9ca31a8372d0c353` |
//! | [`AccountHash`] | Delivery URLs: `imagedelivery.net/{account_hash}/{image_id}/{variant}` | `ZWd9g1K7eljCn_KDTu_MWA` |
//!
//! Both are shown on the **Images > Overview** page of the dashboard. The
//! account hash also appears in every URL of [`Image::variants`].
//!
//! # Endpoints
//!
//! Paths are relative to `/accounts/{account_id}/images`.
//!
//! | Method | HTTP |
//! |---|---|
//! | [`Images::upload`] | `POST /v1` (multipart) |
//! | [`Images::get`] | `GET /v1/{image_id}` |
//! | [`Images::update`] | `PATCH /v1/{image_id}` |
//! | [`Images::delete`] | `DELETE /v1/{image_id}` |
//! | [`Images::blob`] | `GET /v1/{image_id}/blob` |
//! | [`Images::list`], [`Images::list_all`] | `GET /v2` |
//! | [`Images::list_v1`] (deprecated) | `GET /v1` |
//! | [`Images::metadata_keys`] | `GET /v2/metadata/keys` |
//! | [`Variants::list`] | `GET /v1/variants` |
//! | [`Variants::get`] | `GET /v1/variants/{variant_id}` |
//! | [`Variants::create`] | `POST /v1/variants` |
//! | [`Variants::update`] | `PATCH /v1/variants/{variant_id}` |
//! | [`Variants::delete`] | `DELETE /v1/variants/{variant_id}` |
//! | [`SigningKeys::list`] | `GET /v1/keys` |
//! | [`SigningKeys::create`] | `PUT /v1/keys/{signing_key_name}` |
//! | [`SigningKeys::delete`] | `DELETE /v1/keys/{signing_key_name}` |
//! | [`Client::stats`] | `GET /v1/stats` |
//! | [`DirectUploads::create`] | `POST /v2/direct_upload` (multipart) |
//!
//! # Feature flags
//!
//! | Feature | Default | Description |
//! |---|---|---|
//! | `rustls-tls` | yes | TLS through rustls (reqwest's `rustls` feature). |
//! | `native-tls` | no | TLS through the platform library (reqwest's `native-tls` feature). |
//! | `signing` | yes | [`UrlSigner`] for signed delivery URLs (adds `hmac`, `sha2`, `hex`). |
//! | `tracing` | no | A `tracing` span (`cf_images.request`) around every API request. |
//!
//! With neither TLS feature enabled the client can only reach `http://` base
//! URLs (useful against a local mock server).
//!
//! # Errors and retries
//!
//! Every fallible call returns [`Error`]. API errors carry the HTTP status
//! and Cloudflare's error codes; [`Error::is_not_found`],
//! [`Error::is_rate_limited`], [`Error::is_unauthorized`] and
//! [`Error::retry_after`] cover common checks.
//!
//! The client doesn't retry. Wrap calls in your own loop where you need it:
//!
//! ```no_run
//! # async fn run(client: cf_images::Client, id: cf_images::ImageId) -> cf_images::Result<()> {
//! use std::time::Duration;
//!
//! let mut attempt = 0;
//! let image = loop {
//!     match client.images().get(&id).await {
//!         Err(e) if e.is_rate_limited() && attempt < 3 => {
//!             attempt += 1;
//!             let wait = e.retry_after().unwrap_or(Duration::from_secs(1 << attempt));
//!             tokio::time::sleep(wait).await;
//!         }
//!         other => break other?,
//!     }
//! };
//! # Ok(()) }
//! ```
//!
//! Only retry uploads deliberately: if an upload succeeded but its response
//! was lost, retrying creates a second image unless you set a custom ID.
//! Uploads from a stream can't be replayed, so re-create the
//! [`UploadSource`] for each attempt.
//!
//! # Stability
//!
//! Public structs and enums are `#[non_exhaustive]` so fields and variants can
//! be added in minor releases. Response models ignore unknown JSON fields and
//! map unknown enum values to an `Unknown` variant, so additive API changes
//! don't break deserialization.

// The crate docs link to `UrlSigner`, which only exists with `signing`.
#![cfg_attr(not(feature = "signing"), allow(rustdoc::broken_intra_doc_links))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![warn(rust_2018_idioms, unreachable_pub)]

mod client;
mod delivery;
mod direct_upload;
mod error;
mod images;
mod keys;
#[cfg(feature = "signing")]
mod signing;
mod stats;
mod transport;
mod types;
mod variants;

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

pub use client::{Client, ClientBuilder, DEFAULT_BASE_URL};
pub use delivery::{DEFAULT_DELIVERY_BASE, Delivery, FlexibleVariant};
pub use direct_upload::{DirectUpload, DirectUploadRequest, DirectUploads};
pub use error::{ApiError, Error, Result};
pub use images::{
    Image, ImagePage, Images, ListImagesRequest, ListImagesV1Request, MetaFilter, MetaOperator,
    MetadataKeys, SortOrder, UpdateImage, UploadRequest, UploadSource,
};
pub use keys::{SigningKey, SigningKeys};
#[cfg(feature = "signing")]
#[cfg_attr(docsrs, doc(cfg(feature = "signing")))]
pub use signing::UrlSigner;
pub use stats::{Stats, StatsCount};
pub use types::{AccountHash, AccountId, ImageId, SigningKeyName, VariantName};
pub use variants::{
    Fit, MetadataBehavior, NewVariant, UpdateVariant, Variant, VariantOptions, Variants,
};

/// The `reqwest` version this crate is built on, for [`UploadSource::body`]
/// and for matching on [`Error::Http`].
pub use reqwest;
/// The `secrecy` version used for the API token and signing keys.
pub use secrecy;
