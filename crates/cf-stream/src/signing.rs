//! Offline signed playback tokens (feature `signing`).
//!
//! Implements Cloudflare's
//! [self-signed token](https://developers.cloudflare.com/stream/viewing-videos/securing-your-stream/)
//! format: an RS256 JSON Web Token whose header carries the signing key's ID
//! (`kid`) and whose claims name the video (`sub`), the expiry (`exp`) and
//! optional restrictions. The token replaces the video ID in delivery URLs.

use std::fmt;
use std::time::{Duration, SystemTime};

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD as BASE64URL};
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Map, json};

use crate::access::AccessRule;
use crate::error::{Error, Result};
use crate::keys::SigningKey;
use crate::types::SigningKeyId;

/// Signs playback tokens locally with a Stream signing key, without an API
/// call per token. Its `Debug` output never contains the key.
///
/// Create a key once with [`SigningKeys::create`](crate::SigningKeys::create)
/// and store its ID and PEM; Cloudflare doesn't return the key material
/// again.
///
/// ```no_run
/// use std::time::Duration;
/// use cf_stream::{CustomerCode, Delivery, TokenSigner};
///
/// # fn run() -> cf_stream::Result<()> {
/// let signer = TokenSigner::new(
///     std::env::var("STREAM_KEY_ID").expect("key ID"),
///     std::env::var("STREAM_KEY_PEM").expect("base64 PEM"),
/// )?;
/// let token = signer
///     .token("ea95132c15732412d22c1476fa83f27a")
///     .expires_in(Duration::from_secs(3600))
///     .sign()?;
///
/// let delivery = Delivery::new(CustomerCode::new("m033z5x00ks6nunl"))?;
/// println!("{}", delivery.hls(&token));
/// # Ok(()) }
/// ```
#[non_exhaustive]
pub struct TokenSigner {
    key_id: SigningKeyId,
    key: RsaKeyPair,
    rng: SystemRandom,
}

impl fmt::Debug for TokenSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenSigner")
            .field("key_id", &self.key_id)
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl TokenSigner {
    /// A signer for the key `key_id` with private key `pem`.
    ///
    /// `pem` may be the base64-encoded PEM that
    /// [`SigningKeys::create`](crate::SigningKeys::create) returns, or a
    /// decoded PEM (`-----BEGIN RSA PRIVATE KEY-----` or
    /// `-----BEGIN PRIVATE KEY-----`).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the key ID is empty or the key can't be
    /// parsed as an RSA private key of at least 2048 bits. The message never
    /// contains key material.
    ///
    /// ```
    /// let err = cf_stream::TokenSigner::new("key-id", "not a key").unwrap_err();
    /// assert!(matches!(err, cf_stream::Error::InvalidInput(_)));
    /// ```
    pub fn new(key_id: impl Into<SigningKeyId>, pem: impl Into<SecretString>) -> Result<Self> {
        let key_id = key_id.into();
        crate::types::require_non_empty("signing key ID", key_id.as_str())?;
        let pem: SecretString = pem.into();
        let key = parse_private_key(pem.expose_secret())?;
        Ok(Self {
            key_id,
            key,
            rng: SystemRandom::new(),
        })
    }

    /// A signer for a key just returned by
    /// [`SigningKeys::create`](crate::SigningKeys::create).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the key has no ID or PEM, or the PEM is
    /// invalid.
    ///
    /// ```no_run
    /// # async fn run(client: cf_stream::Client) -> cf_stream::Result<()> {
    /// let key = client.signing_keys().create().await?;
    /// let signer = cf_stream::TokenSigner::from_signing_key(&key)?;
    /// # Ok(()) }
    /// ```
    pub fn from_signing_key(key: &SigningKey) -> Result<Self> {
        let id = key
            .id
            .clone()
            .ok_or_else(|| Error::InvalidInput("signing key has no ID".into()))?;
        let pem = key
            .pem
            .clone()
            .ok_or_else(|| Error::InvalidInput("signing key has no PEM".into()))?;
        Self::new(id, pem)
    }

    /// The ID of the signing key.
    ///
    /// ```no_run
    /// # fn run(signer: cf_stream::TokenSigner) {
    /// println!("{}", signer.key_id());
    /// # }
    /// ```
    pub fn key_id(&self) -> &SigningKeyId {
        &self.key_id
    }

    /// Starts a token for `subject`: a video ID, or a live input ID for a
    /// live stream. Configure it with the returned builder and call
    /// [`sign`](TokenBuilder::sign).
    ///
    /// ```no_run
    /// # fn run(signer: cf_stream::TokenSigner) -> cf_stream::Result<()> {
    /// let token = signer.token("ea95132c15732412d22c1476fa83f27a").sign()?;
    /// # Ok(()) }
    /// ```
    pub fn token(&self, subject: impl Into<String>) -> TokenBuilder<'_> {
        TokenBuilder {
            signer: self,
            subject: subject.into(),
            expires_at: None,
            not_before: None,
            downloadable: None,
            access_rules: Vec::new(),
        }
    }

    fn sign_jwt(&self, claims: &serde_json::Value) -> Result<String> {
        let header = json!({ "alg": "RS256", "kid": self.key_id, "typ": "JWT" });
        let encode = |value: &serde_json::Value| BASE64URL.encode(value.to_string());
        let signing_input = format!("{}.{}", encode(&header), encode(claims));
        let mut signature = vec![0; self.key.public().modulus_len()];
        self.key
            .sign(
                &RSA_PKCS1_SHA256,
                &self.rng,
                signing_input.as_bytes(),
                &mut signature,
            )
            .map_err(|_| Error::InvalidInput("failed to sign token".into()))?;
        Ok(format!("{signing_input}.{}", BASE64URL.encode(signature)))
    }
}

/// The default token lifetime, matching Cloudflare's API default.
const DEFAULT_TTL: Duration = Duration::from_secs(3600);

/// A token being configured. Created by [`TokenSigner::token`].
#[derive(Clone, Debug)]
#[must_use = "a token builder does nothing until you call `sign`"]
#[non_exhaustive]
pub struct TokenBuilder<'a> {
    signer: &'a TokenSigner,
    subject: String,
    expires_at: Option<SystemTime>,
    not_before: Option<SystemTime>,
    downloadable: Option<bool>,
    access_rules: Vec<AccessRule>,
}

impl TokenBuilder<'_> {
    /// When the token expires. The default is one hour from signing.
    ///
    /// ```no_run
    /// # fn run(signer: cf_stream::TokenSigner) -> cf_stream::Result<()> {
    /// use std::time::{Duration, SystemTime};
    ///
    /// let token = signer
    ///     .token("ea95132c15732412d22c1476fa83f27a")
    ///     .expires_at(SystemTime::now() + Duration::from_secs(600))
    ///     .sign()?;
    /// # Ok(()) }
    /// ```
    pub fn expires_at(mut self, at: SystemTime) -> Self {
        self.expires_at = Some(at);
        self
    }

    /// Sets the expiry relative to now. See [`expires_at`](Self::expires_at).
    ///
    /// ```no_run
    /// # fn run(signer: cf_stream::TokenSigner) -> cf_stream::Result<()> {
    /// let token = signer
    ///     .token("ea95132c15732412d22c1476fa83f27a")
    ///     .expires_in(std::time::Duration::from_secs(600))
    ///     .sign()?;
    /// # Ok(()) }
    /// ```
    pub fn expires_in(self, ttl: Duration) -> Self {
        self.expires_at(SystemTime::now() + ttl)
    }

    /// The token isn't valid before this time.
    ///
    /// ```no_run
    /// # fn run(signer: cf_stream::TokenSigner) -> cf_stream::Result<()> {
    /// let token = signer
    ///     .token("ea95132c15732412d22c1476fa83f27a")
    ///     .not_before(std::time::SystemTime::now())
    ///     .sign()?;
    /// # Ok(()) }
    /// ```
    pub fn not_before(mut self, at: SystemTime) -> Self {
        self.not_before = Some(at);
        self
    }

    /// Allows the token to be used for MP4 downloads.
    ///
    /// ```no_run
    /// # fn run(signer: cf_stream::TokenSigner) -> cf_stream::Result<()> {
    /// let token = signer.token("ea95132c15732412d22c1476fa83f27a").downloadable(true).sign()?;
    /// # Ok(()) }
    /// ```
    pub fn downloadable(mut self, downloadable: bool) -> Self {
        self.downloadable = Some(downloadable);
        self
    }

    /// Adds an access rule (at most 5). See [`AccessRule`].
    ///
    /// ```no_run
    /// # fn run(signer: cf_stream::TokenSigner) -> cf_stream::Result<()> {
    /// use cf_stream::{AccessAction, AccessRule};
    ///
    /// let token = signer
    ///     .token("ea95132c15732412d22c1476fa83f27a")
    ///     .access_rule(AccessRule::country(AccessAction::Block, ["XX"]))
    ///     .sign()?;
    /// # Ok(()) }
    /// ```
    pub fn access_rule(mut self, rule: AccessRule) -> Self {
        self.access_rules.push(rule);
        self
    }

    /// Signs the token.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the subject is empty, a time is before
    /// 1970, the token would expire before it becomes valid, or the access
    /// rules are invalid.
    ///
    /// ```no_run
    /// # fn run(signer: cf_stream::TokenSigner) -> cf_stream::Result<()> {
    /// let token = signer.token("ea95132c15732412d22c1476fa83f27a").sign()?;
    /// # Ok(()) }
    /// ```
    pub fn sign(&self) -> Result<String> {
        self.sign_at(SystemTime::now())
    }

    fn sign_at(&self, now: SystemTime) -> Result<String> {
        crate::types::require_non_empty("token subject", &self.subject)?;
        let exp = crate::util::unix_secs(self.expires_at.unwrap_or(now + DEFAULT_TTL), "expiry")?;
        let mut claims = Map::new();
        claims.insert("sub".into(), self.subject.clone().into());
        claims.insert("kid".into(), self.signer.key_id.as_str().into());
        claims.insert("exp".into(), exp.into());
        if let Some(at) = self.not_before {
            let nbf = crate::util::unix_secs(at, "not-before time")?;
            if nbf >= exp {
                return Err(Error::InvalidInput(
                    "token would expire before it becomes valid".into(),
                ));
            }
            claims.insert("nbf".into(), nbf.into());
        }
        if let Some(downloadable) = self.downloadable {
            claims.insert("downloadable".into(), downloadable.into());
        }
        if !self.access_rules.is_empty() {
            claims.insert(
                "accessRules".into(),
                crate::access::rules_json(&self.access_rules)?,
            );
        }
        self.signer.sign_jwt(&claims.into())
    }
}

/// Parses an RSA private key from a (possibly base64-wrapped) PEM.
fn parse_private_key(input: &str) -> Result<RsaKeyPair> {
    let invalid = |why: &str| Error::InvalidInput(format!("invalid signing key PEM: {why}"));
    let input = input.trim();
    let decoded;
    let pem = if input.starts_with("-----BEGIN") {
        input
    } else {
        let bytes = BASE64
            .decode(strip_whitespace(input))
            .map_err(|_| invalid("not a PEM and not base64"))?;
        decoded = String::from_utf8(bytes).map_err(|_| invalid("base64 doesn't decode to text"))?;
        decoded.trim()
    };

    let (label, der) = pem_body(pem).ok_or_else(|| invalid("malformed PEM"))?;
    let der = BASE64
        .decode(strip_whitespace(der))
        .map_err(|_| invalid("malformed PEM body"))?;
    let key = match label {
        "RSA PRIVATE KEY" => RsaKeyPair::from_der(&der),
        "PRIVATE KEY" => RsaKeyPair::from_pkcs8(&der),
        _ => return Err(invalid("expected an RSA PRIVATE KEY or PRIVATE KEY block")),
    };
    key.map_err(|e| invalid(&e.to_string()))
}

/// Splits `-----BEGIN {label}-----{body}-----END {label}-----`.
fn pem_body(pem: &str) -> Option<(&str, &str)> {
    let rest = pem.strip_prefix("-----BEGIN ")?;
    let (label, rest) = rest.split_once("-----")?;
    let end = format!("-----END {label}-----");
    let body = rest.trim_end().strip_suffix(end.as_str())?;
    Some((label, body))
}

fn strip_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_ascii_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::AccessAction;
    use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
    use std::time::UNIX_EPOCH;

    // A throwaway 2048-bit key generated for these tests only, with
    // `openssl genrsa -traditional 2048` (PKCS#1) and converted with
    // `openssl pkcs8 -topk8 -nocrypt` (PKCS#8). It protects nothing. The
    // fixtures are base64-wrapped, the way the API returns keys, so the
    // repository holds no raw PEM blocks for secret scanners to flag.
    const PKCS1_B64: &str = include_str!("../tests/fixtures/test_rsa_pkcs1.pem.b64");
    const PKCS8_B64: &str = include_str!("../tests/fixtures/test_rsa_pkcs8.pem.b64");

    fn pem(b64: &str) -> String {
        String::from_utf8(BASE64.decode(b64.trim()).unwrap()).unwrap()
    }

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn decode_part(part: &str) -> serde_json::Value {
        serde_json::from_slice(&BASE64URL.decode(part).unwrap()).unwrap()
    }

    fn verify(signer: &TokenSigner, token: &str) {
        let (input, sig) = token.rsplit_once('.').unwrap();
        let public = UnparsedPublicKey::new(
            &RSA_PKCS1_2048_8192_SHA256,
            signer.key.public().as_ref().to_vec(),
        );
        public
            .verify(input.as_bytes(), &BASE64URL.decode(sig).unwrap())
            .expect("signature verifies with the public key");
    }

    #[test]
    fn signs_verifiable_rs256_tokens() {
        let signer = TokenSigner::new("8f926b2b01f383510025a78a4dcbf6a", PKCS1_B64).unwrap();
        let token = signer
            .token("ea95132c15732412d22c1476fa83f27a")
            .expires_at(at(1_700_003_600))
            .not_before(at(1_700_000_000))
            .downloadable(true)
            .access_rule(AccessRule::country(AccessAction::Allow, ["US"]))
            .access_rule(AccessRule::any(AccessAction::Block))
            .sign()
            .unwrap();

        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(!token.contains('='), "base64url without padding");
        assert_eq!(
            decode_part(parts[0]),
            json!({"alg": "RS256", "kid": "8f926b2b01f383510025a78a4dcbf6a", "typ": "JWT"})
        );
        assert_eq!(
            decode_part(parts[1]),
            json!({
                "sub": "ea95132c15732412d22c1476fa83f27a",
                "kid": "8f926b2b01f383510025a78a4dcbf6a",
                "exp": 1_700_003_600u64,
                "nbf": 1_700_000_000u64,
                "downloadable": true,
                "accessRules": [
                    {"type": "ip.geoip.country", "action": "allow", "country": ["US"]},
                    {"type": "any", "action": "block"},
                ],
            })
        );
        verify(&signer, &token);
    }

    #[test]
    fn default_expiry_is_one_hour() {
        let signer = TokenSigner::new("kid", PKCS1_B64).unwrap();
        let token = signer.token("uid").sign_at(at(1_000)).unwrap();
        let claims = decode_part(token.split('.').nth(1).unwrap());
        assert_eq!(claims["exp"], 4_600);
        assert!(claims.get("nbf").is_none());
    }

    #[test]
    fn accepts_pkcs8_and_base64_wrapped_pem() {
        for key in [
            pem(PKCS1_B64),
            pem(PKCS8_B64),
            PKCS1_B64.to_owned(),
            PKCS8_B64.to_owned(),
        ] {
            let signer = TokenSigner::new("kid", key).unwrap();
            verify(&signer, &signer.token("uid").sign().unwrap());
        }
    }

    #[test]
    fn rejects_bad_input_without_leaking_it() {
        let full = pem(PKCS1_B64);
        let truncated = &full[..full.len() / 2];
        for key in [
            "",
            "not a key",
            "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----",
            truncated,
        ] {
            let err = TokenSigner::new("kid", key).unwrap_err();
            let msg = err.to_string();
            assert!(matches!(err, Error::InvalidInput(_)), "{msg}");
            assert!(!msg.contains("MII"), "message leaks key material: {msg}");
        }
        assert!(TokenSigner::new("", PKCS1_B64).is_err());
    }

    #[test]
    fn rejects_invalid_claims() {
        let signer = TokenSigner::new("kid", PKCS1_B64).unwrap();
        assert!(signer.token("").sign().is_err());
        assert!(
            signer
                .token("uid")
                .expires_at(at(10))
                .not_before(at(10))
                .sign()
                .is_err()
        );
        let mut six = signer.token("uid");
        for _ in 0..6 {
            six = six.access_rule(AccessRule::any(AccessAction::Allow));
        }
        assert!(six.sign().is_err());
    }

    #[test]
    fn debug_hides_key() {
        let signer = TokenSigner::new("kid", PKCS1_B64).unwrap();
        let debug = format!("{signer:?} {:?}", signer.token("uid"));
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("MII"), "{debug}");
    }
}
