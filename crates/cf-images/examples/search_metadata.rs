//! List images whose metadata matches filters.
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... cargo run --example search_metadata
//! ```

use anyhow::Context;
use cf_images::{Client, MetaFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new(
        std::env::var("CF_ACCOUNT_ID").context("CF_ACCOUNT_ID")?,
        std::env::var("CF_API_TOKEN").context("CF_API_TOKEN")?,
    )?;

    // Filters are combined with AND. `meta.owner[eq]=user-1&meta.year[gte]=2024`
    let page = client
        .images()
        .list()
        .meta_filter(MetaFilter::eq("owner", "user-1"))
        .meta_filter(MetaFilter::gte("year", 2024))
        .send()
        .await?;

    for image in &page.images {
        println!("{:?}\t{:?}", image.id, image.meta);
    }

    // Which metadata keys can be filtered on?
    println!("{:?}", client.images().metadata_keys().await?.keys);
    Ok(())
}
