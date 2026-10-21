//! Tests against the real Cloudflare API. All are `#[ignore]`d; run with:
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... \
//!     cargo test -p cf-stream --all-features --test live -- --ignored --test-threads=1
//! ```
//!
//! The token needs **Account > Stream > Edit**. Set
//! `CF_STREAM_TEST_VIDEO_URL` to a small, publicly reachable MP4 to also run
//! the video lifecycle test (it waits for encoding, up to 10 minutes). Every
//! test deletes what it creates, even when an assertion fails midway. The
//! account's webhook is never touched, because there is only one per
//! account.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use cf_stream::{
    Client, CustomerCode, Delivery, DownloadType, LiveInputId, LiveInputSettings, RecordingMode,
    RecordingSettings, SigningKeyId, UpdateVideo, VideoId, VideoState,
};
use futures_util::{StreamExt, TryStreamExt};
use serde_json::json;

fn env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} must be set for live tests"))
}

fn client() -> Result<Client> {
    Ok(Client::new(env("CF_ACCOUNT_ID")?, env("CF_API_TOKEN")?)?)
}

fn unique(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}{nanos}")
}

/// Something to delete when a test ends.
enum Resource {
    Video(VideoId),
    LiveInput(LiveInputId),
    SigningKey(SigningKeyId),
}

/// Deletes the resource when dropped unless disarmed with `take`.
///
/// Drop can't be async, so the delete runs on a fresh runtime thread with its
/// own `Client`: reusing the test's client would hand that thread connections
/// owned by the (blocked) test runtime and deadlock.
struct Cleanup(Option<Resource>);

impl Cleanup {
    fn take(&mut self) {
        self.0 = None;
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(resource) = self.0.take() {
            let _ = std::thread::spawn(move || {
                let client = client()?;
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?
                    .block_on(async {
                        match resource {
                            Resource::Video(id) => client.videos().delete(&id).await,
                            Resource::LiveInput(id) => client.live_inputs().delete(&id).await,
                            Resource::SigningKey(id) => client.signing_keys().delete(&id).await,
                        }
                    })?;
                anyhow::Ok(())
            })
            .join();
        }
    }
}

#[tokio::test]
#[ignore = "requires Cloudflare credentials"]
async fn list_and_storage_usage() -> Result<()> {
    let client = client()?;
    let page = client
        .videos()
        .list()
        .limit(5)
        .include_counts(true)
        .send()
        .await?;
    assert!(page.videos.len() <= 5);
    assert!(page.total.is_some(), "include_counts reports a total");
    let first: Vec<_> = client.videos().list_all().take(3).try_collect().await?;
    assert!(first.len() <= 3);
    let usage = client.videos().storage_usage().send().await?;
    assert!(usage.video_count.is_some(), "{usage:?}");
    client.watermarks().list().await?;
    client.live_inputs().list().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires Cloudflare credentials"]
async fn list_date_bounds() -> Result<()> {
    // Three pending direct uploads, created a little apart.
    let client = client()?;
    let name = unique("cf-stream-live-bounds-");
    let mut ids = Vec::new();
    let mut cleanups = Vec::new();
    for _ in 0..3 {
        let upload = client
            .direct_uploads()
            .create(60)
            .meta(&json!({ "name": name }))
            .send()
            .await?;
        let id = upload.uid.context("uid")?;
        cleanups.push(Cleanup(Some(Resource::Video(id.clone()))));
        ids.push(id);
        tokio::time::sleep(Duration::from_millis(1100)).await;
    }

    let listed = |asc: bool| {
        client
            .videos()
            .list()
            .video_name(&name)
            .asc(asc)
            .limit(1)
            .into_stream()
            .map_ok(|v| v.uid.expect("uid"))
            .try_collect::<Vec<_>>()
    };
    // One video per page, so every boundary is crossed in both directions.
    assert_eq!(listed(true).await?, ids, "oldest first");
    let mut newest_first = ids.clone();
    newest_first.reverse();
    assert_eq!(listed(false).await?, newest_first, "newest first");

    // `before` is exclusive and `after` inclusive of the exact time.
    let middle = client.videos().get(&ids[1]).await?;
    let created = middle.created.context("created")?;
    let raw = |param: &'static str| {
        let created = created.clone();
        let name = name.clone();
        async move {
            // Raw request: the crate's `before`/`after` take a `SystemTime`
            // with whole seconds, but this needs the exact microseconds.
            let url = cf_stream::reqwest::Url::parse_with_params(
                &format!(
                    "https://api.cloudflare.com/client/v4/accounts/{}/stream",
                    env("CF_ACCOUNT_ID")?
                ),
                &[("video_name", name.as_str()), (param, created.as_str())],
            )?;
            let body: serde_json::Value = cf_stream::reqwest::Client::new()
                .get(url)
                .bearer_auth(env("CF_API_TOKEN")?)
                .send()
                .await?
                .json()
                .await?;
            let uids: Vec<String> = body["result"]
                .as_array()
                .context("result array")?
                .iter()
                .filter_map(|v| v["uid"].as_str().map(str::to_owned))
                .collect();
            anyhow::Ok(uids)
        }
    };
    let before = raw("before").await?;
    assert_eq!(before, [ids[0].to_string()], "before is exclusive");
    let mut after = raw("after").await?;
    after.sort();
    let mut expected = vec![ids[1].to_string(), ids[2].to_string()];
    expected.sort();
    assert_eq!(after, expected, "after is inclusive");
    drop(cleanups);
    Ok(())
}

#[tokio::test]
#[ignore = "requires Cloudflare credentials"]
async fn signing_key_lifecycle() -> Result<()> {
    let client = client()?;
    let key = client.signing_keys().create().await?;
    let id = key.id.clone().context("key id")?;
    let mut cleanup = Cleanup(Some(Resource::SigningKey(id.clone())));
    assert!(key.pem().is_some() && key.jwk().is_some());

    #[cfg(feature = "signing")]
    {
        // The PEM Cloudflare returns must parse and sign.
        let signer = cf_stream::TokenSigner::from_signing_key(&key)?;
        signer.token("0123456789abcdef0123456789abcdef").sign()?;
    }

    let keys = client.signing_keys().list().await?;
    assert!(keys.iter().any(|k| k.id.as_ref() == Some(&id)));
    client.signing_keys().delete(&id).await?;
    cleanup.take();
    Ok(())
}

#[tokio::test]
#[ignore = "requires Cloudflare credentials"]
async fn live_input_lifecycle() -> Result<()> {
    let client = client()?;
    let name = unique("cf-stream-live-");
    let input = client
        .live_inputs()
        .create(
            LiveInputSettings::new()
                .meta(&json!({ "name": name }))
                .recording(RecordingSettings::new(RecordingMode::Automatic).timeout_seconds(10)),
        )
        .idempotency_key(&name)
        .send()
        .await?;
    let id = input.uid.clone().context("live input id")?;
    let mut cleanup = Cleanup(Some(Resource::LiveInput(id.clone())));
    assert!(
        input
            .rtmps
            .as_ref()
            .and_then(|r| r.stream_key.as_ref())
            .is_some()
    );

    let inputs = client.live_inputs();
    let fetched = inputs.get(&id).await?;
    assert_eq!(fetched.meta, Some(json!({ "name": name })));
    let updated = inputs
        .update(
            &id,
            LiveInputSettings::new().delete_recording_after_days(30),
        )
        .await?;
    assert_eq!(updated.delete_recording_after_days, Some(30));
    inputs.disable(&id).await?;
    assert_eq!(inputs.get(&id).await?.enabled, Some(false));
    inputs.enable(&id).await?;
    assert_eq!(inputs.get(&id).await?.enabled, Some(true));
    let rotated = inputs.rotate_keys(&id).await?;
    assert!(rotated.rtmps.and_then(|r| r.stream_key).is_some());

    // Lifecycle, on the delivery domain named in the input's playback URLs.
    let playback = fetched
        .web_rtc_playback
        .as_ref()
        .and_then(|w| w.url.as_ref())
        .map(|u| secrecy::ExposeSecret::expose_secret(u).to_owned())
        .context("WebRTC playback URL")?;
    let delivery = Delivery::new(customer_code(&playback)?)?;
    let lifecycle = inputs.lifecycle(&delivery, &id).await?;
    assert_eq!(lifecycle.is_input, Some(true));
    assert_eq!(lifecycle.live, Some(false));
    assert_eq!(lifecycle.video_uid, None);
    let missing = inputs
        .lifecycle(&delivery, "00000000000000000000000000000000")
        .await
        .unwrap_err();
    assert!(missing.is_not_found(), "{missing:?}");

    let outputs = client.live_outputs();
    let output = outputs
        // Cloudflare checks that the destination responds.
        .create(&id, "rtmp://a.rtmp.youtube.com/live2", "cf-stream-live-key")
        .await?;
    let output_id = output.uid.context("output id")?;
    assert_eq!(outputs.list(&id).await?.len(), 1);
    let disabled = outputs.update(&id, &output_id, false).await?;
    assert_eq!(disabled.enabled, Some(false));
    outputs.delete(&id, &output_id).await?;

    inputs.delete(&id).await?;
    cleanup.take();
    let err = inputs.get(&id).await.unwrap_err();
    assert!(err.is_not_found(), "{err:?}");
    Ok(())
}

#[tokio::test]
#[ignore = "requires Cloudflare credentials"]
async fn direct_and_tus_upload_creation() -> Result<()> {
    let client = client()?;
    let upload = client
        .direct_uploads()
        .create(60)
        .expires_in(Duration::from_secs(10 * 60))
        .meta(&json!({ "name": "cf-stream-live-direct" }))
        .send()
        .await?;
    let id = upload.uid.context("uid")?;
    let _cleanup = Cleanup(Some(Resource::Video(id.clone())));
    assert!(upload.upload_url.is_some());
    let pending = client.videos().get(&id).await?;
    assert_eq!(
        pending.status.and_then(|s| s.state),
        Some(VideoState::PendingUpload)
    );

    let tus = client
        .direct_uploads()
        .create_tus(10 * 1024 * 1024)
        .direct_user(true)
        .name("cf-stream-live-tus")
        .max_duration_seconds(60)
        .send()
        .await?;
    let tus_id = tus.video_id.context("stream-media-id")?;
    let _tus_cleanup = Cleanup(Some(Resource::Video(tus_id)));
    assert_eq!(tus.location.scheme(), "https");
    Ok(())
}

#[tokio::test]
#[ignore = "requires Cloudflare credentials and CF_STREAM_TEST_VIDEO_URL"]
async fn video_lifecycle() -> Result<()> {
    let Ok(source) = std::env::var("CF_STREAM_TEST_VIDEO_URL") else {
        eprintln!("CF_STREAM_TEST_VIDEO_URL not set; skipping");
        return Ok(());
    };
    let client = client()?;
    let video = client
        .videos()
        .copy(source.parse()?)
        .meta(&json!({ "name": unique("cf-stream-live-") }))
        .creator("cf-stream-live")
        .require_signed_urls(true)
        .send()
        .await?;
    let id = video.uid.clone().context("uid")?;
    let mut cleanup = Cleanup(Some(Resource::Video(id.clone())));

    // Wait for encoding.
    let deadline = SystemTime::now() + Duration::from_secs(600);
    let video = loop {
        let video = client.videos().get(&id).await?;
        match video.status.as_ref().and_then(|s| s.state) {
            Some(VideoState::Ready) if video.ready_to_stream == Some(true) => break video,
            Some(VideoState::Error) => bail!("encoding failed: {:?}", video.status),
            _ if SystemTime::now() > deadline => bail!("timed out: {:?}", video.status),
            _ => tokio::time::sleep(Duration::from_secs(5)).await,
        }
    };
    assert_eq!(video.creator.as_deref(), Some("cf-stream-live"));

    // Appears in a filtered list.
    let listed: Vec<_> = client
        .videos()
        .list()
        .creator("cf-stream-live")
        .into_stream()
        .try_collect()
        .await?;
    assert!(listed.iter().any(|v| v.uid.as_ref() == Some(&id)));

    let updated = client
        .videos()
        .update(&id, UpdateVideo::new().thumbnail_timestamp_pct(0.5))
        .await?;
    assert_eq!(updated.thumbnail_timestamp_pct, Some(0.5));

    client.captions().list(&id).await?;
    client
        .downloads()
        .create(&id, DownloadType::Default)
        .await?;
    client.downloads().get(&id).await?;
    client
        .downloads()
        .delete(&id, DownloadType::Default)
        .await?;

    // Signed playback: the manifest is refused without a token and served
    // with one.
    let hls = video.playback.and_then(|p| p.hls).context("playback URL")?;
    let delivery = Delivery::new(customer_code(&hls)?)?;

    // The lifecycle of a video that requires signed URLs needs a token too.
    let err = client
        .live_inputs()
        .lifecycle(&delivery, &id)
        .await
        .unwrap_err();
    assert!(err.is_unauthorized(), "{err:?}");

    let http = cf_stream::reqwest::Client::new();
    let unsigned = http.get(delivery.hls(&id)).send().await?;
    assert!(!unsigned.status().is_success(), "served without a token");

    let token = client
        .videos()
        .create_token(&id)
        .expires_in(Duration::from_secs(300))
        .send()
        .await?;
    let token = secrecy::ExposeSecret::expose_secret(&token);
    let response = http.get(delivery.hls(token)).send().await?;
    assert!(response.status().is_success(), "{}", response.status());
    let lifecycle = client.live_inputs().lifecycle(&delivery, token).await?;
    assert_eq!(lifecycle.is_input, Some(false));
    assert_eq!(lifecycle.video_uid.as_ref(), Some(&id));

    #[cfg(feature = "signing")]
    {
        let key = client.signing_keys().create().await?;
        let key_id = key.id.clone().context("key id")?;
        let _key_cleanup = Cleanup(Some(Resource::SigningKey(key_id)));
        let signer = cf_stream::TokenSigner::from_signing_key(&key)?;
        let token = signer
            .token(id.as_str())
            .expires_in(Duration::from_secs(300))
            .sign()?;
        let response = http.get(delivery.hls(&token)).send().await?;
        assert!(response.status().is_success(), "{}", response.status());
    }

    client.videos().delete(&id).await?;
    cleanup.take();
    Ok(())
}

/// The customer code in a delivery URL (`https://customer-{code}.cloudflarestream.com/...`).
fn customer_code(url: &str) -> Result<CustomerCode> {
    let url: cf_stream::reqwest::Url = url.parse()?;
    let code = url
        .host_str()
        .and_then(|h| h.strip_prefix("customer-"))
        .and_then(|h| h.split('.').next())
        .context("customer code in delivery URL")?;
    Ok(CustomerCode::new(code))
}
