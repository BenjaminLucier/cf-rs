mod common;

use common::*;
use serde_json::json;

fn keys() -> serde_json::Value {
    json!({"keys": [
        {"name": "default", "value": "Oix0bbNaT8Rge9PuyxUBrjI6zrgnsyJ5="},
        {"name": "backend", "value": "second-secret-value"}
    ]})
}

#[tokio::test]
async fn list_signing_keys() {
    let (server, client) = setup().await;
    mock("GET", "/v1/keys")
        .respond_with(success(keys()))
        .expect(1)
        .mount(&server)
        .await;
    let keys = client.signing_keys().list().await.unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0].name.as_ref().unwrap(), "default");
    assert_eq!(keys[0].value(), Some("Oix0bbNaT8Rge9PuyxUBrjI6zrgnsyJ5="));

    let debug = format!("{keys:?}");
    assert!(!debug.contains("Oix0bb"), "{debug}");
    assert!(!debug.contains("second-secret-value"), "{debug}");
}

#[tokio::test]
async fn create_signing_key_uses_put() {
    let (server, client) = setup().await;
    mock("PUT", "/v1/keys/backend")
        .respond_with(success(keys()))
        .expect(1)
        .mount(&server)
        .await;
    let keys = client
        .signing_keys()
        .create(&"backend".into())
        .await
        .unwrap();
    assert_eq!(keys.len(), 2);
}

#[tokio::test]
async fn delete_signing_key() {
    let (server, client) = setup().await;
    mock("DELETE", "/v1/keys/backend")
        .respond_with(success(
            json!({"keys": [{"name": "default", "value": "v"}]}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let remaining = client
        .signing_keys()
        .delete(&"backend".into())
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
}

#[tokio::test]
async fn signing_key_error() {
    let (server, client) = setup().await;
    mock("DELETE", "/v1/keys/default")
        .respond_with(failure(400, 5400, "Cannot delete default key"))
        .mount(&server)
        .await;
    let err = client
        .signing_keys()
        .delete(&"default".into())
        .await
        .unwrap_err();
    assert_eq!(err.status(), Some(400));
}
