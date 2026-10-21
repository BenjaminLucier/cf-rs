mod common;

use cf_stream::{Error, WatermarkPosition, WatermarkSource};
use common::*;
use serde_json::json;

fn watermark() -> serde_json::Value {
    json!({
        "uid": "ea95132c15732412d22c1476fa83f27a",
        "name": "Marketing Videos",
        "created": "2014-01-02T02:20:00Z",
        "downloadedFrom": null,
        "size": 29472,
        "height": 600,
        "width": 400,
        "opacity": 0.75,
        "padding": 0.1,
        "scale": 0.1,
        "position": "lowerRight"
    })
}

#[tokio::test]
async fn create_from_bytes_is_multipart() {
    let (server, client) = setup().await;
    mock("POST", "/watermarks")
        .respond_with(success(watermark()))
        .expect(1)
        .mount(&server)
        .await;
    let created = client
        .watermarks()
        .create(WatermarkSource::bytes(b"\x89PNG".to_vec(), "logo.png"))
        .name("Marketing Videos")
        .opacity(0.75)
        .padding(0.1)
        .scale(0.1)
        .position(WatermarkPosition::LowerRight)
        .send()
        .await
        .unwrap();
    assert_eq!(created.position, Some(WatermarkPosition::LowerRight));
    assert_eq!(created.width, Some(400));

    let req = only_request(&server).await;
    let fields = multipart_fields(&req);
    assert_eq!(field(&fields, "file").data, b"\x89PNG");
    assert_eq!(field(&fields, "file").filename.as_deref(), Some("logo.png"));
    assert_eq!(field(&fields, "name").text(), "Marketing Videos");
    assert_eq!(field(&fields, "opacity").text(), "0.75");
    assert_eq!(field(&fields, "position").text(), "lowerRight");
}

#[tokio::test]
async fn create_from_url_is_json() {
    let (server, client) = setup().await;
    mock("POST", "/watermarks")
        .respond_with(success(watermark()))
        .expect(1)
        .mount(&server)
        .await;
    client
        .watermarks()
        .create(WatermarkSource::url(
            "https://example.com/logo.png".parse().unwrap(),
        ))
        .position(WatermarkPosition::Center)
        .send()
        .await
        .unwrap();
    assert_eq!(
        body_json(&only_request(&server).await),
        json!({"url": "https://example.com/logo.png", "position": "center"})
    );
}

#[tokio::test]
async fn invalid_values_send_nothing() {
    let (server, client) = setup().await;
    let source = || WatermarkSource::url("https://example.com/l.png".parse().unwrap());
    let watermarks = client.watermarks();
    for request in [
        watermarks.create(source()).opacity(1.5),
        watermarks.create(source()).padding(-0.1),
        watermarks.create(source()).scale(f64::NAN),
        watermarks
            .create(source())
            .position(WatermarkPosition::Unknown),
    ] {
        let err = request.send().await.unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{err:?}");
    }
    assert_no_requests(&server).await;
}

#[tokio::test]
async fn list_get_delete() {
    let (server, client) = setup().await;
    let mut unknown = watermark();
    unknown["position"] = json!("somewhere");
    mock("GET", "/watermarks")
        .respond_with(success(json!([watermark(), unknown])))
        .expect(1)
        .mount(&server)
        .await;
    mock("GET", "/watermarks/ea95132c15732412d22c1476fa83f27a")
        .respond_with(success(watermark()))
        .expect(1)
        .mount(&server)
        .await;
    mock("DELETE", "/watermarks/ea95132c15732412d22c1476fa83f27a")
        .respond_with(success(json!("")))
        .expect(1)
        .mount(&server)
        .await;
    let list = client.watermarks().list().await.unwrap();
    assert_eq!(list[1].position, Some(WatermarkPosition::Unknown));
    let id = "ea95132c15732412d22c1476fa83f27a".into();
    client.watermarks().get(&id).await.unwrap();
    client.watermarks().delete(&id).await.unwrap();
}
