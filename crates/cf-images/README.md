# cf-images

[![crates.io](https://img.shields.io/crates/v/cf-images.svg)](https://crates.io/crates/cf-images)
[![docs.rs](https://img.shields.io/docsrs/cf-images)](https://docs.rs/cf-images)
![MSRV](https://img.shields.io/badge/MSRV-1.87-blue)
[![CI](https://github.com/BenjaminLucier/cf-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/BenjaminLucier/cf-rs/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/cf-images.svg)](#license)

Unofficial async Rust client for the [Cloudflare Images](https://developers.cloudflare.com/images/) API.
The official [`cloudflare`](https://crates.io/crates/cloudflare) crate has no
Images endpoints; this crate covers the whole Images API and adds delivery and
signed URL helpers.

> **Unofficial.** This crate is not affiliated with, endorsed by, or supported
> by Cloudflare, Inc.

- Every Images REST endpoint: upload (bytes, streams, or URL), get, update,
  delete, list (v2 with metadata filters, plus a paginating `Stream`),
  download the original, variants, signing keys, usage stats and direct
  creator uploads.
- Offline helpers: `imagedelivery.net` and custom-domain delivery URLs,
  flexible variants, and signed URLs for private images.
- The API token never appears in `Debug` output; responses tolerate new
  fields and enum values.

```toml
[dependencies]
cf-images = "0.1"
```

```rust no_run
use cf_images::{Client, UploadSource};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(std::env::var("CF_ACCOUNT_ID")?, std::env::var("CF_API_TOKEN")?)?;

    let image = client
        .images()
        .upload(UploadSource::bytes(std::fs::read("cat.png")?, "cat.png"))
        .metadata(&serde_json::json!({ "owner": "user-1" }))
        .send()
        .await?;
    println!("{:?}", image.variants);
    Ok(())
}
```

Signed URL for a private image (feature `signing`, on by default):

```rust
use std::time::Duration;
use cf_images::{AccountHash, Delivery, UrlSigner};

fn main() -> cf_images::Result<()> {
    let delivery = Delivery::new(AccountHash::new("your-account-hash"));
    let url = delivery.url(&"image-id".into(), &"public".into());
    let signed = UrlSigner::new("signing-key-value").sign_for(&url, Duration::from_secs(3600))?;
    println!("{signed}");
    Ok(())
}
```

The **account ID** (used by the API) and the **account hash** (used in
delivery URLs) are different values; both are on the Images overview page of
the dashboard.

## Authentication

Use an [API token](https://developers.cloudflare.com/fundamentals/api/get-started/create-token/)
with **Account > Cloudflare Images > Edit** (or **Read** for read-only use).

## Features

| Feature | Default | |
|---|---|---|
| `rustls-tls` | yes | TLS via rustls |
| `native-tls` | no | TLS via the platform library |
| `signing` | yes | Signed delivery URLs |
| `tracing` | no | `tracing` spans around requests |

## Examples

See [`examples/`](examples):

| Example | Shows |
|---|---|
| `images_upload` | Uploading a local file |
| `images_download` | Downloading an image's original bytes |
| `images_list_all` | Streaming every image |
| `images_search_metadata` | Listing with metadata filters |
| `images_variants` | Creating, updating, listing and deleting variants |
| `images_delivery_urls` | Building named, flexible and custom-domain URLs (offline) |
| `images_signed_url` | Signing a URL for a private image (offline) |
| `images_direct_upload` | One-time upload URLs for end users |

## MSRV

Rust 1.87. Raising it is a minor-version change.

## License

MIT OR Apache-2.0.
