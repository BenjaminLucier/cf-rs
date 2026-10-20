//! Upload a local file.
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... cargo run --example upload -- path/to/cat.png
//! ```

use anyhow::Context;
use cf_images::{Client, UploadSource};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).context("usage: upload <file>")?;
    let client = Client::new(
        std::env::var("CF_ACCOUNT_ID").context("CF_ACCOUNT_ID")?,
        std::env::var("CF_API_TOKEN").context("CF_API_TOKEN")?,
    )?;

    let filename = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("upload")
        .to_owned();

    // Stream the file instead of reading it into memory.
    let file = tokio::fs::File::open(&path).await?;
    let len = file.metadata().await?.len();
    let source = UploadSource::body(file.into(), filename).with_length(len);

    let image = client
        .images()
        .upload(source)
        .metadata(&serde_json::json!({ "source": "cf-images example" }))
        .send()
        .await?;

    println!("uploaded {:?}", image.id);
    for url in image.variants.unwrap_or_default() {
        println!("  {url}");
    }
    Ok(())
}
