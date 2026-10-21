//! Compile-time checks that the client is thread-safe and every returned
//! future and stream is `Send`, so they can be spawned on a multi-threaded
//! runtime.

use cf_stream::*;

fn assert_send<T: Send>(_: &T) {}
fn assert_send_sync<T: Send + Sync + 'static>() {}
fn assert_clone<T: Clone>() {}

#[test]
fn client_is_send_sync_clone() {
    assert_send_sync::<Client>();
    assert_clone::<Client>();
    assert_send_sync::<Error>();
    assert_send_sync::<Video>();
    assert_send_sync::<LiveInput>();
    assert_send_sync::<Delivery>();
    #[cfg(feature = "signing")]
    assert_send_sync::<TokenSigner>();
    #[cfg(feature = "webhooks")]
    {
        assert_send_sync::<WebhookVerifier>();
        assert_send_sync::<WebhookError>();
    }
}

#[allow(dead_code, unused_must_use)]
#[allow(clippy::too_many_arguments)]
fn futures_are_send(
    client: &Client,
    video: &VideoId,
    input: &LiveInputId,
    output: &OutputId,
    watermark: &WatermarkId,
    track: &AudioTrackId,
    key: &SigningKeyId,
    url: reqwest::Url,
) {
    let videos = client.videos();
    assert_send(&videos.get(video));
    assert_send(&videos.update(video, UpdateVideo::new()));
    assert_send(&videos.delete(video));
    let list = videos.list();
    assert_send(&list.send());
    assert_send(&videos.list_all());
    assert_send(&videos.list().into_stream());
    assert_send(&videos.upload(UploadSource::bytes(vec![], "a")).send());
    assert_send(&videos.copy(url.clone()).send());
    assert_send(&videos.clip(video, 0, 1).send());
    let usage = videos.storage_usage();
    assert_send(&usage.send());
    assert_send(&videos.create_token(video).send());

    let uploads = client.direct_uploads();
    assert_send(&uploads.create(60).send());
    assert_send(&uploads.create_tus(1).send());

    let captions = client.captions();
    assert_send(&captions.list(video));
    assert_send(&captions.get(video, "en"));
    assert_send(&captions.upload(video, "en", vec![]));
    assert_send(&captions.generate(video, "en"));
    assert_send(&captions.delete(video, "en"));
    assert_send(&captions.vtt(video, "en"));

    let audio = client.audio_tracks();
    assert_send(&audio.list(video));
    assert_send(&audio.copy(video, "l", url.clone()));
    assert_send(&audio.update(video, track, UpdateAudioTrack::new()));
    assert_send(&audio.delete(video, track));

    let downloads = client.downloads();
    assert_send(&downloads.get(video));
    assert_send(&downloads.create(video, DownloadType::Default));
    assert_send(&downloads.delete(video, DownloadType::Audio));

    let keys = client.signing_keys();
    assert_send(&keys.create());
    assert_send(&keys.list());
    assert_send(&keys.delete(key));

    let inputs = client.live_inputs();
    assert_send(&inputs.list());
    assert_send(&inputs.get(input));
    assert_send(&inputs.create(LiveInputSettings::new()).send());
    assert_send(&inputs.update(input, LiveInputSettings::new()));
    assert_send(&inputs.delete(input));
    assert_send(&inputs.enable(input));
    assert_send(&inputs.disable(input));
    assert_send(&inputs.rotate_keys(input));
    let delivery = Delivery::new(CustomerCode::new("code")).unwrap();
    assert_send(&inputs.lifecycle(&delivery, input));

    let outputs = client.live_outputs();
    assert_send(&outputs.list(input));
    assert_send(&outputs.create(input, "rtmp://x", "key"));
    assert_send(&outputs.update(input, output, true));
    assert_send(&outputs.delete(input, output));

    let watermarks = client.watermarks();
    assert_send(&watermarks.create(WatermarkSource::url(url)).send());
    assert_send(&watermarks.list());
    assert_send(&watermarks.get(watermark));
    assert_send(&watermarks.delete(watermark));

    let webhooks = client.webhooks();
    assert_send(&webhooks.get());
    assert_send(&webhooks.set("https://example.com".parse().unwrap()));
    assert_send(&webhooks.delete());
}

#[test]
fn compiles() {
    // `futures_are_send` only needs to type-check.
    let _ = futures_are_send;
}
