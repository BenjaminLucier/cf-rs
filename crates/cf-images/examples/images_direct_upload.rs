//! Create a one-time upload URL, then upload to it the way a browser would.
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... cargo run --example images_direct_upload -- path/to/cat.png
//! ```

use std::time::Duration;

use anyhow::Context;
use cf_images::Client;
use cf_images::reqwest::multipart::{Form, Part};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("usage: images_direct_upload <file>")?;
    let client = Client::new(
        std::env::var("CF_ACCOUNT_ID").context("CF_ACCOUNT_ID")?,
        std::env::var("CF_API_TOKEN").context("CF_API_TOKEN")?,
    )?;

    // Server side: create the URL and hand it to the end user.
    let upload = client
        .direct_uploads()
        .create()
        .expires_in(Duration::from_secs(10 * 60))
        .metadata(&serde_json::json!({ "uploaded_by": "example" }))
        .send()
        .await?;
    let upload_url = upload.upload_url.context("no uploadURL in response")?;
    let id = upload.id.context("no id in response")?;
    println!("created upload URL for image {id}");

    // Client side: a single unauthenticated multipart POST with a `file` part.
    let data = tokio::fs::read(&path).await?;
    let form = Form::new().part("file", Part::bytes(data).file_name("upload"));
    let response = cf_images::reqwest::Client::new()
        .post(&upload_url)
        .multipart(form)
        .send()
        .await?;
    println!("upload status: {}", response.status());

    let image = client.images().get(&id).await?;
    println!("draft: {:?}, variants: {:?}", image.draft, image.variants);
    Ok(())
}
