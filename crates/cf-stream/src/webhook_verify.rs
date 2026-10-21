//! Webhook signature verification (feature `webhooks`).
//!
//! Cloudflare signs each notification with the webhook secret and sends
//! `Webhook-Signature: time=<unix seconds>,sig1=<hex HMAC-SHA256>`, where the
//! HMAC covers `"{time}.{raw body}"`. See Cloudflare's
//! [webhook guide](https://developers.cloudflare.com/stream/manage-video-library/using-webhooks/).

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hmac::{Hmac, KeyInit, Mac};
use secrecy::{ExposeSecret, SecretString};
use sha2::Sha256;

/// The header that carries the signature.
pub const SIGNATURE_HEADER: &str = "Webhook-Signature";

/// The default maximum age (and clock skew) of a notification.
const DEFAULT_TOLERANCE: Duration = Duration::from_secs(5 * 60);

/// Verifies the signature of webhook notifications. Its `Debug` output
/// never contains the secret.
///
/// Verify against the **raw** request body, before parsing it, then parse
/// the body as a [`Video`](crate::Video):
///
/// ```
/// use cf_stream::{Video, WebhookVerifier};
///
/// fn handle(signature_header: &str, body: &[u8]) -> Result<Video, Box<dyn std::error::Error>> {
///     let verifier = WebhookVerifier::new("webhook-secret");
///     verifier.verify(signature_header, body)?;
///     Ok(serde_json::from_slice(body)?)
/// }
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub struct WebhookVerifier {
    secret: SecretString,
    tolerance: Duration,
}

impl fmt::Debug for WebhookVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebhookVerifier")
            .field("secret", &"[REDACTED]")
            .field("tolerance", &self.tolerance)
            .finish()
    }
}

/// Why a notification failed verification.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WebhookError {
    /// The signature header is missing a part or can't be parsed.
    #[error("malformed webhook signature header")]
    Malformed,
    /// The signature's time is outside the tolerance window (a stale or
    /// replayed notification, or a large clock skew).
    #[error("webhook signature time is outside the tolerance window")]
    Expired,
    /// The signature doesn't match the body and secret.
    #[error("webhook signature doesn't match")]
    Mismatch,
}

impl WebhookVerifier {
    /// A verifier for the secret returned by
    /// [`Webhooks::set`](crate::Webhooks::set), accepting notifications
    /// signed at most 5 minutes from now.
    ///
    /// ```
    /// let verifier = cf_stream::WebhookVerifier::new("webhook-secret");
    /// ```
    pub fn new(secret: impl Into<SecretString>) -> Self {
        Self {
            secret: secret.into(),
            tolerance: DEFAULT_TOLERANCE,
        }
    }

    /// Changes how far the signature time may be from now, in either
    /// direction.
    ///
    /// ```
    /// let verifier = cf_stream::WebhookVerifier::new("secret")
    ///     .tolerance(std::time::Duration::from_secs(60));
    /// ```
    pub fn tolerance(mut self, tolerance: Duration) -> Self {
        self.tolerance = tolerance;
        self
    }

    /// Verifies a notification: `header` is the value of the
    /// [`Webhook-Signature`](SIGNATURE_HEADER) header and `body` the raw
    /// request body.
    ///
    /// # Errors
    ///
    /// A [`WebhookError`] if the header is malformed, too old, or the
    /// signature doesn't match.
    ///
    /// ```
    /// let verifier = cf_stream::WebhookVerifier::new("secret");
    /// let err = verifier.verify("time=1,sig1=00", b"{}").unwrap_err();
    /// assert_eq!(err, cf_stream::WebhookError::Expired);
    /// ```
    pub fn verify(&self, header: &str, body: &[u8]) -> Result<(), WebhookError> {
        self.verify_at(header, body, SystemTime::now())
    }

    /// Like [`verify`](Self::verify), with an explicit current time.
    ///
    /// ```
    /// use std::time::{Duration, UNIX_EPOCH};
    ///
    /// let verifier = cf_stream::WebhookVerifier::new("secret");
    /// let now = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    /// let err = verifier.verify_at("time=1700000000,sig1=00", b"{}", now).unwrap_err();
    /// assert_eq!(err, cf_stream::WebhookError::Mismatch);
    /// ```
    pub fn verify_at(
        &self,
        header: &str,
        body: &[u8],
        now: SystemTime,
    ) -> Result<(), WebhookError> {
        let mut time = None;
        let mut sig = None;
        for part in header.split(',') {
            let Some((key, value)) = part.split_once('=') else {
                continue;
            };
            match key.trim() {
                "time" => time = Some(value.trim()),
                "sig1" => sig = Some(value.trim()),
                _ => {}
            }
        }
        let (Some(time), Some(sig)) = (time, sig) else {
            return Err(WebhookError::Malformed);
        };
        if time.is_empty() || !time.bytes().all(|b| b.is_ascii_digit()) {
            return Err(WebhookError::Malformed);
        }
        let signed_at: u64 = time.parse().map_err(|_| WebhookError::Malformed)?;
        let sig = hex::decode(sig).map_err(|_| WebhookError::Malformed)?;

        let now = now
            .duration_since(UNIX_EPOCH)
            .map_err(|_| WebhookError::Expired)?
            .as_secs();
        if now.abs_diff(signed_at) > self.tolerance.as_secs() {
            return Err(WebhookError::Expired);
        }

        // HMAC accepts keys of any length, so this doesn't fail in practice.
        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret.expose_secret().as_bytes())
            .map_err(|_| WebhookError::Mismatch)?;
        mac.update(time.as_bytes());
        mac.update(b".");
        mac.update(body);
        mac.verify_slice(&sig).map_err(|_| WebhookError::Mismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test vector computed independently of this crate with OpenSSL:
    //   printf '%s' '1230811200.{"uid":"abc"}' | openssl dgst -sha256 -hmac 'secret'
    const SECRET: &str = "secret";
    const BODY: &[u8] = br#"{"uid":"abc"}"#;
    const SIG: &str = "fbadfae27c910b6fbc5c026838f5c8fac150983bfa63cf14d8df4d52d6d7897a";

    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_230_811_200 + 60)
    }

    fn verify(header: &str, body: &[u8]) -> Result<(), WebhookError> {
        WebhookVerifier::new(SECRET).verify_at(header, body, now())
    }

    #[test]
    fn valid_signature() {
        verify(&format!("time=1230811200,sig1={SIG}"), BODY).unwrap();
        // Order, whitespace, extra keys and upper-case hex are tolerated.
        verify(
            &format!(" sig1={} , time=1230811200, v=2", SIG.to_uppercase()),
            BODY,
        )
        .unwrap();
    }

    #[test]
    fn tampered_body_or_wrong_secret() {
        let header = format!("time=1230811200,sig1={SIG}");
        assert_eq!(
            verify(&header, br#"{"uid":"abd"}"#),
            Err(WebhookError::Mismatch)
        );
        assert_eq!(
            WebhookVerifier::new("other").verify_at(&header, BODY, now()),
            Err(WebhookError::Mismatch)
        );
        // The time is part of the signed message.
        assert_eq!(
            verify(&format!("time=1230811201,sig1={SIG}"), BODY),
            Err(WebhookError::Mismatch)
        );
    }

    #[test]
    fn tolerance_window() {
        let header = format!("time=1230811200,sig1={SIG}");
        let verifier = WebhookVerifier::new(SECRET).tolerance(Duration::from_secs(30));
        assert_eq!(
            verifier.verify_at(&header, BODY, now()),
            Err(WebhookError::Expired)
        );
        // Signatures from the future are rejected too.
        let early = UNIX_EPOCH + Duration::from_secs(1_230_811_200 - 31);
        assert_eq!(
            verifier.verify_at(&header, BODY, early),
            Err(WebhookError::Expired)
        );
    }

    #[test]
    fn malformed_headers() {
        for header in [
            "",
            "time=1230811200",
            &format!("sig1={SIG}"),
            &format!("time=,sig1={SIG}"),
            &format!("time=-1,sig1={SIG}"),
            &format!("time=12.5,sig1={SIG}"),
            "time=1230811200,sig1=not-hex",
        ] {
            assert_eq!(
                verify(header, BODY),
                Err(WebhookError::Malformed),
                "{header}"
            );
        }
    }

    #[test]
    fn debug_hides_secret() {
        let debug = format!("{:?}", WebhookVerifier::new("super-secret-value"));
        assert!(!debug.contains("super-secret-value"), "{debug}");
    }
}
