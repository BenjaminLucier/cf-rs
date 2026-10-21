mod common;

use common::*;
use serde_json::json;

#[tokio::test]
async fn create_returns_redacted_key_material() {
    let (server, client) = setup().await;
    mock("POST", "/keys")
        .respond_with(success(json!({
            "id": "ee15edd5e5e8bb0a7c0b1a0c3a8b1c35",
            "created": "2024-01-01T00:00:00Z",
            "pem": "LS0tLS1CRUdJTiBQRU0tc2VjcmV0",
            "jwk": "eyJqd2stc2VjcmV0Ijp0cnVlfQ=="
        })))
        .expect(1)
        .mount(&server)
        .await;
    let key = client.signing_keys().create().await.unwrap();
    assert_eq!(key.id.as_ref().unwrap(), "ee15edd5e5e8bb0a7c0b1a0c3a8b1c35");
    assert_eq!(key.pem(), Some("LS0tLS1CRUdJTiBQRU0tc2VjcmV0"));
    assert_eq!(key.jwk(), Some("eyJqd2stc2VjcmV0Ijp0cnVlfQ=="));
    let debug = format!("{key:?}");
    assert!(!debug.contains("LS0tLS1"), "{debug}");
    assert!(!debug.contains("eyJqd2s"), "{debug}");
    assert!(only_request(&server).await.body.is_empty());
}

#[tokio::test]
async fn list_and_delete() {
    let (server, client) = setup().await;
    mock("GET", "/keys")
        .respond_with(success(json!([
            {"id": "a", "created": "2024-01-01T00:00:00Z", "key_id": "a"},
            {"id": "b", "created": "2024-01-02T00:00:00Z"}
        ])))
        .expect(1)
        .mount(&server)
        .await;
    mock("DELETE", "/keys/a")
        .respond_with(success(json!("ok")))
        .expect(1)
        .mount(&server)
        .await;
    let keys = client.signing_keys().list().await.unwrap();
    assert_eq!(keys.len(), 2);
    client.signing_keys().delete(&"a".into()).await.unwrap();
}
