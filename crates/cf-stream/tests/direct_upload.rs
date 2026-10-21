mod common;

use std::time::{Duration, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use cf_stream::Error;
use common::*;
use serde_json::json;
use wiremock::ResponseTemplate;
use wiremock::matchers::{header, query_param, query_param_is_missing};

#[tokio::test]
async fn direct_upload_body() {
    let (server, client) = setup().await;
    mock("POST", "/direct_upload")
        .respond_with(success(json!({
            "uid": "ea95132c15732412d22c1476fa83f27a",
            "uploadURL": "https://upload.videodelivery.net/ea95132c15732412d22c1476fa83f27a",
            "scheduledDeletion": null,
            "watermark": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    let upload = client
        .direct_uploads()
        .create(3600)
        .expiry(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        .creator("user-1")
        .require_signed_urls(true)
        .send()
        .await
        .unwrap();
    assert_eq!(
        upload.upload_url.as_deref(),
        Some("https://upload.videodelivery.net/ea95132c15732412d22c1476fa83f27a")
    );
    assert_eq!(
        body_json(&only_request(&server).await),
        json!({
            "maxDurationSeconds": 3600,
            "expiry": "2023-11-14T22:13:20Z",
            "creator": "user-1",
            "requireSignedURLs": true,
        })
    );
}

#[tokio::test]
async fn direct_upload_validates_duration() {
    let (server, client) = setup().await;
    for secs in [0, 36_001] {
        let err = client
            .direct_uploads()
            .create(secs)
            .send()
            .await
            .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
    }
    let err = client
        .direct_uploads()
        .create(60)
        .watermark("")
        .send()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)));
    assert_no_requests(&server).await;
}

#[tokio::test]
async fn tus_creation_headers_and_response() {
    let (server, client) = setup().await;
    mock("POST", "")
        .and(header("tus-resumable", "1.0.0"))
        .and(header("upload-length", "1500000000"))
        .and(header("upload-creator", "user-1"))
        .and(query_param("direct_user", "true"))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header(
                    "location",
                    "https://upload.cloudflarestream.com/tus/ea95132c15732412d22c1476fa83f27a?tusv2=true",
                )
                .insert_header("stream-media-id", "ea95132c15732412d22c1476fa83f27a"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let upload = client
        .direct_uploads()
        .create_tus(1_500_000_000)
        .direct_user(true)
        .creator("user-1")
        .name("talk.mp4")
        .require_signed_urls(true)
        .send()
        .await
        .unwrap();
    assert_eq!(upload.video_id.unwrap(), "ea95132c15732412d22c1476fa83f27a");
    assert_eq!(
        upload.location.host_str(),
        Some("upload.cloudflarestream.com")
    );

    let req = only_request(&server).await;
    let metadata = req
        .headers
        .get("upload-metadata")
        .unwrap()
        .to_str()
        .unwrap();
    let encoded_name = BASE64.encode("talk.mp4");
    assert_eq!(metadata, format!("name {encoded_name},requiresignedurls"));
    assert!(req.body.is_empty());
}

#[tokio::test]
async fn tus_without_options_sends_no_metadata() {
    let (server, client) = setup().await;
    mock("POST", "")
        .and(query_param_is_missing("direct_user"))
        .respond_with(
            ResponseTemplate::new(201).insert_header("location", "https://u.example/tus/1"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let upload = client.direct_uploads().create_tus(10).send().await.unwrap();
    assert_eq!(upload.video_id, None);
    let req = only_request(&server).await;
    assert!(req.headers.get("upload-metadata").is_none());
    assert!(req.headers.get("upload-creator").is_none());
}

#[tokio::test]
async fn tus_missing_location_is_a_decode_error() {
    let (server, client) = setup().await;
    mock("POST", "")
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    let err = client
        .direct_uploads()
        .create_tus(10)
        .send()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Decode { .. }), "{err:?}");
}

#[tokio::test]
async fn tus_errors_and_validation() {
    let (server, client) = setup().await;
    mock("POST", "")
        .respond_with(failure(413, 10011, "Upload too large"))
        .mount(&server)
        .await;
    let err = client
        .direct_uploads()
        .create_tus(10)
        .send()
        .await
        .unwrap_err();
    assert_eq!(err.status(), Some(413));

    let (server, client) = setup().await;
    let err = client
        .direct_uploads()
        .create_tus(0)
        .send()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)));
    assert_no_requests(&server).await;
}
