mod common;

use cf_stream::{Error, LiveInputSettings, LiveInputStatus, RecordingMode, RecordingSettings};
use common::*;
use secrecy::ExposeSecret;
use serde_json::json;
use wiremock::matchers::{header, query_param};

const INPUT: &str = "66be4bf738797e01e1fca35a7bdecdcd";
const OUTPUT: &str = "baea4d9c515887b80289d5c33cf01145";

#[tokio::test]
async fn get_parses_fixture_and_redacts_secrets() {
    let (server, client) = setup().await;
    mock("GET", &format!("/live_inputs/{INPUT}"))
        .respond_with(success(fixture("live_input.json")))
        .expect(1)
        .mount(&server)
        .await;
    let input = client.live_inputs().get(&INPUT.into()).await.unwrap();
    assert_eq!(input.uid.as_ref().unwrap(), INPUT);
    assert_eq!(input.status, None);
    assert_eq!(input.delete_recording_after_days, Some(45));
    assert_eq!(
        input.recording.as_ref().unwrap().mode,
        Some(RecordingMode::Off)
    );
    let rtmps = input.rtmps.as_ref().unwrap();
    assert_eq!(
        rtmps.url.as_deref(),
        Some("rtmps://live.cloudflare.com:443/live/")
    );
    assert!(
        rtmps
            .stream_key
            .as_ref()
            .unwrap()
            .expose_secret()
            .starts_with("2fb3cb9f")
    );

    let debug = format!("{input:?}");
    for secret in [
        "2fb3cb9f17e68a2568d6ebed8d5505ea",
        "srt-passphrase-secret",
        "srt-stream-id-secret",
        "webrtc-key-secret",
    ] {
        assert!(!debug.contains(secret), "Debug leaks {secret}: {debug}");
    }
}

#[tokio::test]
async fn list_unwraps_live_inputs_with_counts() {
    let (server, client) = setup().await;
    mock("GET", "/live_inputs")
        .and(query_param("include_counts", "true"))
        .respond_with(success(json!({
            "liveInputs": [{"uid": INPUT, "enabled": true, "meta": {"name": "a"}}],
            "range": 0,
            "total": "1"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let list = client.live_inputs().list().await.unwrap();
    assert_eq!(list.live_inputs.len(), 1);
    assert_eq!(list.range, Some(0));
    assert_eq!(list.total, Some(1));
}

#[tokio::test]
async fn create_with_idempotency_key() {
    let (server, client) = setup().await;
    mock("POST", "/live_inputs")
        .and(header("idempotency-key", "studio-a"))
        .respond_with(success(json!({"uid": INPUT, "status": "connected"})))
        .expect(1)
        .mount(&server)
        .await;
    let input = client
        .live_inputs()
        .create(
            LiveInputSettings::new()
                .meta(&json!({"name": "Studio A"}))
                .default_creator("user-1")
                .prefer_low_latency(true)
                .delete_recording_after_days(30)
                .recording(
                    RecordingSettings::new(RecordingMode::Automatic)
                        .require_signed_urls(true)
                        .allowed_origins(["example.com"])
                        .hide_live_viewer_count(true),
                ),
        )
        .idempotency_key("studio-a")
        .send()
        .await
        .unwrap();
    assert_eq!(input.status, Some(LiveInputStatus::Connected));
    assert_eq!(
        body_json(&only_request(&server).await),
        json!({
            "meta": {"name": "Studio A"},
            "defaultCreator": "user-1",
            "preferLowLatency": true,
            "deleteRecordingAfterDays": 30,
            "recording": {
                "mode": "automatic",
                "allowedOrigins": ["example.com"],
                "requireSignedURLs": true,
                "hideLiveViewerCount": true
            },
        })
    );
}

#[tokio::test]
async fn invalid_settings_send_nothing() {
    let (server, client) = setup().await;
    let err = client
        .live_inputs()
        .create(LiveInputSettings::new())
        .idempotency_key("")
        .send()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)));
    let err = client
        .live_inputs()
        .update(
            &INPUT.into(),
            LiveInputSettings::new().delete_recording_after_days(1),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)));
    assert_no_requests(&server).await;
}

#[tokio::test]
async fn update_actions_and_delete() {
    let (server, client) = setup().await;
    mock("PUT", &format!("/live_inputs/{INPUT}"))
        .respond_with(success(json!({"uid": INPUT, "enabled": false})))
        .expect(1)
        .mount(&server)
        .await;
    // Enable and disable answer with `"ok"`, not the input.
    for action in ["enable", "disable"] {
        mock("POST", &format!("/live_inputs/{INPUT}/{action}"))
            .respond_with(success(json!("ok")))
            .expect(1)
            .mount(&server)
            .await;
    }
    mock("POST", &format!("/live_inputs/{INPUT}/rotate_keys"))
        .respond_with(success(json!({"uid": INPUT, "status": {"current": "odd"}})))
        .expect(1)
        .mount(&server)
        .await;
    mock("DELETE", &format!("/live_inputs/{INPUT}"))
        .respond_with(success(json!("")))
        .expect(1)
        .mount(&server)
        .await;

    let inputs = client.live_inputs();
    let id = INPUT.into();
    let updated = inputs
        .update(&id, LiveInputSettings::new().enabled(false))
        .await
        .unwrap();
    assert_eq!(updated.enabled, Some(false));
    inputs.enable(&id).await.unwrap();
    inputs.disable(&id).await.unwrap();
    let rotated = inputs.rotate_keys(&id).await.unwrap();
    assert_eq!(rotated.status, Some(LiveInputStatus::Unknown));
    inputs.delete(&id).await.unwrap();
}

#[tokio::test]
async fn outputs() {
    let (server, client) = setup().await;
    let output = json!({"uid": OUTPUT, "url": "rtmp://a.rtmp.youtube.com/live2", "streamKey": "yt-secret-key", "enabled": true});
    mock("GET", &format!("/live_inputs/{INPUT}/outputs"))
        .respond_with(success(json!([output.clone()])))
        .expect(1)
        .mount(&server)
        .await;
    mock("POST", &format!("/live_inputs/{INPUT}/outputs"))
        .respond_with(success(output.clone()))
        .expect(1)
        .mount(&server)
        .await;
    mock("PUT", &format!("/live_inputs/{INPUT}/outputs/{OUTPUT}"))
        .respond_with(success(output.clone()))
        .expect(1)
        .mount(&server)
        .await;
    mock("DELETE", &format!("/live_inputs/{INPUT}/outputs/{OUTPUT}"))
        .respond_with(raw(200, "{}"))
        .expect(1)
        .mount(&server)
        .await;

    let outputs = client.live_outputs();
    let (input, out) = (INPUT.into(), OUTPUT.into());
    let list = outputs.list(&input).await.unwrap();
    assert!(!format!("{list:?}").contains("yt-secret-key"));
    outputs
        .create(&input, "rtmp://a.rtmp.youtube.com/live2", "yt-secret-key")
        .await
        .unwrap();
    outputs.update(&input, &out, false).await.unwrap();
    outputs.delete(&input, &out).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        body_json(&requests[1]),
        json!({"url": "rtmp://a.rtmp.youtube.com/live2", "streamKey": "yt-secret-key"})
    );
    assert_eq!(body_json(&requests[2]), json!({"enabled": false}));
}

mod lifecycle {
    use super::*;
    use cf_stream::{Delivery, Lifecycle};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn delivery_server() -> (MockServer, Delivery) {
        let server = MockServer::start().await;
        let delivery = Delivery::with_base(server.uri().parse().unwrap()).unwrap();
        (server, delivery)
    }

    #[tokio::test]
    async fn fetches_status_without_the_api_token() {
        let (api, client) = setup().await;
        let (cdn, delivery) = delivery_server().await;
        // The live response carries more fields than documented.
        Mock::given(method("GET"))
            .and(path(format!("/{INPUT}/lifecycle")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "isInput": true,
                "videoUID": "ea95132c15732412d22c1476fa83f27a",
                "live": true,
                "status": "connected",
                "chunked": true,
                "unstable": {"ltxEnabled": true}
            })))
            .expect(1)
            .mount(&cdn)
            .await;

        let status = client
            .live_inputs()
            .lifecycle(&delivery, INPUT)
            .await
            .unwrap();
        assert_eq!(status.is_input, Some(true));
        assert_eq!(status.live, Some(true));
        assert_eq!(
            status.video_uid.unwrap(),
            "ea95132c15732412d22c1476fa83f27a"
        );
        assert_eq!(status.status.as_deref(), Some("connected"));

        let request = only_request(&cdn).await;
        assert!(
            request.headers.get("authorization").is_none(),
            "the API token must not be sent to the delivery domain"
        );
        assert_no_requests(&api).await;
    }

    #[tokio::test]
    async fn idle_input_has_no_video() {
        let (_api, client) = setup().await;
        let (cdn, delivery) = delivery_server().await;
        Mock::given(method("GET"))
            .and(path(format!("/{INPUT}/lifecycle")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "isInput": true, "videoUID": null, "live": false
            })))
            .mount(&cdn)
            .await;
        let status: Lifecycle = client
            .live_inputs()
            .lifecycle(&delivery, INPUT)
            .await
            .unwrap();
        assert_eq!(status.video_uid, None);
        assert_eq!(status.live, Some(false));
        assert_eq!(status.status, None);
    }

    #[tokio::test]
    async fn unknown_id_is_not_found() {
        // The delivery domain answers unknown IDs with a plain-text 404.
        let (_api, client) = setup().await;
        let (cdn, delivery) = delivery_server().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404).set_body_string("404 not found"))
            .mount(&cdn)
            .await;
        let err = client
            .live_inputs()
            .lifecycle(&delivery, "00000000000000000000000000000000")
            .await
            .unwrap_err();
        assert!(err.is_not_found(), "{err:?}");
    }

    #[tokio::test]
    async fn bad_body_and_empty_id() {
        let (_api, client) = setup().await;
        let (cdn, delivery) = delivery_server().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>"))
            .mount(&cdn)
            .await;
        let err = client
            .live_inputs()
            .lifecycle(&delivery, INPUT)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Decode { .. }), "{err:?}");

        let err = client
            .live_inputs()
            .lifecycle(&delivery, "")
            .await
            .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{err:?}");
        assert_eq!(cdn.received_requests().await.unwrap().len(), 1);
    }
}
