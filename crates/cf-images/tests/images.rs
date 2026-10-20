mod common;

use bytes::Bytes;
use cf_images::{Error, ImageId, MetaFilter, SortOrder, UpdateImage, UploadSource};
use common::*;
use futures_util::TryStreamExt;
use serde_json::json;
use wiremock::matchers::{body_json, query_param, query_param_is_missing};
use wiremock::{Match, Request, ResponseTemplate};

fn image_id() -> ImageId {
    ImageId::new("083eb7b2-5392-4565-b69e-aff66acddd00")
}

#[tokio::test]
async fn upload_bytes_sends_all_multipart_fields() {
    let (server, client) = setup().await;
    mock("POST", "/v1")
        .respond_with(success(fixture("image.json")))
        .expect(1)
        .mount(&server)
        .await;

    let image = client
        .images()
        .upload(
            UploadSource::bytes(b"\x89PNG data".to_vec(), "cat.png").with_content_type("image/png"),
        )
        .id("custom/id-1")
        .metadata(&json!({"owner": "user-1", "n": 1}))
        .require_signed_urls(true)
        .creator("user-1")
        .send()
        .await
        .unwrap();
    assert_eq!(image.id, Some(image_id()));
    assert_eq!(image.filename.as_deref(), Some("cat.png"));
    assert_eq!(image.meta.as_ref().unwrap()["nested"]["n"], 1);
    assert_eq!(image.variants.as_ref().unwrap().len(), 2);

    let req = only_request(&server).await;
    let fields = multipart_fields(&req);
    let file = field(&fields, "file");
    assert_eq!(file.filename.as_deref(), Some("cat.png"));
    assert_eq!(file.content_type.as_deref(), Some("image/png"));
    assert_eq!(file.data, b"\x89PNG data");
    assert_eq!(field(&fields, "id").text(), "custom/id-1");
    let metadata: serde_json::Value =
        serde_json::from_str(field(&fields, "metadata").text()).unwrap();
    assert_eq!(metadata, json!({"owner": "user-1", "n": 1}));
    assert_eq!(field(&fields, "requireSignedURLs").text(), "true");
    assert_eq!(field(&fields, "creator").text(), "user-1");
    assert!(fields.iter().all(|f| f.name != "url"));
}

#[tokio::test]
async fn upload_from_url_sends_url_field_only() {
    let (server, client) = setup().await;
    mock("POST", "/v1")
        .respond_with(success(fixture("image.json")))
        .mount(&server)
        .await;

    let url = "https://example.com/path/to/logo.png".parse().unwrap();
    client
        .images()
        .upload(UploadSource::url(url))
        .send()
        .await
        .unwrap();

    let req = only_request(&server).await;
    let fields = multipart_fields(&req);
    assert_eq!(fields.len(), 1, "{fields:?}");
    assert_eq!(
        field(&fields, "url").text(),
        "https://example.com/path/to/logo.png"
    );
}

#[tokio::test]
async fn upload_from_stream() {
    let (server, client) = setup().await;
    mock("POST", "/v1")
        .respond_with(success(fixture("image.json")))
        .mount(&server)
        .await;

    let chunks = futures_util::stream::iter(vec![
        Ok::<_, std::io::Error>(Bytes::from_static(b"chunk-1;")),
        Ok(Bytes::from_static(b"chunk-2")),
    ]);
    client
        .images()
        .upload(UploadSource::stream(chunks, "big.jpg"))
        .send()
        .await
        .unwrap();

    let req = only_request(&server).await;
    let fields = multipart_fields(&req);
    let file = field(&fields, "file");
    assert_eq!(file.filename.as_deref(), Some("big.jpg"));
    assert_eq!(file.content_type, None, "no part content type unless set");
    assert_eq!(file.data, b"chunk-1;chunk-2");
}

#[tokio::test]
async fn upload_with_unserializable_metadata_fails_before_sending() {
    let (server, client) = setup().await;
    let mut bad = std::collections::HashMap::new();
    bad.insert((1, 2), "tuple keys can't be JSON object keys");

    let err = client
        .images()
        .upload(UploadSource::bytes(vec![1], "a.png"))
        .metadata(&bad)
        .send()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err:?}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn get_image() {
    let (server, client) = setup().await;
    mock("GET", &format!("/v1/{}", image_id()))
        .respond_with(success(fixture("image.json")))
        .expect(1)
        .mount(&server)
        .await;

    let image = client.images().get(&image_id()).await.unwrap();
    assert_eq!(image.creator.as_deref(), Some("user-1"));
    assert_eq!(image.require_signed_urls, Some(false));
    assert_eq!(image.uploaded.as_deref(), Some("2024-05-01T12:34:56.789Z"));
    assert_eq!(image.draft, None);
}

#[tokio::test]
async fn custom_ids_with_slashes_are_one_encoded_segment() {
    let (server, client) = setup().await;
    // `/` inside an ID is percent-encoded so it stays one path segment.
    // wiremock matches on the decoded path, so check the raw URL instead.
    mock("GET", "/v1/products%2F42")
        .respond_with(success(fixture("image.json")))
        .mount(&server)
        .await;
    let _ = client.images().get(&ImageId::new("products/42")).await;
    let req = only_request(&server).await;
    assert!(
        req.url.path().ends_with("/images/v1/products%2F42"),
        "{}",
        req.url
    );
}

#[tokio::test]
async fn image_with_only_some_fields_parses() {
    let (server, client) = setup().await;
    mock("GET", "/v1/x")
        .respond_with(success(json!({"id": "x", "draft": true})))
        .mount(&server)
        .await;
    let image = client.images().get(&"x".into()).await.unwrap();
    assert_eq!(image.id, Some("x".into()));
    assert_eq!(image.draft, Some(true));
    assert_eq!(image.variants, None);
}

#[tokio::test]
async fn update_sends_json_body_with_api_casing() {
    let (server, client) = setup().await;
    mock("PATCH", &format!("/v1/{}", image_id()))
        .and(body_json(json!({
            "metadata": {"reviewed": true},
            "requireSignedURLs": true,
            "creator": "user-2",
        })))
        .respond_with(success(fixture("image.json")))
        .expect(1)
        .mount(&server)
        .await;

    client
        .images()
        .update(
            &image_id(),
            UpdateImage::new()
                .metadata(&json!({"reviewed": true}))
                .require_signed_urls(true)
                .creator("user-2"),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn update_omits_unset_fields() {
    let (server, client) = setup().await;
    mock("PATCH", "/v1/x")
        .and(body_json(json!({"creator": "c"})))
        .respond_with(success(fixture("image.json")))
        .expect(1)
        .mount(&server)
        .await;
    client
        .images()
        .update(&"x".into(), UpdateImage::new().creator("c"))
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_accepts_empty_object_and_null_results() {
    let (server, client) = setup().await;
    mock("DELETE", "/v1/a")
        .respond_with(success(json!({})))
        .mount(&server)
        .await;
    mock("DELETE", "/v1/b")
        .respond_with(success(json!(null)))
        .mount(&server)
        .await;
    mock("DELETE", "/v1/c")
        .respond_with(success(json!("")))
        .mount(&server)
        .await;
    client.images().delete(&"a".into()).await.unwrap();
    client.images().delete(&"b".into()).await.unwrap();
    client.images().delete(&"c".into()).await.unwrap();
}

#[tokio::test]
async fn delete_not_found() {
    let (server, client) = setup().await;
    mock("DELETE", "/v1/missing")
        .respond_with(failure(404, 5404, "Image not found"))
        .mount(&server)
        .await;
    let err = client.images().delete(&"missing".into()).await.unwrap_err();
    assert!(err.is_not_found(), "{err:?}");
    assert_eq!(err.api_errors()[0].code(), 5404);
    assert_eq!(err.api_errors()[0].message(), "Image not found");
}

#[tokio::test]
async fn empty_id_is_rejected_without_a_request() {
    let (server, client) = setup().await;
    let err = client.images().delete(&"".into()).await.unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "{err:?}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn blob_returns_raw_bytes() {
    let (server, client) = setup().await;
    mock("GET", "/v1/x/blob")
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/png")
                .set_body_bytes(b"\x89PNG\r\n\x1a\nrest".to_vec()),
        )
        .mount(&server)
        .await;
    let bytes = client.images().blob(&"x".into()).await.unwrap();
    assert_eq!(&bytes[..], b"\x89PNG\r\n\x1a\nrest");
}

#[tokio::test]
async fn blob_error_envelope() {
    let (server, client) = setup().await;
    mock("GET", "/v1/missing/blob")
        .respond_with(failure(404, 5404, "Image not found"))
        .mount(&server)
        .await;
    let err = client.images().blob(&"missing".into()).await.unwrap_err();
    assert!(err.is_not_found());
    assert_eq!(err.api_errors()[0].code(), 5404);
}

#[tokio::test]
async fn blob_error_with_non_json_body() {
    let (server, client) = setup().await;
    mock("GET", "/v1/x/blob")
        .respond_with(ResponseTemplate::new(500).set_body_string("upstream exploded"))
        .mount(&server)
        .await;
    let err = client.images().blob(&"x".into()).await.unwrap_err();
    assert_eq!(err.status(), Some(500));
    assert!(err.api_errors().is_empty());
}

#[tokio::test]
async fn blob_json_error_with_200_status() {
    let (server, client) = setup().await;
    mock("GET", "/v1/x/blob")
        .respond_with(failure(200, 5455, "Unsupported image format"))
        .mount(&server)
        .await;
    let err = client.images().blob(&"x".into()).await.unwrap_err();
    assert_eq!(err.api_errors()[0].code(), 5455);
}

#[tokio::test]
async fn list_v2_sends_query_params() {
    let (server, client) = setup().await;
    mock("GET", "/v2")
        .and(query_param("per_page", "100"))
        .and(query_param("sort_order", "asc"))
        .and(query_param("creator", "user-1"))
        .and(query_param("continuation_token", "tok"))
        .and(query_param("meta.status[eq]", "active"))
        .and(query_param("meta.size[gte]", "1024"))
        .and(query_param("meta.tag[in]", "a|b"))
        .respond_with(success(json!({
            "images": [fixture("image.json")],
            "continuation_token": "next"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let page = client
        .images()
        .list()
        .per_page(100)
        .sort_order(SortOrder::Asc)
        .creator("user-1")
        .continuation_token("tok")
        .meta_filter(MetaFilter::eq("status", "active"))
        .meta_filter(MetaFilter::gte("size", 1024))
        .meta_filter(MetaFilter::any_of("tag", ["a", "b"]))
        .send()
        .await
        .unwrap();
    assert_eq!(page.images.len(), 1);
    assert_eq!(page.continuation_token.as_deref(), Some("next"));
}

#[tokio::test]
async fn list_v2_without_options_sends_no_query() {
    let (server, client) = setup().await;
    mock("GET", "/v2")
        .respond_with(success(json!({"images": [], "continuation_token": null})))
        .mount(&server)
        .await;
    let page = client.images().list().send().await.unwrap();
    assert!(page.images.is_empty());
    assert_eq!(page.continuation_token, None);
    assert_eq!(only_request(&server).await.url.query(), None);
}

/// Matches requests whose `continuation_token` query param equals `token`
/// (`None`: param absent).
struct Token(Option<&'static str>);

impl Match for Token {
    fn matches(&self, request: &Request) -> bool {
        let found = request
            .url
            .query_pairs()
            .find(|(k, _)| k == "continuation_token")
            .map(|(_, v)| v.into_owned());
        found.as_deref() == self.0
    }
}

fn page(ids: &[&str], token: Option<&str>) -> ResponseTemplate {
    let images: Vec<_> = ids.iter().map(|id| json!({"id": id})).collect();
    success(json!({"images": images, "continuation_token": token}))
}

#[tokio::test]
async fn list_all_follows_continuation_tokens_across_three_pages() {
    let (server, client) = setup().await;
    mock("GET", "/v2")
        .and(Token(None))
        .and(query_param("per_page", "10"))
        .respond_with(page(&["a", "b"], Some("t1")))
        .expect(1)
        .mount(&server)
        .await;
    mock("GET", "/v2")
        .and(Token(Some("t1")))
        .and(query_param("per_page", "10"))
        .respond_with(page(&["c", "d"], Some("t2")))
        .expect(1)
        .mount(&server)
        .await;
    mock("GET", "/v2")
        .and(Token(Some("t2")))
        .respond_with(page(&["e"], None))
        .expect(1)
        .mount(&server)
        .await;

    let images: Vec<_> = client
        .images()
        .list()
        .per_page(10)
        .into_stream()
        .try_collect()
        .await
        .unwrap();
    let ids: Vec<_> = images
        .iter()
        .map(|i| i.id.as_ref().unwrap().as_str())
        .collect();
    assert_eq!(ids, ["a", "b", "c", "d", "e"]);
}

#[tokio::test]
async fn list_all_stops_on_repeated_token_and_on_error() {
    let (server, client) = setup().await;
    mock("GET", "/v2")
        .and(Token(None))
        .respond_with(page(&["a"], Some("same")))
        .mount(&server)
        .await;
    mock("GET", "/v2")
        .and(Token(Some("same")))
        .respond_with(page(&["b"], Some("same")))
        .expect(1)
        .mount(&server)
        .await;
    let images: Vec<_> = client.images().list_all().try_collect().await.unwrap();
    assert_eq!(images.len(), 2);

    let (server, client) = setup().await;
    mock("GET", "/v2")
        .and(Token(None))
        .respond_with(page(&["a"], Some("t1")))
        .mount(&server)
        .await;
    mock("GET", "/v2")
        .and(Token(Some("t1")))
        .respond_with(failure(500, 5000, "internal"))
        .mount(&server)
        .await;
    let results: Vec<_> =
        futures_util::StreamExt::collect::<Vec<_>>(client.images().list_all()).await;
    assert_eq!(results.len(), 2);
    assert!(results[0].is_ok());
    assert_eq!(results[1].as_ref().unwrap_err().status(), Some(500));
}

#[tokio::test]
async fn metadata_keys() {
    let (server, client) = setup().await;
    mock("GET", "/v2/metadata/keys")
        .respond_with(success(
            json!({"keys": ["owner", "status"], "truncated": false}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let keys = client.images().metadata_keys().await.unwrap();
    assert_eq!(keys.keys, ["owner", "status"]);
    assert_eq!(keys.truncated, Some(false));
}

#[tokio::test]
#[allow(deprecated)]
async fn list_v1_uses_page_numbers() {
    let (server, client) = setup().await;
    mock("GET", "/v1")
        .and(query_param("page", "2"))
        .and(query_param("per_page", "50"))
        .and(query_param_is_missing("continuation_token"))
        .respond_with(success(json!({"images": [fixture("image.json")]})))
        .expect(1)
        .mount(&server)
        .await;
    let images = client
        .images()
        .list_v1()
        .page(2)
        .per_page(50)
        .send()
        .await
        .unwrap();
    assert_eq!(images.len(), 1);
}
