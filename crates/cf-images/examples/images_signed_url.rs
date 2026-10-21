//! Build a signed delivery URL for a private image. No network access needed.
//!
//! ```sh
//! CF_ACCOUNT_HASH=... CF_SIGNING_KEY=... cargo run --example images_signed_url -- <image-id> [variant]
//! ```

use std::time::Duration;

use anyhow::Context;
use cf_images::{AccountHash, Delivery, FlexibleVariant, UrlSigner};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let image_id = args
        .next()
        .context("usage: images_signed_url <image-id> [variant]")?;
    let variant = args.next().unwrap_or_else(|| "public".to_owned());

    let delivery = Delivery::new(AccountHash::new(
        std::env::var("CF_ACCOUNT_HASH").context("CF_ACCOUNT_HASH")?,
    ));
    let signer = UrlSigner::new(std::env::var("CF_SIGNING_KEY").context("CF_SIGNING_KEY")?);

    let url = delivery.url(&image_id.as_str().into(), &variant.into());
    let signed = signer.sign_for(&url, Duration::from_secs(60 * 60))?;
    println!("{signed}");

    // Flexible variants can be signed the same way.
    let flexible = delivery.flexible_url(
        &image_id.into(),
        &FlexibleVariant::new().width(400).height(300).format("auto"),
    );
    println!(
        "{}",
        signer.sign_for(&flexible, Duration::from_secs(60 * 60))?
    );
    Ok(())
}
