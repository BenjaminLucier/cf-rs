//! Stream every image in the account.
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... cargo run --example images_list_all
//! ```

use anyhow::Context;
use cf_images::{Client, SortOrder};
use futures_util::TryStreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new(
        std::env::var("CF_ACCOUNT_ID").context("CF_ACCOUNT_ID")?,
        std::env::var("CF_API_TOKEN").context("CF_API_TOKEN")?,
    )?;

    let mut images = std::pin::pin!(
        client
            .images()
            .list()
            .per_page(1000)
            .sort_order(SortOrder::Asc)
            .into_stream()
    );

    let mut count = 0usize;
    while let Some(image) = images.try_next().await? {
        count += 1;
        println!(
            "{}\t{}\t{}",
            image.id.as_ref().map(|i| i.as_str()).unwrap_or("-"),
            image.uploaded.as_deref().unwrap_or("-"),
            image.filename.as_deref().unwrap_or("-"),
        );
    }
    eprintln!("{count} images");
    Ok(())
}
