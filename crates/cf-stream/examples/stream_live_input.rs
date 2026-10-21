//! Create a recorded live input, print its RTMPS and SRT details, and add a
//! restreaming output.
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... cargo run --example stream_live_input
//! ```

use anyhow::Context;
use cf_stream::secrecy::ExposeSecret;
use cf_stream::{Client, LiveInputSettings, RecordingMode, RecordingSettings};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new(
        std::env::var("CF_ACCOUNT_ID").context("CF_ACCOUNT_ID")?,
        std::env::var("CF_API_TOKEN").context("CF_API_TOKEN")?,
    )?;

    let input = client
        .live_inputs()
        .create(
            LiveInputSettings::new()
                .meta(&serde_json::json!({ "name": "cf-stream example" }))
                .recording(RecordingSettings::new(RecordingMode::Automatic))
                .delete_recording_after_days(30),
        )
        .send()
        .await?;
    let id = input.uid.clone().context("live input ID")?;
    println!("live input {id}");

    if let Some(rtmps) = &input.rtmps {
        println!("RTMPS server: {:?}", rtmps.url);
        // Stream keys are secrets; only print them where it's safe.
        if let Some(key) = &rtmps.stream_key {
            println!("RTMPS stream key: {}", key.expose_secret());
        }
    }
    if let Some(srt) = &input.srt {
        println!("SRT server: {:?}", srt.url);
    }

    let output = client
        .live_outputs()
        .create(
            &id,
            "rtmp://a.rtmp.youtube.com/live2",
            "your-youtube-stream-key",
        )
        .await?;
    println!("restreaming via output {:?}", output.uid);
    Ok(())
}
