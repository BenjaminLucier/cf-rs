mod common;

use common::*;
use serde_json::json;

#[tokio::test]
async fn stats() {
    let (server, client) = setup().await;
    mock("GET", "/v1/stats")
        .respond_with(success(
            json!({"count": {"allowed": 100000, "current": 1000}}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let stats = client.stats().await.unwrap();
    let count = stats.count.unwrap();
    assert_eq!(count.allowed, Some(100_000));
    assert_eq!(count.current, Some(1000));
}

#[tokio::test]
async fn stats_error() {
    let (server, client) = setup().await;
    mock("GET", "/v1/stats")
        .respond_with(failure(403, 10000, "Authentication error"))
        .mount(&server)
        .await;
    let err = client.stats().await.unwrap_err();
    assert!(err.is_unauthorized());
}
