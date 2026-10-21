//! Unofficial async Rust client for the [Cloudflare Stream] API.
//!
//! > **Unofficial.** This crate is not affiliated with, endorsed by, or
//! > supported by Cloudflare, Inc. "Cloudflare" is a trademark of
//! > Cloudflare, Inc.
//!
//! It covers the Stream REST API: videos (upload, copy from a URL, clip,
//! list, update, delete), direct creator and tus upload creation, captions,
//! audio tracks, downloads, signing keys, live inputs and outputs (including
//! [live status](LiveInputs::lifecycle)), watermark profiles and webhooks. It also adds three offline helpers: a
//! [delivery URL builder](Delivery), [signed playback tokens][`TokenSigner`]
//! and [webhook signature verification][`WebhookVerifier`].
//!
//! [Cloudflare Stream]: https://developers.cloudflare.com/stream/
//!
//! # Quickstart
//!
//! ```no_run
//! use cf_stream::Client;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Client::new(
//!     std::env::var("CF_ACCOUNT_ID")?,
//!     std::env::var("CF_API_TOKEN")?,
//! )?;
//!
//! // Ask Cloudflare to fetch a video from a URL
//! let video = client
//!     .videos()
//!     .copy("https://example.com/intro.mp4".parse()?)
//!     .meta(&serde_json::json!({ "name": "intro" }))
//!     .send()
//!     .await?;
//! let id = video.uid.expect("new videos have a UID");
//!
//! // Check on it, list, and clean up
//! let video = client.videos().get(&id).await?;
//! println!("{:?}", video.status);
//! let page = client.videos().list().limit(100).send().await?;
//! client.videos().delete(&id).await?;
//! # Ok(()) }
//! ```
//!
//! # Authentication
//!
//! Create an [API token] with the **Account > Stream > Edit** permission
//! (**Read** is enough for read-only use) and pass it to [`Client::new`] or
//! [`Client::builder`]. The token is held in a
//! [`SecretString`](secrecy::SecretString) and never appears in `Debug`
//! output.
//!
//! [API token]: https://developers.cloudflare.com/fundamentals/api/get-started/create-token/
//!
//! # Account ID vs. customer code
//!
//! Stream uses two different account identifiers:
//!
//! | | Used for | Example |
//! |---|---|---|
//! | [`AccountId`] | API calls: `api.cloudflare.com/client/v4/accounts/{account_id}/stream/...` | `023e105f4ecef8ad9ca31a8372d0c353` |
//! | [`CustomerCode`] | Delivery URLs: `customer-{code}.cloudflarestream.com/{video_id}/...` | `m033z5x00ks6nunl` |
//!
//! Both are shown in the dashboard. The customer code also appears in the
//! `playback`, `preview` and `thumbnail` URLs of every [`Video`].
//!
//! # Endpoints
//!
//! Paths are relative to `/accounts/{account_id}/stream`.
//!
//! | Method | HTTP |
//! |---|---|
//! | [`Videos::list`], [`Videos::list_all`] | `GET /` |
//! | [`Videos::get`] | `GET /{video_id}` |
//! | [`Videos::update`] | `POST /{video_id}` |
//! | [`Videos::delete`] | `DELETE /{video_id}` |
//! | [`Videos::upload`] | `POST /` (multipart) |
//! | [`Videos::copy`] | `POST /copy` |
//! | [`Videos::clip`] | `POST /clip` |
//! | [`Videos::storage_usage`] | `GET /storage-usage` |
//! | [`Videos::create_token`] | `POST /{video_id}/token` |
//! | [`DirectUploads::create`] | `POST /direct_upload` |
//! | [`DirectUploads::create_tus`] | `POST /` (tus creation) |
//! | [`Captions::list`] | `GET /{video_id}/captions` |
//! | [`Captions::get`] | `GET /{video_id}/captions/{language}` |
//! | [`Captions::upload`] | `PUT /{video_id}/captions/{language}` (multipart) |
//! | [`Captions::generate`] | `POST /{video_id}/captions/{language}/generate` |
//! | [`Captions::delete`] | `DELETE /{video_id}/captions/{language}` |
//! | [`Captions::vtt`] | `GET /{video_id}/captions/{language}/vtt` |
//! | [`AudioTracks::list`] | `GET /{video_id}/audio` |
//! | [`AudioTracks::copy`] | `POST /{video_id}/audio/copy` |
//! | [`AudioTracks::update`] | `PATCH /{video_id}/audio/{audio_id}` |
//! | [`AudioTracks::delete`] | `DELETE /{video_id}/audio/{audio_id}` |
//! | [`Downloads::get`] | `GET /{video_id}/downloads` |
//! | [`Downloads::create`] | `POST /{video_id}/downloads/{default\|audio}` |
//! | [`Downloads::delete`] | `DELETE /{video_id}/downloads/{default\|audio}` |
//! | [`SigningKeys::create`] | `POST /keys` |
//! | [`SigningKeys::list`] | `GET /keys` |
//! | [`SigningKeys::delete`] | `DELETE /keys/{key_id}` |
//! | [`LiveInputs::list`] | `GET /live_inputs` |
//! | [`LiveInputs::get`] | `GET /live_inputs/{live_input_id}` |
//! | [`LiveInputs::create`] | `POST /live_inputs` |
//! | [`LiveInputs::update`] | `PUT /live_inputs/{live_input_id}` |
//! | [`LiveInputs::delete`] | `DELETE /live_inputs/{live_input_id}` |
//! | [`LiveInputs::enable`], [`disable`](LiveInputs::disable), [`rotate_keys`](LiveInputs::rotate_keys) | `POST /live_inputs/{live_input_id}/{action}` |
//! | [`LiveInputs::lifecycle`] | `GET https://customer-{code}.cloudflarestream.com/{id}/lifecycle` (delivery domain, no token) |
//! | [`LiveOutputs::list`] | `GET /live_inputs/{live_input_id}/outputs` |
//! | [`LiveOutputs::create`] | `POST /live_inputs/{live_input_id}/outputs` |
//! | [`LiveOutputs::update`] | `PUT /live_inputs/{live_input_id}/outputs/{output_id}` |
//! | [`LiveOutputs::delete`] | `DELETE /live_inputs/{live_input_id}/outputs/{output_id}` |
//! | [`Watermarks::create`] | `POST /watermarks` (multipart or JSON) |
//! | [`Watermarks::list`] | `GET /watermarks` |
//! | [`Watermarks::get`] | `GET /watermarks/{watermark_id}` |
//! | [`Watermarks::delete`] | `DELETE /watermarks/{watermark_id}` |
//! | [`Webhooks::get`] | `GET /webhook` |
//! | [`Webhooks::set`] | `PUT /webhook` |
//! | [`Webhooks::delete`] | `DELETE /webhook` |
//!
//! Not covered: the deprecated `GET /{video_id}/embed` (use
//! [`Delivery::iframe`]), the legacy untyped `/{video_id}/downloads`
//! create/delete (use the per-type endpoints), and the usage analytics
//! endpoint.
//!
//! # Large uploads (tus)
//!
//! [`Videos::upload`] and direct upload URLs accept files up to 200 MB.
//! Larger files use the [tus](https://tus.io) resumable upload protocol.
//! This crate only *creates* tus uploads with
//! [`DirectUploads::create_tus`], which returns the upload URL and the
//! future video ID. Sending the data in chunks, resuming and retrying are
//! left to a tus client, because those choices (buffering, retry policy,
//! progress reporting, where to persist the upload URL) depend on the
//! application:
//!
//! - **End users in a browser** (`direct_user(true)`): hand the URL to
//!   [tus-js-client](https://github.com/tus/tus-js-client) or
//!   [Uppy](https://uppy.io/docs/tus/).
//! - **Server-side Rust**: any tus 1.0 client works, such as
//!   [`tus-uploader`](https://crates.io/crates/tus-uploader); see also the
//!   tus.io [list of implementations](https://tus.io/implementations).
//!   Without `direct_user`, every request to the upload URL must also carry
//!   your API token as `Authorization: Bearer ...`.
//! - The `stream_tus_upload` example in the repository is a short, copyable upload
//!   loop built on `reqwest` alone.
//!
//! Cloudflare's guides: [resumable and large files](https://developers.cloudflare.com/stream/uploading-videos/resumable-uploads/)
//! and [direct creator uploads](https://developers.cloudflare.com/stream/uploading-videos/direct-creator-uploads/).
//!
//! # Feature flags
//!
//! | Feature | Default | Description |
//! |---|---|---|
//! | `rustls-tls` | yes | TLS through rustls (reqwest's `rustls` feature). |
//! | `native-tls` | no | TLS through the platform library (reqwest's `native-tls` feature). |
//! | `signing` | yes | [`TokenSigner`] for signed playback tokens (adds `ring`). |
//! | `webhooks` | yes | [`WebhookVerifier`] for webhook signatures (adds `hmac`, `sha2`, `hex`). |
//! | `tracing` | no | A `tracing` span (`cf_stream.request`) around every API request. |
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
//! # async fn run(client: cf_stream::Client, id: cf_stream::VideoId) -> cf_stream::Result<()> {
//! use std::time::Duration;
//!
//! let mut attempt = 0;
//! let video = loop {
//!     match client.videos().get(&id).await {
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
//! Only retry creating requests deliberately: if an upload, copy, clip or
//! direct upload succeeded but its response was lost, retrying creates a
//! second video. Live inputs accept an
//! [idempotency key](CreateLiveInputRequest::idempotency_key) for safe
//! retries. Uploads from a stream can't be replayed, so re-create the
//! [`UploadSource`] for each attempt.
//!
//! # Stability
//!
//! Public structs and enums are `#[non_exhaustive]` so fields and variants can
//! be added in minor releases. Response models ignore unknown JSON fields and
//! map unknown enum values to an `Unknown` variant, so additive API changes
//! don't break deserialization.

// The crate docs link to items that only exist with some features.
#![cfg_attr(
    not(all(feature = "signing", feature = "webhooks")),
    allow(rustdoc::broken_intra_doc_links)
)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![warn(rust_2018_idioms, unreachable_pub)]

mod access;
mod audio;
mod captions;
mod client;
mod de;
mod delivery;
mod direct_upload;
mod downloads;
mod error;
mod keys;
mod live_inputs;
mod options;
#[cfg(feature = "signing")]
mod signing;
mod transport;
mod types;
mod util;
mod videos;
mod watermarks;
#[cfg(feature = "webhooks")]
mod webhook_verify;
mod webhooks;

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

pub use access::{AccessAction, AccessRule};
pub use audio::{AudioTrack, AudioTrackStatus, AudioTracks, UpdateAudioTrack};
pub use captions::{Caption, CaptionStatus, Captions};
pub use client::{Client, ClientBuilder, DEFAULT_BASE_URL};
pub use delivery::{Delivery, ThumbnailFit, ThumbnailOptions};
pub use direct_upload::{
    DirectUpload, DirectUploadRequest, DirectUploads, TusUpload, TusUploadRequest,
};
pub use downloads::{Download, DownloadStatus, DownloadType, Downloads, VideoDownloads};
pub use error::{ApiError, Error, Result};
pub use keys::{SigningKey, SigningKeyInfo, SigningKeys};
pub use live_inputs::{
    CreateLiveInputRequest, Lifecycle, LiveInput, LiveInputList, LiveInputSettings,
    LiveInputStatus, LiveInputSummary, LiveInputs, LiveOutputs, Output, RecordingMode,
    RecordingSettings, RtmpsEndpoint, SrtEndpoint, WebRtcEndpoint,
};
#[cfg(feature = "signing")]
#[cfg_attr(docsrs, doc(cfg(feature = "signing")))]
pub use signing::{TokenBuilder, TokenSigner};
pub use types::{
    AccountId, AudioTrackId, CustomerCode, LiveInputId, OutputId, SigningKeyId, VideoId,
    WatermarkId,
};
pub use videos::{
    ClipRequest, CopyRequest, ListVideosRequest, Playback, PublicDetails, StorageUsage,
    StorageUsageRequest, TokenRequest, UpdateVideo, UploadRequest, UploadSource, Video, VideoInput,
    VideoKind, VideoPage, VideoState, VideoStatus, Videos,
};
pub use watermarks::{
    CreateWatermarkRequest, Watermark, WatermarkPosition, WatermarkSource, Watermarks,
};
#[cfg(feature = "webhooks")]
#[cfg_attr(docsrs, doc(cfg(feature = "webhooks")))]
pub use webhook_verify::{SIGNATURE_HEADER, WebhookError, WebhookVerifier};
pub use webhooks::{Webhook, Webhooks};

/// The `reqwest` version this crate is built on, for [`UploadSource::body`]
/// and for matching on [`Error::Http`].
pub use reqwest;
/// The `secrecy` version used for the API token, keys and stream secrets.
pub use secrecy;
