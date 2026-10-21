//! Verify a webhook notification, as a webhook handler would.
//!
//! This builds a correctly signed example notification so it runs offline;
//! in a real handler, `header` and `body` come from the incoming request.
//!
//! ```sh
//! cargo run --example stream_verify_webhook
//! ```

use std::time::{SystemTime, UNIX_EPOCH};

use cf_stream::{SIGNATURE_HEADER, Video, VideoState, WebhookError, WebhookVerifier};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

fn main() -> anyhow::Result<()> {
    // The secret returned by `client.webhooks().set(url)`.
    let secret = "85011ed3a913c6ad5f9cf6c5573cc0a7";
    let body = br#"{"uid":"ea95132c15732412d22c1476fa83f27a","readyToStream":true,"status":{"state":"ready"}}"#;
    let header = sign(secret, body);
    println!("{SIGNATURE_HEADER}: {header}");

    let verifier = WebhookVerifier::new(secret);
    verifier.verify(&header, body)?;
    let video: Video = serde_json::from_slice(body)?;
    assert_eq!(video.status.and_then(|s| s.state), Some(VideoState::Ready));
    println!("verified notification for {:?}", video.uid);

    let tampered = br#"{"uid":"someone-else"}"#;
    assert_eq!(
        verifier.verify(&header, tampered),
        Err(WebhookError::Mismatch)
    );
    println!("tampered body rejected");
    Ok(())
}

/// What Cloudflare does when sending a notification.
fn sign(secret: &str, body: &[u8]) -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{time}.").as_bytes());
    mac.update(body);
    format!(
        "time={time},sig1={}",
        hex::encode(mac.finalize().into_bytes())
    )
}
