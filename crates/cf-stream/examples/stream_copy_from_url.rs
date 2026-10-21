//! Import a video from a URL and wait until it's ready to stream.
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... cargo run --example stream_copy_from_url -- https://example.com/video.mp4
//! ```

use std::time::Duration;

use anyhow::{Context, bail};
use cf_stream::{Client, VideoState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new(
        std::env::var("CF_ACCOUNT_ID").context("CF_ACCOUNT_ID")?,
        std::env::var("CF_API_TOKEN").context("CF_API_TOKEN")?,
    )?;
    let url = std::env::args()
        .nth(1)
        .context("usage: stream_copy_from_url <video URL>")?;

    let video = client
        .videos()
        .copy(url.parse()?)
        .meta(&serde_json::json!({ "name": "copied with cf-stream" }))
        .send()
        .await?;
    let id = video.uid.context("new videos have a UID")?;
    println!("created {id}");

    loop {
        let video = client.videos().get(&id).await?;
        let status = video
            .status
            .unwrap_or_else(|| unreachable!("videos have a status"));
        match status.state {
            Some(VideoState::Ready) => {
                println!("ready: {:?}", video.playback);
                return Ok(());
            }
            Some(VideoState::Error) => bail!("failed: {:?}", status.error_reason()),
            state => {
                println!("{state:?} {:?}%", status.percent_complete());
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
