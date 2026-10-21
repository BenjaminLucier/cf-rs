//! Build delivery URLs for named and flexible variants. No network access
//! needed.
//!
//! ```sh
//! cargo run --example images_delivery_urls
//! ```

use cf_images::{AccountHash, Delivery, Fit, FlexibleVariant};

fn main() -> anyhow::Result<()> {
    let hash = AccountHash::new("ZWd9g1K7eljCn_KDTu_MWA");
    let image = "083eb7b2-5392-4565-b69e-aff66acddd00".into();

    // https://imagedelivery.net/{hash}/{image}/{variant}
    let delivery = Delivery::new(hash.clone());
    println!("{}", delivery.url(&image, &"public".into()));

    // Resize on the fly (flexible variants must be enabled for the account).
    let options = FlexibleVariant::new()
        .width(400)
        .height(300)
        .fit(Fit::Cover)
        .quality(80);
    println!("{}", delivery.flexible_url(&image, &options));

    // Through your own zone: https://{zone}/cdn-cgi/imagedelivery/{hash}/...
    let custom = Delivery::custom_domain(hash, "images.example.com")?;
    println!("{}", custom.url(&image, &"public".into()));
    Ok(())
}
