mod common;

use cf_images::{Fit, MetadataBehavior, NewVariant, UpdateVariant, VariantOptions};
use common::*;
use serde_json::json;
use wiremock::matchers::body_json;

fn thumb() -> serde_json::Value {
    json!({
        "variant": {
            "id": "thumb",
            "options": {"fit": "cover", "width": 200, "height": 100, "metadata": "keep"},
            "neverRequireSignedURLs": true
        }
    })
}

#[tokio::test]
async fn list_returns_map_keyed_by_name() {
    let (server, client) = setup().await;
    mock("GET", "/v1/variants")
        .respond_with(success(fixture("variants.json")))
        .expect(1)
        .mount(&server)
        .await;

    let variants = client.variants().list().await.unwrap();
    assert_eq!(
        variants.keys().map(String::as_str).collect::<Vec<_>>(),
        ["public", "squiggle", "thumbnail"]
    );
    let public = &variants["public"];
    assert_eq!(public.options.fit, Fit::ScaleDown);
    assert_eq!(public.options.width, 1366);
    assert_eq!(public.options.metadata, MetadataBehavior::None);
    assert_eq!(public.never_require_signed_urls, Some(false));
    assert_eq!(
        variants["thumbnail"].options.metadata,
        MetadataBehavior::Copyright
    );
}

#[tokio::test]
async fn unknown_enum_values_deserialize_to_unknown() {
    let (server, client) = setup().await;
    mock("GET", "/v1/variants")
        .respond_with(success(fixture("variants.json")))
        .mount(&server)
        .await;
    let variants = client.variants().list().await.unwrap();
    let squiggle = &variants["squiggle"];
    assert_eq!(squiggle.options.fit, Fit::Unknown);
    assert_eq!(squiggle.options.metadata, MetadataBehavior::Unknown);
    assert_eq!(squiggle.never_require_signed_urls, None);
}

#[tokio::test]
async fn get_variant() {
    let (server, client) = setup().await;
    mock("GET", "/v1/variants/thumb")
        .respond_with(success(thumb()))
        .expect(1)
        .mount(&server)
        .await;
    let v = client.variants().get(&"thumb".into()).await.unwrap();
    assert_eq!(v.id, "thumb");
    assert_eq!(
        v.options,
        VariantOptions::new(Fit::Cover, 200, 100).metadata(MetadataBehavior::Keep)
    );
    assert_eq!(v.never_require_signed_urls, Some(true));
}

#[tokio::test]
async fn create_variant_sends_definition() {
    let (server, client) = setup().await;
    mock("POST", "/v1/variants")
        .and(body_json(json!({
            "id": "thumb",
            "options": {"fit": "cover", "width": 200, "height": 100, "metadata": "keep"},
            "neverRequireSignedURLs": true
        })))
        .respond_with(success(thumb()))
        .expect(1)
        .mount(&server)
        .await;
    let options = VariantOptions::new(Fit::Cover, 200, 100).metadata(MetadataBehavior::Keep);
    let v = client
        .variants()
        .create(NewVariant::new("thumb", options).never_require_signed_urls(true))
        .await
        .unwrap();
    assert_eq!(v.id, "thumb");
}

#[tokio::test]
async fn update_variant_sends_options_only_when_flag_unset() {
    let (server, client) = setup().await;
    mock("PATCH", "/v1/variants/thumb")
        .and(body_json(json!({
            "options": {"fit": "pad", "width": 300, "height": 300, "metadata": "none"}
        })))
        .respond_with(success(thumb()))
        .expect(1)
        .mount(&server)
        .await;
    client
        .variants()
        .update(
            &"thumb".into(),
            UpdateVariant::new(VariantOptions::new(Fit::Pad, 300, 300)),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn unknown_enum_values_are_not_sent() {
    let (server, client) = setup().await;
    let err = client
        .variants()
        .create(NewVariant::new(
            "x",
            VariantOptions::new(Fit::Unknown, 1, 1),
        ))
        .await
        .unwrap_err();
    assert!(matches!(err, cf_images::Error::InvalidInput(_)), "{err:?}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn delete_variant() {
    let (server, client) = setup().await;
    mock("DELETE", "/v1/variants/thumb")
        .respond_with(success(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    client.variants().delete(&"thumb".into()).await.unwrap();
}

#[tokio::test]
async fn variant_error_envelope() {
    let (server, client) = setup().await;
    mock("POST", "/v1/variants")
        .respond_with(failure(409, 5409, "Variant already exists"))
        .mount(&server)
        .await;
    let err = client
        .variants()
        .create(NewVariant::new(
            "public",
            VariantOptions::new(Fit::Cover, 1, 1),
        ))
        .await
        .unwrap_err();
    assert_eq!(err.status(), Some(409));
    assert_eq!(err.api_errors()[0].message(), "Variant already exists");
}
