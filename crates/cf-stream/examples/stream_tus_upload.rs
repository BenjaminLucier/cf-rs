//! A resumable tus upload from your server, with `reqwest` only.
//!
//! cf-stream creates the upload; this example sends the file in chunks and
//! resumes from the server's offset after a failure. It's a starting point
//! to copy, not a full tus client: there is no progress reporting, the
//! upload URL isn't persisted, and failures are retried a fixed number of
//! times. A dedicated tus client may suit you better; see the crate docs.
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... cargo run --example stream_tus_upload -- big-video.mp4
//! ```

use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

use anyhow::{Context, bail};
use cf_stream::Client;
use cf_stream::reqwest::{self, StatusCode, Url};

/// 50 MiB: a multiple of 256 KiB, as Cloudflare requires for every chunk but
/// the last (and at least 5 MiB).
const CHUNK: u64 = 50 * 1024 * 1024;
const MAX_RETRIES: u32 = 3;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let account = std::env::var("CF_ACCOUNT_ID").context("CF_ACCOUNT_ID")?;
    let token = std::env::var("CF_API_TOKEN").context("CF_API_TOKEN")?;
    let path = std::env::args()
        .nth(1)
        .context("usage: stream_tus_upload <file>")?;
    let mut file = std::fs::File::open(&path)?;
    let length = file.metadata()?.len();

    let name = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());

    let client = Client::new(account, token.clone())?;
    let upload = client
        .direct_uploads()
        .create_tus(length)
        .name(name)
        .send()
        .await?;
    println!(
        "uploading video {:?} to {}",
        upload.video_id, upload.location
    );

    // Server-side uploads authenticate every request with the API token.
    // Only send it to Cloudflare over HTTPS (uploads currently go to
    // `edge-production.gateway.api.cloudflare.com`).
    let host = upload.location.host_str().unwrap_or_default();
    let cloudflare = ["cloudflare.com", "cloudflarestream.com"]
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")));
    if upload.location.scheme() != "https" || !cloudflare {
        bail!("unexpected upload URL {}", upload.location);
    }

    let http = reqwest::Client::new();
    let mut offset = 0;
    let mut retries = 0;
    while offset < length {
        let len = CHUNK.min(length - offset);
        let mut chunk = vec![0; len as usize];
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut chunk)?;

        match patch(&http, &upload.location, &token, offset, chunk).await {
            Ok(next) => {
                offset = next;
                retries = 0;
                println!("{offset}/{length} bytes");
            }
            Err(err) if retries < MAX_RETRIES => {
                retries += 1;
                eprintln!("chunk failed ({err:#}); resuming");
                tokio::time::sleep(Duration::from_secs(2u64.pow(retries))).await;
                offset = server_offset(&http, &upload.location, &token).await?;
            }
            Err(err) => return Err(err),
        }
    }
    println!("done");
    Ok(())
}

/// Sends one chunk and returns the server's new offset.
async fn patch(
    http: &reqwest::Client,
    location: &Url,
    token: &str,
    offset: u64,
    chunk: Vec<u8>,
) -> anyhow::Result<u64> {
    let response = http
        .patch(location.clone())
        .bearer_auth(token)
        .header("Tus-Resumable", "1.0.0")
        .header("Upload-Offset", offset.to_string())
        .header("Content-Type", "application/offset+octet-stream")
        .body(chunk)
        .send()
        .await?;
    if response.status() != StatusCode::NO_CONTENT {
        bail!("PATCH returned {}", response.status());
    }
    upload_offset(&response)
}

/// Asks the server how much it has received.
async fn server_offset(http: &reqwest::Client, location: &Url, token: &str) -> anyhow::Result<u64> {
    let response = http
        .head(location.clone())
        .bearer_auth(token)
        .header("Tus-Resumable", "1.0.0")
        .send()
        .await?
        .error_for_status()?;
    upload_offset(&response)
}

fn upload_offset(response: &reqwest::Response) -> anyhow::Result<u64> {
    Ok(response
        .headers()
        .get("Upload-Offset")
        .context("no Upload-Offset header")?
        .to_str()?
        .parse()?)
}
