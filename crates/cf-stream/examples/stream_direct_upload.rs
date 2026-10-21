//! Create a one-time upload URL for an end user, then upload a small file to
//! it as the end user would (no API token).
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... cargo run --example stream_direct_upload -- video.mp4
//! ```

use std::time::Duration;

use anyhow::Context;
use cf_stream::Client;
use cf_stream::reqwest::multipart::{Form, Part};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new(
        std::env::var("CF_ACCOUNT_ID").context("CF_ACCOUNT_ID")?,
        std::env::var("CF_API_TOKEN").context("CF_API_TOKEN")?,
    )?;
    let path = std::env::args()
        .nth(1)
        .context("usage: stream_direct_upload <file>")?;

    // On your server: create the URL and hand it to the client.
    let upload = client
        .direct_uploads()
        .create(3600)
        .expires_in(Duration::from_secs(30 * 60))
        .creator("user-42")
        .send()
        .await?;
    let url = upload.upload_url.context("uploadURL")?;
    println!("video {:?} will be uploaded to {url}", upload.uid);

    // On the end user's side: one multipart POST, at most 200 MB.
    let form = Form::new().part("file", Part::bytes(std::fs::read(&path)?).file_name(path));
    let response = cf_stream::reqwest::Client::new()
        .post(url)
        .multipart(form)
        .send()
        .await?;
    println!("upload: {}", response.status());
    Ok(())
}
