//! Create, update, list and delete a variant.
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... cargo run --example images_variants
//! ```

use anyhow::Context;
use cf_images::{Client, Fit, NewVariant, UpdateVariant, VariantOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new(
        std::env::var("CF_ACCOUNT_ID").context("CF_ACCOUNT_ID")?,
        std::env::var("CF_API_TOKEN").context("CF_API_TOKEN")?,
    )?;
    let variants = client.variants();

    let name = "example_thumbnail".into();
    let created = variants
        .create(NewVariant::new(
            "example_thumbnail",
            VariantOptions::new(Fit::Cover, 200, 200),
        ))
        .await?;
    println!("created {}", created.id);

    let update = UpdateVariant::new(VariantOptions::new(Fit::Contain, 400, 400));
    let updated = variants.update(&name, update).await?;
    println!("now {:?}", updated.options);

    for (name, variant) in variants.list().await? {
        println!(
            "{name}: {}x{} ({:?})",
            variant.options.width, variant.options.height, variant.options.fit
        );
    }

    variants.delete(&name).await?;
    Ok(())
}
