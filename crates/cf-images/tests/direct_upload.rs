mod common;

use std::time::{Duration, UNIX_EPOCH};

use common::*;
use serde_json::json;

#[tokio::test]
async fn direct_upload_sends_multipart_fields() {
    let (server, client) = setup().await;
    mock("POST", "/v2/direct_upload")
        .respond_with(success(json!({
            "id": "e22e9e6b-c02b-42fd-c405-6c32af5fe600",
            "uploadURL": "https://upload.imagedelivery.net/FxUufywByo0m2v3xhKSiU8/e22e9e6b-c02b-42fd-c405-6c32af5fe600"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let upload = client
        .direct_uploads()
        .create()
        .id("avatars/42")
        .expiry(UNIX_EPOCH + Duration::from_secs(1_631_635_200))
        .metadata(&json!({"key": "value"}))
        .require_signed_urls(false)
        .creator("user-42")
        .send()
        .await
        .unwrap();
    assert_eq!(upload.id.unwrap(), "e22e9e6b-c02b-42fd-c405-6c32af5fe600");
    assert!(
        upload
            .upload_url
            .unwrap()
            .starts_with("https://upload.imagedelivery.net/")
    );

    let req = only_request(&server).await;
    let fields = multipart_fields(&req);
    assert_eq!(field(&fields, "id").text(), "avatars/42");
    assert_eq!(field(&fields, "expiry").text(), "2021-09-14T16:00:00Z");
    assert_eq!(field(&fields, "metadata").text(), r#"{"key":"value"}"#);
    assert_eq!(field(&fields, "requireSignedURLs").text(), "false");
    assert_eq!(field(&fields, "creator").text(), "user-42");
}

#[tokio::test]
async fn direct_upload_with_no_options_sends_empty_form() {
    let (server, client) = setup().await;
    mock("POST", "/v2/direct_upload")
        .respond_with(success(
            json!({"id": "x", "uploadURL": "https://upload.imagedelivery.net/h/x"}),
        ))
        .mount(&server)
        .await;
    client.direct_uploads().create().send().await.unwrap();
    let req = only_request(&server).await;
    assert!(multipart_fields(&req).is_empty());
}

#[tokio::test]
async fn direct_upload_error() {
    let (server, client) = setup().await;
    mock("POST", "/v2/direct_upload")
        .respond_with(failure(400, 5400, "Bad expiry"))
        .mount(&server)
        .await;
    let err = client.direct_uploads().create().send().await.unwrap_err();
    assert_eq!(err.api_errors()[0].message(), "Bad expiry");
}
