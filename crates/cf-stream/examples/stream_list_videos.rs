//! Stream every video in the account, oldest first.
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... cargo run --example stream_list_videos
//! ```

use anyhow::Context;
use cf_stream::Client;
use futures_util::TryStreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new(
        std::env::var("CF_ACCOUNT_ID").context("CF_ACCOUNT_ID")?,
        std::env::var("CF_API_TOKEN").context("CF_API_TOKEN")?,
    )?;

    let mut videos = std::pin::pin!(client.videos().list().asc(true).into_stream());

    let mut count = 0usize;
    while let Some(video) = videos.try_next().await? {
        count += 1;
        println!(
            "{}\t{}\t{:?}\t{}",
            video.uid.as_ref().map(|i| i.as_str()).unwrap_or("-"),
            video.created.as_deref().unwrap_or("-"),
            video.status.as_ref().and_then(|s| s.state),
            video
                .meta
                .as_ref()
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("-"),
        );
    }
    eprintln!("{count} videos");
    Ok(())
}
