mod common;

use std::time::{Duration, UNIX_EPOCH};

use cf_stream::{
    AccessAction, AccessRule, Error, UpdateVideo, UploadSource, Video, VideoKind, VideoState,
};
use common::*;
use futures_util::TryStreamExt;
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use wiremock::matchers::{query_param, query_param_is_missing};

const UID: &str = "ea95132c15732412d22c1476fa83f27a";

#[tokio::test]
async fn get_video_parses_fixture() {
    let (server, client) = setup().await;
    mock("GET", &format!("/{UID}"))
        .respond_with(success(fixture("video.json")))
        .expect(1)
        .mount(&server)
        .await;
    let video = client.videos().get(&UID.into()).await.unwrap();
    assert_eq!(video.uid.as_ref().unwrap(), UID);
    assert_eq!(video.require_signed_urls, Some(true));
    assert_eq!(video.duration(), Some(Duration::from_millis(5500)));
    assert_eq!(video.size, Some(4_190_963));
    assert_eq!(video.max_duration_seconds, None);
    assert_eq!(video.watermark, None);
    assert_eq!(video.input.as_ref().unwrap().width, Some(1920));
    let status = video.status.as_ref().unwrap();
    assert_eq!(status.state, Some(VideoState::Ready));
    assert_eq!(status.percent_complete(), Some(39.0));
    assert_eq!(status.error_reason(), None);
    assert_eq!(video.meta, Some(json!({"name": "video12345.mp4"})));
}

#[tokio::test]
async fn lenient_video_fields() {
    let (server, client) = setup().await;
    mock("GET", &format!("/{UID}"))
        .respond_with(success(json!({
            "uid": UID,
            "duration": -1,
            "input": {"width": -1, "height": -1},
            "size": 12.0,
            "status": {"state": "not-started", "pctComplete": "", "errorReasonCode": "ERR_NON_VIDEO"},
            "maxDurationSeconds": -1
        })))
        .mount(&server)
        .await;
    let video = client.videos().get(&UID.into()).await.unwrap();
    assert_eq!(video.duration(), None);
    assert_eq!(video.input.unwrap().height, Some(-1));
    assert_eq!(video.size, Some(12));
    assert_eq!(video.max_duration_seconds, Some(-1));
    let status = video.status.unwrap();
    assert_eq!(status.state, Some(VideoState::Unknown));
    assert_eq!(status.percent_complete(), None);
    assert_eq!(status.error_reason(), Some("ERR_NON_VIDEO"));
}

#[tokio::test]
async fn update_sends_only_set_fields() {
    let (server, client) = setup().await;
    mock("POST", &format!("/{UID}"))
        .respond_with(success(json!({"uid": UID})))
        .expect(1)
        .mount(&server)
        .await;
    client
        .videos()
        .update(
            &UID.into(),
            UpdateVideo::new()
                .meta(&json!({"name": "renamed"}))
                .require_signed_urls(false)
                .allowed_origins(["example.com"])
                .thumbnail_timestamp_pct(0.5)
                .max_duration_seconds(60)
                .upload_expiry(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
                .clear_scheduled_deletion(),
        )
        .await
        .unwrap();
    let body = body_json(&only_request(&server).await);
    assert_eq!(
        body,
        json!({
            "meta": {"name": "renamed"},
            "requireSignedURLs": false,
            "allowedOrigins": ["example.com"],
            "thumbnailTimestampPct": 0.5,
            "maxDurationSeconds": 60,
            "uploadExpiry": "2023-11-14T22:13:20Z",
            "scheduledDeletion": null,
        })
    );
}

#[tokio::test]
async fn invalid_update_sends_nothing() {
    let (server, client) = setup().await;
    for update in [
        UpdateVideo::new().thumbnail_timestamp_pct(1.5),
        UpdateVideo::new().max_duration_seconds(0),
    ] {
        let err = client
            .videos()
            .update(&UID.into(), update)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{err:?}");
    }
    assert!(matches!(
        client.videos().get(&"".into()).await.unwrap_err(),
        Error::InvalidInput(_)
    ));
    assert_no_requests(&server).await;
}

#[tokio::test]
async fn delete_accepts_every_success_shape() {
    for response in [
        raw(200, ""),
        raw(200, "{}"),
        raw(204, ""),
        success(json!("ok")),
        success(json!("")),
        success(Value::Null),
    ] {
        let (server, client) = setup().await;
        mock("DELETE", &format!("/{UID}"))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
        client.videos().delete(&UID.into()).await.unwrap();
    }
}

#[tokio::test]
async fn delete_reports_failures() {
    let (server, client) = setup().await;
    mock("DELETE", &format!("/{UID}"))
        .respond_with(failure(200, 10003, "nope"))
        .mount(&server)
        .await;
    let err = client.videos().delete(&UID.into()).await.unwrap_err();
    assert!(matches!(err, Error::Api { status: 200, .. }), "{err:?}");

    let (server, client) = setup().await;
    mock("DELETE", &format!("/{UID}"))
        .respond_with(failure(404, 10005, "Not found"))
        .mount(&server)
        .await;
    assert!(
        client
            .videos()
            .delete(&UID.into())
            .await
            .unwrap_err()
            .is_not_found()
    );
}

#[tokio::test]
async fn list_sends_filters_and_reads_counts() {
    let (server, client) = setup().await;
    mock("GET", "")
        .and(query_param("after", "2023-11-14T22:13:20Z"))
        .and(query_param("asc", "true"))
        .and(query_param("creator", "user-1"))
        .and(query_param("id", "a,b"))
        .and(query_param("name", "n1"))
        .and(query_param("live_input_id", "live"))
        .and(query_param("search", "keynote"))
        .and(query_param("video_name", "exact.mp4"))
        .and(query_param("status", "live-inprogress"))
        .and(query_param("type", "vod"))
        .and(query_param("limit", "10"))
        .and(query_param("include_counts", "true"))
        .respond_with(envelope_with(
            json!([{"uid": "a"}]),
            json!({"range": 0, "total": 1}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let page = client
        .videos()
        .list()
        .after(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        .asc(true)
        .creator("user-1")
        .ids(["a", "b"])
        .names(["n1"])
        .live_input("live")
        .search("keynote")
        .video_name("exact.mp4")
        .status(VideoState::LiveInProgress)
        .kind(VideoKind::Vod)
        .limit(10)
        .include_counts(true)
        .send()
        .await
        .unwrap();
    assert_eq!(page.videos.len(), 1);
    assert_eq!(page.range, Some(0));
    assert_eq!(page.total, Some(1));
}

#[tokio::test]
async fn list_with_counts_reads_nested_result() {
    // With `include_counts`, the live API nests the videos and counts in
    // `result` (unlike the API reference, which puts the counts beside it).
    let (server, client) = setup().await;
    mock("GET", "")
        .and(query_param("include_counts", "true"))
        .respond_with(success(json!({
            "videos": [{"uid": "a"}, {"uid": "b"}],
            "total": 7,
            "range": 5
        })))
        .expect(1)
        .mount(&server)
        .await;
    let page = client
        .videos()
        .list()
        .include_counts(true)
        .send()
        .await
        .unwrap();
    assert_eq!(uids(&page.videos), ["a", "b"]);
    assert_eq!(page.total, Some(7));
    assert_eq!(page.range, Some(5));
}

#[tokio::test]
async fn invalid_list_sends_nothing() {
    let (server, client) = setup().await;
    let videos = client.videos();
    for request in [
        videos.list().limit(0),
        videos.list().limit(1001),
        videos.list().status(VideoState::Unknown),
        videos.list().kind(VideoKind::Unknown),
        videos.list().ids(["a,b"]),
        videos.list().ids([""]),
    ] {
        let err = request.send().await.unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{err:?}");
    }
    assert_no_requests(&server).await;
}

/// A minimal video for pagination tests.
fn v(uid: &str, created: &str) -> Value {
    json!({"uid": uid, "created": created})
}

fn uids(videos: &[Video]) -> Vec<&str> {
    videos
        .iter()
        .map(|v| v.uid.as_ref().unwrap().as_str())
        .collect()
}

#[tokio::test]
async fn list_all_pages_by_created_and_dedupes_boundaries() {
    let (server, client) = setup().await;
    let (t1, t2, t3) = (
        "2024-01-01T00:00:01.000Z",
        "2024-01-01T00:00:02.000Z",
        "2024-01-01T00:00:03.000Z",
    );
    mock("GET", "")
        .and(query_param_is_missing("before"))
        .respond_with(envelope_with(
            json!([v("a", t3), v("b", t2)]),
            json!({"range": 2}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    // The boundary is inclusive here: `b` comes back and must not repeat.
    mock("GET", "")
        .and(query_param("before", t2))
        .respond_with(envelope_with(
            json!([v("b", t2), v("c", t1)]),
            json!({"range": 1}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    mock("GET", "")
        .and(query_param("before", t1))
        .respond_with(envelope_with(json!([v("c", t1)]), json!({})))
        .expect(1)
        .mount(&server)
        .await;

    let videos: Vec<Video> = client
        .videos()
        .list()
        .limit(2)
        .into_stream()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(uids(&videos), ["a", "b", "c"]);
}

#[tokio::test]
async fn list_all_stops_on_zero_range() {
    let (server, client) = setup().await;
    mock("GET", "")
        .respond_with(envelope_with(
            json!([v("a", "t2"), v("b", "t1")]),
            json!({"range": 0}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let videos: Vec<Video> = client
        .videos()
        .list()
        .limit(2)
        .into_stream()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(videos.len(), 2);
}

#[tokio::test]
async fn list_all_stops_on_short_page_without_range() {
    let (server, client) = setup().await;
    mock("GET", "")
        .respond_with(success(json!([v("a", "t1")])))
        .expect(1)
        .mount(&server)
        .await;
    let videos: Vec<Video> = client.videos().list_all().try_collect().await.unwrap();
    assert_eq!(videos.len(), 1);
}

#[tokio::test]
async fn list_all_errors_instead_of_looping_on_identical_timestamps() {
    let (server, client) = setup().await;
    let t = "2024-01-01T00:00:00Z";
    mock("GET", "")
        .respond_with(envelope_with(
            json!([v("a", t), v("b", t)]),
            json!({"range": 5}),
        ))
        .expect(2)
        .mount(&server)
        .await;
    let mut stream = std::pin::pin!(client.videos().list().limit(2).into_stream());
    let mut seen = Vec::new();
    let err = loop {
        match stream.try_next().await {
            Ok(Some(video)) => seen.push(video.uid.unwrap().into_inner()),
            Ok(None) => panic!("stream ended without an error"),
            Err(err) => break err,
        }
    };
    assert_eq!(seen, ["a", "b"]);
    assert!(matches!(err, Error::InvalidInput(_)), "{err:?}");
    assert!(stream.try_next().await.unwrap().is_none());
}

#[tokio::test]
async fn list_all_ascending_uses_after() {
    let (server, client) = setup().await;
    mock("GET", "")
        .and(query_param_is_missing("after"))
        .and(query_param("asc", "true"))
        .respond_with(success(json!([v("a", "2024-01-01T00:00:00Z")])))
        .expect(1)
        .mount(&server)
        .await;
    mock("GET", "")
        .and(query_param("after", "2024-01-01T00:00:00Z"))
        .and(query_param_is_missing("before"))
        .respond_with(success(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    let videos: Vec<Video> = client
        .videos()
        .list()
        .asc(true)
        .limit(1)
        .into_stream()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(uids(&videos), ["a"]);
}

#[tokio::test]
async fn list_all_yields_errors_once() {
    let (server, client) = setup().await;
    mock("GET", "")
        .respond_with(failure(500, 10000, "boom"))
        .expect(1)
        .mount(&server)
        .await;
    let mut stream = std::pin::pin!(client.videos().list_all());
    assert!(stream.try_next().await.is_err());
    assert!(stream.try_next().await.unwrap().is_none());
}

#[tokio::test]
async fn basic_upload_is_multipart() {
    let (server, client) = setup().await;
    mock("POST", "")
        .respond_with(success(json!({"uid": UID})))
        .expect(1)
        .mount(&server)
        .await;
    let video = client
        .videos()
        .upload(
            UploadSource::bytes(b"fake-mp4".to_vec(), "intro.mp4").with_content_type("video/mp4"),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(video.uid.unwrap(), UID);
    let req = only_request(&server).await;
    let fields = multipart_fields(&req);
    assert_eq!(fields.len(), 1);
    let file = field(&fields, "file");
    assert_eq!(file.filename.as_deref(), Some("intro.mp4"));
    assert_eq!(file.content_type.as_deref(), Some("video/mp4"));
    assert_eq!(file.data, b"fake-mp4");
}

#[tokio::test]
async fn copy_sends_input_and_options() {
    let (server, client) = setup().await;
    mock("POST", "/copy")
        .respond_with(success(
            json!({"uid": UID, "status": {"state": "downloading"}}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let video = client
        .videos()
        .copy("https://example.com/a.mp4".parse().unwrap())
        .meta(&json!({"name": "a"}))
        .creator("user-1")
        .require_signed_urls(true)
        .allowed_origins(["example.com"])
        .thumbnail_timestamp_pct(0.25)
        .scheduled_deletion(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        .watermark("wm")
        .send()
        .await
        .unwrap();
    assert_eq!(video.status.unwrap().state, Some(VideoState::Downloading));
    assert_eq!(
        body_json(&only_request(&server).await),
        json!({
            "input": "https://example.com/a.mp4",
            "meta": {"name": "a"},
            "creator": "user-1",
            "requireSignedURLs": true,
            "allowedOrigins": ["example.com"],
            "thumbnailTimestampPct": 0.25,
            "scheduledDeletion": "2023-11-14T22:13:20Z",
            "watermark": {"uid": "wm"},
        })
    );
}

#[tokio::test]
async fn clip_sends_range_and_validates_it() {
    let (server, client) = setup().await;
    mock("POST", "/clip")
        .respond_with(success(json!({"uid": "clip", "clippedFrom": UID})))
        .expect(1)
        .mount(&server)
        .await;
    let clip = client
        .videos()
        .clip(&UID.into(), 5, 15)
        .send()
        .await
        .unwrap();
    assert_eq!(clip.clipped_from.unwrap(), UID);
    assert_eq!(
        body_json(&only_request(&server).await),
        json!({"clippedFromVideoUID": UID, "startTimeSeconds": 5, "endTimeSeconds": 15})
    );

    let (server, client) = setup().await;
    for (start, end) in [(5, 5), (10, 1)] {
        let err = client
            .videos()
            .clip(&UID.into(), start, end)
            .send()
            .await
            .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
    }
    assert_no_requests(&server).await;
}

#[tokio::test]
async fn storage_usage() {
    let (server, client) = setup().await;
    mock("GET", "/storage-usage")
        .and(query_param("creator", "user-1"))
        .respond_with(success(json!({
            "creator": "user-1",
            "totalStorageMinutes": 12.5,
            "totalStorageMinutesLimit": 1000,
            "videoCount": 3
        })))
        .expect(1)
        .mount(&server)
        .await;
    let usage = client
        .videos()
        .storage_usage()
        .creator("user-1")
        .send()
        .await
        .unwrap();
    assert_eq!(usage.total_storage_minutes, Some(12.5));
    assert_eq!(usage.total_storage_minutes_limit, Some(1000));
    assert_eq!(usage.video_count, Some(3));
}

#[tokio::test]
async fn token_request_body_and_secret_response() {
    let (server, client) = setup().await;
    mock("POST", &format!("/{UID}/token"))
        .respond_with(success(json!({"token": "eyJhbGciOi.secret-token.sig"})))
        .expect(1)
        .mount(&server)
        .await;
    let token = client
        .videos()
        .create_token(&UID.into())
        .expires_at(UNIX_EPOCH + Duration::from_secs(2_000))
        .not_before(UNIX_EPOCH + Duration::from_secs(1_000))
        .downloadable(true)
        .original(true)
        .access_rule(AccessRule::ip(AccessAction::Allow, ["1.2.3.4/32"]))
        .send()
        .await
        .unwrap();
    assert_eq!(token.expose_secret(), "eyJhbGciOi.secret-token.sig");
    assert!(!format!("{token:?}").contains("secret-token"));
    assert_eq!(
        body_json(&only_request(&server).await),
        json!({
            "exp": 2000,
            "nbf": 1000,
            "downloadable": true,
            "flags": {"original": true},
            "accessRules": [{"type": "ip.src", "action": "allow", "ip": ["1.2.3.4/32"]}],
        })
    );
}

#[tokio::test]
async fn token_without_options_sends_empty_object() {
    let (server, client) = setup().await;
    mock("POST", &format!("/{UID}/token"))
        .respond_with(success(json!({"token": "t"})))
        .mount(&server)
        .await;
    client
        .videos()
        .create_token(&UID.into())
        .send()
        .await
        .unwrap();
    assert_eq!(body_json(&only_request(&server).await), json!({}));
}

#[tokio::test]
async fn path_segments_are_encoded() {
    let (server, client) = setup().await;
    mock("GET", "/a%2Fb")
        .respond_with(success(json!({"uid": "a/b"})))
        .expect(1)
        .mount(&server)
        .await;
    client.videos().get(&"a/b".into()).await.unwrap();
}
