mod common;

use cf_stream::{AudioTrackStatus, UpdateAudioTrack};
use common::*;
use serde_json::json;

const UID: &str = "ea95132c15732412d22c1476fa83f27a";
const TRACK: &str = "f3f0b5c2a2b14f2a9a0a6d2b6c1d9e8f";

#[tokio::test]
async fn list_unwraps_audio_array() {
    let (server, client) = setup().await;
    mock("GET", &format!("/{UID}/audio"))
        .respond_with(success(json!({"audio": [
            {"uid": TRACK, "label": "Deutsch", "default": false, "status": "ready"}
        ]})))
        .expect(1)
        .mount(&server)
        .await;
    let tracks = client.audio_tracks().list(&UID.into()).await.unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].status, Some(AudioTrackStatus::Ready));
}

#[tokio::test]
async fn copy_update_delete() {
    let (server, client) = setup().await;
    mock("POST", &format!("/{UID}/audio/copy"))
        .respond_with(success(
            json!({"uid": TRACK, "label": "Deutsch", "status": "queued"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    mock("PATCH", &format!("/{UID}/audio/{TRACK}"))
        .respond_with(success(json!({"uid": TRACK, "default": true})))
        .expect(1)
        .mount(&server)
        .await;
    mock("DELETE", &format!("/{UID}/audio/{TRACK}"))
        .respond_with(success(json!("ok")))
        .expect(1)
        .mount(&server)
        .await;

    let audio = client.audio_tracks();
    let track = audio
        .copy(
            &UID.into(),
            "Deutsch",
            "https://example.com/de.m4a".parse().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(track.status, Some(AudioTrackStatus::Queued));
    let track = audio
        .update(
            &UID.into(),
            &TRACK.into(),
            UpdateAudioTrack::new().default(true),
        )
        .await
        .unwrap();
    assert_eq!(track.default, Some(true));
    audio.delete(&UID.into(), &TRACK.into()).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        body_json(&requests[0]),
        json!({"label": "Deutsch", "url": "https://example.com/de.m4a"})
    );
    assert_eq!(body_json(&requests[1]), json!({"default": true}));
}
