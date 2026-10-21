//! Create a one-time tus URL for an end user's large upload. A browser
//! client such as tus-js-client or Uppy then uploads to it without your API
//! token. In a web app, return `location` to the browser (and expose the
//! `Location` header with `Access-Control-Expose-Headers`).
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... cargo run --example stream_tus_direct_creator -- 1500000000
//! ```

use std::time::Duration;

use anyhow::Context;
use cf_stream::Client;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new(
        std::env::var("CF_ACCOUNT_ID").context("CF_ACCOUNT_ID")?,
        std::env::var("CF_API_TOKEN").context("CF_API_TOKEN")?,
    )?;
    // The browser tells your server the file size (its `Upload-Length`).
    let length: u64 = std::env::args()
        .nth(1)
        .context("usage: stream_tus_direct_creator <file size in bytes>")?
        .parse()?;

    let upload = client
        .direct_uploads()
        .create_tus(length)
        .direct_user(true)
        .name("talk.mp4")
        .max_duration_seconds(2 * 3600)
        .expiry(std::time::SystemTime::now() + Duration::from_secs(3600))
        .creator("user-42")
        .send()
        .await?;
    println!("video ID: {:?}", upload.video_id);
    println!("upload URL for the browser: {}", upload.location);
    Ok(())
}
