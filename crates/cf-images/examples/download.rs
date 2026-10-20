//! Download the original bytes of an image.
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... cargo run --example download -- <image-id> <output-file>
//! ```

use anyhow::Context;
use cf_images::Client;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let usage = "usage: download <image-id> <output-file>";
    let id = args.next().context(usage)?;
    let output = args.next().context(usage)?;
    let client = Client::new(
        std::env::var("CF_ACCOUNT_ID").context("CF_ACCOUNT_ID")?,
        std::env::var("CF_API_TOKEN").context("CF_API_TOKEN")?,
    )?;

    let bytes = client.images().blob(&id.into()).await?;
    tokio::fs::write(&output, &bytes).await?;
    println!("wrote {} bytes to {output}", bytes.len());
    Ok(())
}
