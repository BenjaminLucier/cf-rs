mod common;

use cf_stream::{CaptionStatus, Error};
use common::*;
use serde_json::json;
use wiremock::ResponseTemplate;

const UID: &str = "ea95132c15732412d22c1476fa83f27a";

#[tokio::test]
async fn list_and_get() {
    let (server, client) = setup().await;
    mock("GET", &format!("/{UID}/captions"))
        .respond_with(success(json!([
            {"language": "en", "label": "English", "generated": true, "status": "ready"},
            {"language": "de", "label": "Deutsch", "generated": false},
            {"language": "fr", "label": "Français", "generated": true, "status": "queued-for-later"}
        ])))
        .expect(1)
        .mount(&server)
        .await;
    mock("GET", &format!("/{UID}/captions/pt-BR"))
        .respond_with(success(
            json!({"language": "pt-BR", "status": "inprogress"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let captions = client.captions().list(&UID.into()).await.unwrap();
    assert_eq!(captions.len(), 3);
    assert_eq!(captions[0].status, Some(CaptionStatus::Ready));
    assert_eq!(captions[1].status, None);
    assert_eq!(captions[2].status, Some(CaptionStatus::Unknown));
    let caption = client.captions().get(&UID.into(), "pt-BR").await.unwrap();
    assert_eq!(caption.status, Some(CaptionStatus::InProgress));
}

#[tokio::test]
async fn upload_is_a_multipart_put() {
    let (server, client) = setup().await;
    mock("PUT", &format!("/{UID}/captions/en"))
        .respond_with(success(json!({"language": "en", "generated": false})))
        .expect(1)
        .mount(&server)
        .await;
    let vtt = "WEBVTT\n\n00:00.000 --> 00:01.000\nHello\n";
    client
        .captions()
        .upload(&UID.into(), "en", vtt.as_bytes().to_vec())
        .await
        .unwrap();
    let req = only_request(&server).await;
    let fields = multipart_fields(&req);
    let file = field(&fields, "file");
    assert_eq!(file.text(), vtt);
    assert_eq!(file.filename.as_deref(), Some("en.vtt"));
    assert_eq!(file.content_type.as_deref(), Some("text/vtt"));
}

#[tokio::test]
async fn generate_and_delete() {
    let (server, client) = setup().await;
    mock("POST", &format!("/{UID}/captions/en/generate"))
        .respond_with(success(
            json!({"language": "en", "generated": true, "status": "inprogress"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    // Caption deletes answer with an empty-string result.
    mock("DELETE", &format!("/{UID}/captions/en"))
        .respond_with(success(json!("")))
        .expect(1)
        .mount(&server)
        .await;
    let caption = client.captions().generate(&UID.into(), "en").await.unwrap();
    assert_eq!(caption.generated, Some(true));
    client.captions().delete(&UID.into(), "en").await.unwrap();
}

#[tokio::test]
async fn vtt_is_text() {
    let (server, client) = setup().await;
    mock("GET", &format!("/{UID}/captions/en/vtt"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/vtt")
                .set_body_string("WEBVTT\n\nhello"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let vtt = client.captions().vtt(&UID.into(), "en").await.unwrap();
    assert_eq!(vtt, "WEBVTT\n\nhello");
}

#[tokio::test]
async fn vtt_errors() {
    let (server, client) = setup().await;
    mock("GET", &format!("/{UID}/captions/xx/vtt"))
        .respond_with(failure(404, 10005, "Caption not found"))
        .mount(&server)
        .await;
    let err = client.captions().vtt(&UID.into(), "xx").await.unwrap_err();
    assert!(err.is_not_found());

    // A JSON error envelope served with 200.
    let (server, client) = setup().await;
    mock("GET", &format!("/{UID}/captions/xx/vtt"))
        .respond_with(failure(200, 10005, "Caption not found"))
        .mount(&server)
        .await;
    let err = client.captions().vtt(&UID.into(), "xx").await.unwrap_err();
    assert!(matches!(err, Error::Api { status: 200, .. }), "{err:?}");

    let (server, client) = setup().await;
    mock("GET", &format!("/{UID}/captions/xx/vtt"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0xff, 0xfe]))
        .mount(&server)
        .await;
    let err = client.captions().vtt(&UID.into(), "xx").await.unwrap_err();
    assert!(matches!(err, Error::Decode { .. }), "{err:?}");
}

#[tokio::test]
async fn empty_language_is_rejected() {
    let (server, client) = setup().await;
    let err = client.captions().get(&UID.into(), "").await.unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)));
    assert_no_requests(&server).await;
}
