//! Tests against the real Cloudflare API. All are `#[ignore]`d; run with:
//!
//! ```sh
//! CF_ACCOUNT_ID=... CF_API_TOKEN=... CF_ACCOUNT_HASH=... \
//!     cargo test -p cf-images --all-features --test live -- --ignored --test-threads=1
//! ```
//!
//! The token needs **Account > Cloudflare Images > Edit**. Every test deletes
//! what it creates, even when an assertion fails midway.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use cf_images::{AccountHash, Client, Delivery, ImageId, UpdateImage, UploadSource};
use serde_json::json;

fn env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} must be set for live tests"))
}

fn client() -> Result<Client> {
    Ok(Client::new(env("CF_ACCOUNT_ID")?, env("CF_API_TOKEN")?)?)
}

fn unique(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}{nanos}")
}

/// A valid 1x1 transparent PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

/// Deletes the image when dropped unless `disarm`ed.
struct Cleanup {
    client: Client,
    id: Option<ImageId>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            let client = self.client.clone();
            // Drop can't be async; run the delete on a fresh runtime thread.
            let _ = std::thread::spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map(|rt| rt.block_on(client.images().delete(&id)))
            })
            .join();
        }
    }
}

#[tokio::test]
#[ignore = "requires Cloudflare credentials"]
async fn full_lifecycle() -> Result<()> {
    let client = client()?;
    let account_hash = AccountHash::new(env("CF_ACCOUNT_HASH")?);

    // Upload
    let image = client
        .images()
        .upload(
            UploadSource::bytes(PNG.to_vec(), "cf-images-live.png").with_content_type("image/png"),
        )
        .metadata(&json!({"test": "cf-images-live"}))
        .creator("cf-images-live")
        .send()
        .await?;
    let id = image.id.clone().context("uploaded image has an id")?;
    let mut cleanup = Cleanup {
        client: client.clone(),
        id: Some(id.clone()),
    };
    assert_eq!(image.filename.as_deref(), Some("cf-images-live.png"));
    assert!(
        image
            .variants
            .iter()
            .flatten()
            .all(|v| v.contains(account_hash.as_str())),
        "variant URLs should contain CF_ACCOUNT_HASH: {:?}",
        image.variants
    );

    // Get
    let fetched = client.images().get(&id).await?;
    assert_eq!(fetched.creator.as_deref(), Some("cf-images-live"));
    assert_eq!(fetched.meta, Some(json!({"test": "cf-images-live"})));

    // Blob
    let blob = client.images().blob(&id).await?;
    assert!(blob.starts_with(b"\x89PNG"), "blob should be the PNG");

    // Update: require signed URLs. Cloudflare may assign a new ID.
    let updated = client
        .images()
        .update(
            &id,
            UpdateImage::new()
                .require_signed_urls(true)
                .metadata(&json!({"updated": true})),
        )
        .await?;
    assert_eq!(updated.require_signed_urls, Some(true));
    let id = updated.id.clone().unwrap_or(id);
    cleanup.id = Some(id.clone());

    // Signed URL
    #[cfg(feature = "signing")]
    {
        let keys = client.signing_keys().list().await?;
        let key = keys
            .iter()
            .find_map(|k| k.value())
            .context("account has a signing key")?;
        let signer = cf_images::UrlSigner::new(key);
        let delivery = Delivery::new(account_hash.clone());
        let url = delivery.url(&id, &"public".into());

        let http = cf_images::reqwest::Client::new();
        let unsigned = http.get(url.clone()).send().await?;
        assert!(
            !unsigned.status().is_success(),
            "private image served without signature"
        );

        let signed = signer.sign_for(&url, Duration::from_secs(300))?;
        let response = http.get(signed.clone()).send().await?;
        assert!(
            response.status().is_success(),
            "signed URL {signed} returned {}",
            response.status()
        );
    }
    #[cfg(not(feature = "signing"))]
    let _ = Delivery::new(account_hash);

    // Delete
    client.images().delete(&id).await?;
    cleanup.id = None;
    let err = client.images().get(&id).await.unwrap_err();
    assert!(err.is_not_found(), "{err:?}");
    Ok(())
}

#[tokio::test]
#[ignore = "requires Cloudflare credentials"]
async fn list_stats_variants_keys() -> Result<()> {
    let client = client()?;
    let page = client.images().list().per_page(10).send().await?;
    assert!(page.images.len() <= 10);
    let stats = client.stats().await?;
    assert!(stats.count.is_some());
    let variants = client.variants().list().await?;
    assert!(variants.contains_key("public"), "{variants:?}");
    let keys = client.signing_keys().list().await?;
    assert!(!keys.is_empty());
    Ok(())
}

#[tokio::test]
#[ignore = "requires Cloudflare credentials"]
async fn custom_id_with_slash() -> Result<()> {
    let client = client()?;
    let custom = ImageId::new(unique("cf-images-live/custom-"));
    let image = client
        .images()
        .upload(UploadSource::bytes(PNG.to_vec(), "custom.png"))
        .id(custom.clone())
        .send()
        .await?;
    let id = image.id.context("id")?;
    let _cleanup = Cleanup {
        client: client.clone(),
        id: Some(id.clone()),
    };
    assert_eq!(id, custom);
    let fetched = client.images().get(&id).await?;
    assert_eq!(fetched.id, Some(custom));
    Ok(())
}

#[tokio::test]
#[ignore = "requires Cloudflare credentials"]
async fn direct_upload_url() -> Result<()> {
    let client = client()?;
    let upload = client
        .direct_uploads()
        .create()
        .expires_in(Duration::from_secs(5 * 60))
        .metadata(&json!({"test": "cf-images-live"}))
        .send()
        .await?;
    let id = upload.id.context("id")?;
    let _cleanup = Cleanup {
        client: client.clone(),
        id: Some(id.clone()),
    };
    let upload_url = upload.upload_url.context("uploadURL")?;

    let form = cf_images::reqwest::multipart::Form::new().part(
        "file",
        cf_images::reqwest::multipart::Part::bytes(PNG.to_vec()).file_name("direct.png"),
    );
    let response = cf_images::reqwest::Client::new()
        .post(&upload_url)
        .multipart(form)
        .send()
        .await?;
    assert!(response.status().is_success(), "{}", response.status());

    let image = client.images().get(&id).await?;
    assert_ne!(image.draft, Some(true));
    Ok(())
}

#[tokio::test]
#[ignore = "requires Cloudflare credentials"]
async fn variant_lifecycle() -> Result<()> {
    use cf_images::{Fit, NewVariant, UpdateVariant, VariantOptions};

    let client = client()?;
    let name = unique("cfrs");
    let name = &name[..name.len().min(30)];
    let created = client
        .variants()
        .create(NewVariant::new(
            name,
            VariantOptions::new(Fit::Cover, 64, 64),
        ))
        .await?;
    let id = created.id.clone();
    let result = async {
        assert_eq!(created.options.width, 64);
        let fetched = client.variants().get(&id).await?;
        assert_eq!(fetched.options.fit, Fit::Cover);
        let updated = client
            .variants()
            .update(
                &id,
                UpdateVariant::new(VariantOptions::new(Fit::Contain, 128, 128)),
            )
            .await?;
        assert_eq!(updated.options.width, 128);
        anyhow::Ok(())
    }
    .await;
    client.variants().delete(&id).await?;
    result
}
