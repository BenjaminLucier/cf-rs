mod common;

use common::*;
use serde_json::json;

#[tokio::test]
async fn set_sends_camel_case_and_reads_both_spellings() {
    let (server, client) = setup().await;
    // The API echoes the URL under both names.
    mock("PUT", "/webhook")
        .respond_with(success(json!({
            "notificationUrl": "https://example.com/hook",
            "notification_url": "https://example.com/hook",
            "modified": "2024-01-01T00:00:00Z",
            "secret": "85011ed3a913c6ad5f9cf6c5573cc0a7"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let webhook = client
        .webhooks()
        .set("https://example.com/hook".parse().unwrap())
        .await
        .unwrap();
    assert_eq!(
        webhook.notification_url.as_deref(),
        Some("https://example.com/hook")
    );
    assert_eq!(webhook.secret(), Some("85011ed3a913c6ad5f9cf6c5573cc0a7"));
    assert!(!format!("{webhook:?}").contains("85011ed3"));
    assert_eq!(
        body_json(&only_request(&server).await),
        json!({"notificationUrl": "https://example.com/hook"})
    );
}

#[tokio::test]
async fn get_with_snake_case_only_and_delete() {
    let (server, client) = setup().await;
    mock("GET", "/webhook")
        .respond_with(success(
            json!({"notification_url": "https://example.com/hook"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    mock("DELETE", "/webhook")
        .respond_with(success(json!("ok")))
        .expect(1)
        .mount(&server)
        .await;
    let webhook = client.webhooks().get().await.unwrap();
    assert_eq!(
        webhook.notification_url.as_deref(),
        Some("https://example.com/hook")
    );
    assert_eq!(webhook.secret(), None);
    client.webhooks().delete().await.unwrap();
}

#[tokio::test]
async fn get_without_subscription_is_not_found() {
    // Observed live: no webhook set gives HTTP 404 with `success: false`.
    let (server, client) = setup().await;
    mock("GET", "/webhook")
        .respond_with(failure(404, 10003, "Not Found"))
        .mount(&server)
        .await;
    let err = client.webhooks().get().await.unwrap_err();
    assert!(err.is_not_found(), "{err:?}");
}
