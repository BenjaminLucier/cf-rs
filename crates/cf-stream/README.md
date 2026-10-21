# cf-stream

[![crates.io](https://img.shields.io/crates/v/cf-stream.svg)](https://crates.io/crates/cf-stream)
[![docs.rs](https://img.shields.io/docsrs/cf-stream)](https://docs.rs/cf-stream)
![MSRV](https://img.shields.io/badge/MSRV-1.85-blue)
[![CI](https://github.com/BenjaminLucier/cf-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/BenjaminLucier/cf-rs/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/cf-stream.svg)](#license)

Unofficial async Rust client for the [Cloudflare Stream](https://developers.cloudflare.com/stream/) API.

> **Unofficial.** This crate is not affiliated with, endorsed by, or supported
> by Cloudflare, Inc.

- Stream REST endpoints: videos (basic upload, copy from a URL, clip, list
  with a paginating `Stream`, update, delete, storage usage, signed tokens),
  direct creator uploads, tus upload creation, captions (including AI
  generation), audio tracks, downloads, signing keys, live inputs and
  outputs, watermark profiles and webhooks.
- Live status ("is this input broadcasting right now?") from the delivery
  domain's lifecycle endpoint, without sending your API token.
- Offline helpers: playback, thumbnail and download URLs, RS256 signed
  playback tokens, and webhook signature verification.
- The API token, signing keys, stream keys and webhook secrets never appear
  in `Debug` output; responses tolerate new fields and enum values.

```toml
[dependencies]
cf-stream = "0.1"
```

```rust no_run
use cf_stream::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(std::env::var("CF_ACCOUNT_ID")?, std::env::var("CF_API_TOKEN")?)?;

    let video = client
        .videos()
        .copy("https://example.com/intro.mp4".parse()?)
        .meta(&serde_json::json!({ "name": "intro" }))
        .send()
        .await?;
    println!("{:?} {:?}", video.uid, video.status);
    Ok(())
}
```

Signed playback URL for a private video (feature `signing`, on by default):

```rust no_run
use std::time::Duration;
use cf_stream::{CustomerCode, Delivery, TokenSigner};

fn main() -> cf_stream::Result<()> {
    // The key ID and base64 PEM returned once by `signing_keys().create()`.
    let signer = TokenSigner::new("signing-key-id", std::env::var("STREAM_KEY_PEM").unwrap())?;
    let token = signer
        .token("ea95132c15732412d22c1476fa83f27a")
        .expires_in(Duration::from_secs(3600))
        .sign()?;

    let delivery = Delivery::new(CustomerCode::new("m033z5x00ks6nunl"))?;
    println!("{}", delivery.hls(&token));
    Ok(())
}
```

Verify a webhook notification (feature `webhooks`, on by default):

```rust
use cf_stream::{Video, WebhookVerifier};

fn handle(signature: &str, body: &[u8]) -> Result<Video, Box<dyn std::error::Error>> {
    WebhookVerifier::new("webhook-secret").verify(signature, body)?;
    Ok(serde_json::from_slice(body)?)
}
```

The **account ID** (used by the API) and the **customer code** (used in
delivery URLs, `customer-<code>.cloudflarestream.com`) are different values.

## Authentication

Use an [API token](https://developers.cloudflare.com/fundamentals/api/get-started/create-token/)
with **Account > Stream > Edit** (or **Read** for read-only use).

## Large uploads (tus)

Basic and direct uploads accept files up to 200 MB. For larger files, this
crate creates a [tus](https://tus.io) resumable upload with
`direct_uploads().create_tus(len)` and returns its URL; it doesn't upload
the chunks itself. Use any tus client for that:

- in a browser, [tus-js-client](https://github.com/tus/tus-js-client) or
  [Uppy](https://uppy.io/docs/tus/), with `.direct_user(true)` so no API
  token is needed;
- in Rust, for example [`tus-uploader`](https://crates.io/crates/tus-uploader),
  or another client from tus.io's [list of implementations](https://tus.io/implementations).
  Without `direct_user`, send your API token as `Authorization: Bearer ...`
  on every request.

The [`stream_tus_upload`](examples/stream_tus_upload.rs) example is a short upload loop
with resume, using only `reqwest`. See also Cloudflare's guide to
[resumable and large files](https://developers.cloudflare.com/stream/uploading-videos/resumable-uploads/).

## Features

| Feature | Default | |
|---|---|---|
| `rustls-tls` | yes | TLS via rustls |
| `native-tls` | no | TLS via the platform library |
| `signing` | yes | Signed playback tokens (`TokenSigner`) |
| `webhooks` | yes | Webhook signature verification (`WebhookVerifier`) |
| `tracing` | no | `tracing` spans around requests |

## Examples

See [`examples/`](examples):

| Example | Shows |
|---|---|
| `stream_copy_from_url` | Importing a video from a URL and waiting until it's ready |
| `stream_list_videos` | Streaming every video |
| `stream_direct_upload` | One-time upload URLs for end users |
| `stream_tus_direct_creator` | A one-time tus URL for a browser upload |
| `stream_tus_upload` | A resumable tus upload with plain `reqwest` |
| `stream_live_input` | Creating a live input and an output |
| `stream_signed_token` | Signing a playback token (offline) |
| `stream_verify_webhook` | Verifying a webhook signature (offline) |
| `stream_delivery_urls` | Building playback, thumbnail and download URLs (offline) |

## MSRV

Rust 1.85. Raising it is a minor-version change.

## License

MIT OR Apache-2.0.
