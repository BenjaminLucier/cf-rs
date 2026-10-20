//! Error handling and client configuration.

mod common;

use std::time::Duration;

use cf_images::{Client, Error};
use common::*;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn rate_limit_exposes_retry_after() {
    let (server, client) = setup().await;
    mock("GET", "/v1/stats")
        .respond_with(
            failure(
                429,
                971,
                "Please wait and consider throttling your request speed",
            )
            .insert_header("retry-after", "17"),
        )
        .mount(&server)
        .await;
    let err = client.stats().await.unwrap_err();
    assert!(err.is_rate_limited());
    assert_eq!(err.retry_after(), Some(Duration::from_secs(17)));
    assert_eq!(err.api_errors()[0].code(), 971);
}

#[tokio::test]
async fn unauthorized() {
    let (server, client) = setup().await;
    mock("GET", "/v1/stats")
        .respond_with(failure(401, 10001, "Unable to authenticate request"))
        .mount(&server)
        .await;
    let err = client.stats().await.unwrap_err();
    assert!(err.is_unauthorized());
    assert!(!err.is_not_found());
    assert!(
        err.to_string()
            .contains("Unable to authenticate request (10001)")
    );
}

#[tokio::test]
async fn success_false_with_200_is_an_error() {
    let (server, client) = setup().await;
    mock("GET", "/v1/stats")
        .respond_with(failure(200, 5000, "odd"))
        .mount(&server)
        .await;
    let err = client.stats().await.unwrap_err();
    assert!(matches!(err, Error::Api { status: 200, .. }), "{err:?}");
}

#[tokio::test]
async fn malformed_json_is_a_decode_error_with_snippet() {
    let (server, client) = setup().await;
    mock("GET", "/v1/stats")
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"success\": tru"))
        .mount(&server)
        .await;
    match client.stats().await.unwrap_err() {
        Error::Decode { body, .. } => assert_eq!(body, "{\"success\": tru"),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn wrong_result_shape_is_a_decode_error() {
    let (server, client) = setup().await;
    mock("GET", "/v1/stats")
        .respond_with(success(json!({"count": {"allowed": "lots"}})))
        .mount(&server)
        .await;
    assert!(matches!(
        client.stats().await.unwrap_err(),
        Error::Decode { .. }
    ));
}

#[tokio::test]
async fn connection_failure_is_http_error() {
    let client = Client::builder("acc", "tok")
        // Port 9 (discard) on localhost is almost certainly closed.
        .base_url("http://127.0.0.1:9/client/v4")
        .build()
        .unwrap();
    let err = client.stats().await.unwrap_err();
    assert!(matches!(err, Error::Http(_)), "{err:?}");
    assert_eq!(err.status(), None);
}

#[tokio::test]
async fn timeout_applies() {
    let (server, _) = setup().await;
    mock("GET", "/v1/stats")
        .respond_with(success(json!({})).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;
    let client = Client::builder(ACCOUNT, TOKEN)
        .base_url(format!("{}/client/v4", server.uri()))
        .timeout(Duration::from_millis(100))
        .build()
        .unwrap();
    let err = client.stats().await.unwrap_err();
    match err {
        Error::Http(e) => assert!(e.is_timeout(), "{e:?}"),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn base_url_without_trailing_slash_or_prefix() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/accounts/{ACCOUNT}/images/v1/stats")))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(success(json!({"count": {"allowed": 1, "current": 0}})))
        .expect(1)
        .mount(&server)
        .await;
    let client = Client::builder(ACCOUNT, TOKEN)
        .base_url(server.uri())
        .build()
        .unwrap();
    client.stats().await.unwrap();
}

#[tokio::test]
async fn default_client_sends_user_agent() {
    let (server, client) = setup().await;
    mock("GET", "/v1/stats")
        .and(header(
            "user-agent",
            concat!("cf-images/", env!("CARGO_PKG_VERSION")),
        ))
        .respond_with(success(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    client.stats().await.unwrap();
}

#[test]
fn invalid_base_url() {
    let err = Client::builder("acc", "tok")
        .base_url("::nope")
        .build()
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)));
}
