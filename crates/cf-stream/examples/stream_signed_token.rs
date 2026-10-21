//! Sign a playback token offline and build signed URLs with it.
//!
//! Create a signing key once (for example with
//! `client.signing_keys().create()`) and store its ID and PEM.
//!
//! ```sh
//! STREAM_KEY_ID=... STREAM_KEY_PEM=... CUSTOMER_CODE=... \
//!     cargo run --example stream_signed_token -- <video id>
//! ```

use std::time::Duration;

use anyhow::Context;
use cf_stream::{AccessAction, AccessRule, CustomerCode, Delivery, TokenSigner};

fn main() -> anyhow::Result<()> {
    let signer = TokenSigner::new(
        std::env::var("STREAM_KEY_ID").context("STREAM_KEY_ID")?,
        std::env::var("STREAM_KEY_PEM").context("STREAM_KEY_PEM")?,
    )?;
    let video = std::env::args()
        .nth(1)
        .context("usage: stream_signed_token <video id>")?;

    let token = signer
        .token(video)
        .expires_in(Duration::from_secs(3600))
        .downloadable(true)
        .access_rule(AccessRule::country(AccessAction::Block, ["XX"]))
        .sign()?;

    let delivery = Delivery::new(CustomerCode::new(
        std::env::var("CUSTOMER_CODE").context("CUSTOMER_CODE")?,
    ))?;
    println!("HLS:    {}", delivery.hls(&token));
    println!("iframe: {}", delivery.iframe(&token));
    println!(
        "MP4:    {}",
        delivery.download(&token, cf_stream::DownloadType::Default, None)?
    );
    Ok(())
}
