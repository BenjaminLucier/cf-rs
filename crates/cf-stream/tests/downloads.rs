mod common;

use cf_stream::{DownloadStatus, DownloadType};
use common::*;
use serde_json::json;

const UID: &str = "ea95132c15732412d22c1476fa83f27a";

#[tokio::test]
async fn get_create_delete_per_type() {
    let (server, client) = setup().await;
    mock("GET", &format!("/{UID}/downloads"))
        .respond_with(success(json!({
            "default": {"status": "ready", "percentComplete": 100.0, "url": "https://example.com/default.mp4"},
            "audio": {"status": "inprogress", "percentComplete": 75.5}
        })))
        .expect(1)
        .mount(&server)
        .await;
    mock("POST", &format!("/{UID}/downloads/audio"))
        .respond_with(success(
            json!({"audio": {"status": "inprogress", "percentComplete": 0}}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    mock("DELETE", &format!("/{UID}/downloads/default"))
        .respond_with(success(json!("ok")))
        .expect(1)
        .mount(&server)
        .await;

    let downloads = client.downloads().get(&UID.into()).await.unwrap();
    let mp4 = downloads.default.unwrap();
    assert_eq!(mp4.status, Some(DownloadStatus::Ready));
    assert_eq!(mp4.url.as_deref(), Some("https://example.com/default.mp4"));
    assert_eq!(downloads.audio.unwrap().percent_complete, Some(75.5));

    let created = client
        .downloads()
        .create(&UID.into(), DownloadType::Audio)
        .await
        .unwrap();
    assert!(created.default.is_none());
    assert_eq!(
        created.audio.unwrap().status,
        Some(DownloadStatus::InProgress)
    );

    client
        .downloads()
        .delete(&UID.into(), DownloadType::Default)
        .await
        .unwrap();
}
