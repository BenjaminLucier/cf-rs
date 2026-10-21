//! Build playback, thumbnail and download URLs (no API calls).
//!
//! ```sh
//! cargo run --example stream_delivery_urls
//! ```

use std::time::Duration;

use cf_stream::{CustomerCode, Delivery, DownloadType, ThumbnailFit, ThumbnailOptions};

fn main() -> cf_stream::Result<()> {
    let delivery = Delivery::new(CustomerCode::new("m033z5x00ks6nunl"))?;
    let video = "ea95132c15732412d22c1476fa83f27a";

    println!("HLS:       {}", delivery.hls(video));
    println!("DASH:      {}", delivery.dash(video));
    println!("iframe:    {}", delivery.iframe(video));
    println!("preview:   {}", delivery.watch(video));
    println!(
        "thumbnail: {}",
        delivery.thumbnail(
            video,
            &ThumbnailOptions::new()
                .time(Duration::from_secs(12))
                .height(270)
                .fit(ThumbnailFit::Crop),
        )
    );
    println!(
        "GIF:       {}",
        delivery.animated_thumbnail(
            video,
            &ThumbnailOptions::new()
                .time(Duration::from_secs(30))
                .duration(Duration::from_secs(4))
                .fps(10),
        )
    );
    println!(
        "MP4:       {}",
        delivery.download(video, DownloadType::Default, Some("keynote"))?
    );
    Ok(())
}
