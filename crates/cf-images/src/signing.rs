//! Signed delivery URLs for private images (feature `signing`).
//!
//! Implements the algorithm from Cloudflare's
//! [serve private images](https://developers.cloudflare.com/images/manage-images/serve-images/serve-private-images/)
//! guide:
//!
//! 1. set the `exp` query parameter to the expiry as Unix seconds;
//! 2. compute HMAC-SHA256, keyed with the signing key's UTF-8 bytes, over
//!    `path + "?" + query` (form-urlencoded, as `URLSearchParams.toString()`
//!    produces it);
//! 3. append the lowercase hex signature as the `sig` query parameter.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hmac::{Hmac, KeyInit, Mac};
use reqwest::Url;
use secrecy::{ExposeSecret, SecretString};
use sha2::Sha256;

use crate::error::{Error, Result};

/// Signs delivery URLs with an Images signing key. Its `Debug` output never
/// contains the key.
///
/// ```
/// use std::time::{Duration, UNIX_EPOCH};
/// use cf_images::{AccountHash, Delivery, UrlSigner};
///
/// # fn run() -> cf_images::Result<()> {
/// let delivery = Delivery::new(AccountHash::new("hash"));
/// let url = delivery.url(&"image-id".into(), &"public".into());
///
/// let signer = UrlSigner::new("signing-key-value");
/// let signed = signer.sign(&url, UNIX_EPOCH + Duration::from_secs(1_700_000_000))?;
/// assert!(signed.as_str().starts_with("https://imagedelivery.net/hash/image-id/public?exp=1700000000&sig="));
/// # Ok(()) }
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub struct UrlSigner {
    key: SecretString,
}

impl fmt::Debug for UrlSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UrlSigner")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl UrlSigner {
    /// A signer using a signing key value, as returned by
    /// [`SigningKeys::list`](crate::SigningKeys::list) or shown under
    /// **Images > Keys** in the dashboard. The value is used as-is (its UTF-8
    /// bytes are the HMAC key); it isn't base64- or hex-decoded.
    ///
    /// ```
    /// let signer = cf_images::UrlSigner::new("signing-key-value");
    /// ```
    pub fn new(key: impl Into<SecretString>) -> Self {
        Self { key: key.into() }
    }

    /// Returns `url` signed to expire at `expires_at` (rounded down to the
    /// second).
    ///
    /// Existing `exp` and `sig` parameters are replaced; other query
    /// parameters are kept and covered by the signature.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if `expires_at` is before the Unix epoch.
    ///
    /// ```
    /// use std::time::{Duration, SystemTime};
    ///
    /// # fn run() -> cf_images::Result<()> {
    /// let url = "https://imagedelivery.net/hash/id/public".parse().expect("valid URL");
    /// let signer = cf_images::UrlSigner::new("key");
    /// let signed = signer.sign(&url, SystemTime::now() + Duration::from_secs(3600))?;
    /// # Ok(()) }
    /// ```
    pub fn sign(&self, url: &Url, expires_at: SystemTime) -> Result<Url> {
        let exp = expires_at
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::InvalidInput("expiry is before 1970".into()))?
            .as_secs();

        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| k != "exp" && k != "sig")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

        let mut signed = url.clone();
        signed.set_fragment(None);
        signed
            .query_pairs_mut()
            .clear()
            .extend_pairs(&pairs)
            .append_pair("exp", &exp.to_string());

        let string_to_sign = format!("{}?{}", signed.path(), signed.query().unwrap_or_default());
        let sig = self.hmac_hex(string_to_sign.as_bytes())?;
        signed.query_pairs_mut().append_pair("sig", &sig);
        Ok(signed)
    }

    /// Returns `url` signed to expire `ttl` from now.
    ///
    /// ```
    /// # fn run() -> cf_images::Result<()> {
    /// let url = "https://imagedelivery.net/hash/id/public".parse().expect("valid URL");
    /// let signer = cf_images::UrlSigner::new("key");
    /// let signed = signer.sign_for(&url, std::time::Duration::from_secs(24 * 3600))?;
    /// # Ok(()) }
    /// ```
    pub fn sign_for(&self, url: &Url, ttl: Duration) -> Result<Url> {
        self.sign(url, SystemTime::now() + ttl)
    }

    fn hmac_hex(&self, message: &[u8]) -> Result<String> {
        // HMAC accepts keys of any length, so this doesn't fail in practice.
        let mut mac = Hmac::<Sha256>::new_from_slice(self.key.expose_secret().as_bytes())
            .map_err(|e| Error::InvalidInput(format!("invalid signing key: {e}")))?;
        mac.update(message);
        Ok(hex::encode(mac.finalize().into_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    // Test vectors.
    //
    // Each expected signature was computed independently of this crate in
    // two ways, which agreed:
    //
    // 1. With OpenSSL, over the exact string to sign, e.g.
    //      printf '%s' '/ZWd9g1K7eljCn_KDTu_MWA/083eb7b2-5392-4565-b69e-aff66acddd00/public?exp=1700000000' \
    //        | openssl dgst -sha256 -hmac 'Oix0bbNaT8Rge9PuyxUBrjI6zrgnsyJ5='
    // 2. By running the `generateSignedUrl` logic from Cloudflare's "Serve
    //    private images" Worker sample in Node.js (WHATWG `URL`,
    //    `searchParams.set("exp", ...)`, `crypto.subtle` HMAC-SHA256 over
    //    `pathname + "?" + searchParams.toString()`, hex-encoded), with
    //    `Date.now()` replaced by the fixed expiry.
    //
    // The key is the example key value from Cloudflare's OpenAPI schema.
    const KEY: &str = "Oix0bbNaT8Rge9PuyxUBrjI6zrgnsyJ5=";

    #[test]
    fn vector_plain_url() {
        let url: Url =
            "https://imagedelivery.net/ZWd9g1K7eljCn_KDTu_MWA/083eb7b2-5392-4565-b69e-aff66acddd00/public"
                .parse()
                .unwrap();
        let signed = UrlSigner::new(KEY).sign(&url, at(1_700_000_000)).unwrap();
        assert_eq!(
            signed.as_str(),
            "https://imagedelivery.net/ZWd9g1K7eljCn_KDTu_MWA/083eb7b2-5392-4565-b69e-aff66acddd00/public\
             ?exp=1700000000&sig=0fbcc308f651b016c963d2aa9a29783a704eefbe86dce068decacbf4950b4feb"
        );
    }

    #[test]
    fn vector_flexible_variant_with_query() {
        // Existing query parameters are signed too; a stale `exp`/`sig` is replaced.
        let url: Url =
            "https://imagedelivery.net/hash/products/42/w=400,h=300?a=b%20c&exp=1&sig=old"
                .parse()
                .unwrap();
        let signed = UrlSigner::new(KEY).sign(&url, at(1_893_456_000)).unwrap();
        assert_eq!(
            signed.as_str(),
            "https://imagedelivery.net/hash/products/42/w=400,h=300?a=b+c&exp=1893456000&sig=0197ded95611b3910f71c3c68e25a9aa3905cf6f598202f9a25960909635e791"
        );
    }

    #[test]
    fn rejects_pre_epoch_expiry() {
        let url: Url = "https://imagedelivery.net/h/i/v".parse().unwrap();
        let before = UNIX_EPOCH - Duration::from_secs(1);
        assert!(UrlSigner::new(KEY).sign(&url, before).is_err());
    }

    #[test]
    fn debug_hides_key() {
        assert!(!format!("{:?}", UrlSigner::new(KEY)).contains(KEY));
    }
}
