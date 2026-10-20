//! Delivery URL builder (no HTTP).
//!
//! Images are served from `https://imagedelivery.net/{account_hash}/{image_id}/{variant}`.
//! The account hash is **not** the account ID: see [`AccountHash`].

use std::fmt;

use reqwest::Url;

use crate::error::{Error, Result};
use crate::types::{AccountHash, ImageId, VariantName};
use crate::variants::Fit;

/// The default delivery host.
pub const DEFAULT_DELIVERY_BASE: &str = "https://imagedelivery.net/";

/// Builds image delivery URLs for one account.
///
/// ```
/// use cf_images::{AccountHash, Delivery, FlexibleVariant};
///
/// let delivery = Delivery::new(AccountHash::new("ZWd9g1K7eljCn_KDTu_MWA"));
/// let url = delivery.url(&"083eb7b2-5392-4565-b69e-aff66acddd00".into(), &"public".into());
/// assert_eq!(
///     url.as_str(),
///     "https://imagedelivery.net/ZWd9g1K7eljCn_KDTu_MWA/083eb7b2-5392-4565-b69e-aff66acddd00/public"
/// );
///
/// let flexible = delivery.flexible_url(
///     &"083eb7b2-5392-4565-b69e-aff66acddd00".into(),
///     &FlexibleVariant::new().width(400).height(300),
/// );
/// assert!(flexible.as_str().ends_with("/w=400,h=300"));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Delivery {
    base: Url,
    account_hash: AccountHash,
}

impl Delivery {
    /// Delivery through `imagedelivery.net`.
    ///
    /// ```
    /// let delivery = cf_images::Delivery::new(cf_images::AccountHash::new("hash"));
    /// ```
    pub fn new(account_hash: AccountHash) -> Self {
        Self {
            base: Url::parse(DEFAULT_DELIVERY_BASE).expect("DEFAULT_DELIVERY_BASE is a valid URL"),
            account_hash,
        }
    }

    /// Delivery through one of your zones, as
    /// `https://{zone}/cdn-cgi/imagedelivery/{account_hash}/{image_id}/{variant}`.
    ///
    /// `zone` is a host name such as `example.com` or a full origin such as
    /// `https://images.example.com`.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if `zone` isn't a valid host or origin.
    ///
    /// ```
    /// use cf_images::{AccountHash, Delivery};
    ///
    /// # fn run() -> cf_images::Result<()> {
    /// let delivery = Delivery::custom_domain(AccountHash::new("hash"), "example.com")?;
    /// let url = delivery.url(&"img".into(), &"public".into());
    /// assert_eq!(url.as_str(), "https://example.com/cdn-cgi/imagedelivery/hash/img/public");
    /// # Ok(()) }
    /// ```
    pub fn custom_domain(account_hash: AccountHash, zone: &str) -> Result<Self> {
        let raw = if zone.contains("://") {
            zone.to_owned()
        } else {
            format!("https://{zone}")
        };
        let mut base = Url::parse(&raw)
            .map_err(|e| Error::InvalidInput(format!("invalid zone {zone:?}: {e}")))?;
        if base.cannot_be_a_base() || base.host_str().is_none() {
            return Err(Error::InvalidInput(format!("invalid zone {zone:?}")));
        }
        base.set_path("/cdn-cgi/imagedelivery/");
        base.set_query(None);
        base.set_fragment(None);
        Ok(Self { base, account_hash })
    }

    /// The account hash used in URLs.
    ///
    /// ```
    /// let delivery = cf_images::Delivery::new(cf_images::AccountHash::new("hash"));
    /// assert_eq!(delivery.account_hash(), "hash");
    /// ```
    pub fn account_hash(&self) -> &AccountHash {
        &self.account_hash
    }

    /// The URL of `image_id` in a named variant.
    ///
    /// ```
    /// use cf_images::{AccountHash, Delivery};
    ///
    /// let delivery = Delivery::new(AccountHash::new("hash"));
    /// // Custom IDs containing `/` keep their path structure.
    /// let url = delivery.url(&"products/42".into(), &"thumbnail".into());
    /// assert_eq!(url.as_str(), "https://imagedelivery.net/hash/products/42/thumbnail");
    /// ```
    pub fn url(&self, image_id: &ImageId, variant: &VariantName) -> Url {
        self.build(image_id, variant.as_str())
    }

    /// The URL of `image_id` resized on the fly with
    /// [flexible variant](https://developers.cloudflare.com/images/transform-images/flexible-variants/)
    /// options. Flexible variants must be enabled for the account, and don't
    /// work for images that require signed URLs unless the URL is signed.
    ///
    /// ```
    /// use cf_images::{AccountHash, Delivery, Fit, FlexibleVariant};
    ///
    /// let delivery = Delivery::new(AccountHash::new("hash"));
    /// let url = delivery.flexible_url(&"img".into(), &FlexibleVariant::new().width(200).fit(Fit::Cover));
    /// assert_eq!(url.as_str(), "https://imagedelivery.net/hash/img/w=200,fit=cover");
    /// ```
    pub fn flexible_url(&self, image_id: &ImageId, options: &FlexibleVariant) -> Url {
        self.build(image_id, &options.to_string())
    }

    fn build(&self, image_id: &ImageId, last: &str) -> Url {
        let mut url = self.base.clone();
        // `base` is always an http(s) URL, so it can be a base.
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty()
                .push(self.account_hash.as_str())
                .extend(image_id.as_str().split('/'))
                .push(last);
        }
        url
    }
}

/// Options for a flexible variant, rendered as comma-separated `key=value`
/// pairs in the order they were set (e.g. `w=400,h=300,fit=cover`).
///
/// ```
/// use cf_images::FlexibleVariant;
///
/// let v = FlexibleVariant::new().width(400).quality(80).option("sharpen", 1);
/// assert_eq!(v.to_string(), "w=400,q=80,sharpen=1");
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FlexibleVariant {
    options: Vec<(String, String)>,
}

impl FlexibleVariant {
    /// No options.
    ///
    /// ```
    /// assert!(cf_images::FlexibleVariant::new().is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets an arbitrary option (`key=value`), replacing an earlier value for
    /// the same key.
    ///
    /// ```
    /// let v = cf_images::FlexibleVariant::new().option("rotate", 90);
    /// assert_eq!(v.to_string(), "rotate=90");
    /// ```
    pub fn option(mut self, key: impl Into<String>, value: impl fmt::Display) -> Self {
        let key = key.into();
        let value = value.to_string();
        match self.options.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.options.push((key, value)),
        }
        self
    }

    /// Maximum width in pixels (`w`).
    ///
    /// ```
    /// assert_eq!(cf_images::FlexibleVariant::new().width(400).to_string(), "w=400");
    /// ```
    pub fn width(self, px: u32) -> Self {
        self.option("w", px)
    }

    /// Maximum height in pixels (`h`).
    ///
    /// ```
    /// assert_eq!(cf_images::FlexibleVariant::new().height(300).to_string(), "h=300");
    /// ```
    pub fn height(self, px: u32) -> Self {
        self.option("h", px)
    }

    /// Resizing mode (`fit`). [`Fit::Unknown`] is ignored.
    ///
    /// ```
    /// use cf_images::{Fit, FlexibleVariant};
    ///
    /// assert_eq!(FlexibleVariant::new().fit(Fit::ScaleDown).to_string(), "fit=scale-down");
    /// ```
    pub fn fit(self, fit: Fit) -> Self {
        match crate::types::wire_str(&fit) {
            Some(fit) => self.option("fit", fit),
            None => self,
        }
    }

    /// Quality from 1 to 100 (`q`).
    ///
    /// ```
    /// assert_eq!(cf_images::FlexibleVariant::new().quality(75).to_string(), "q=75");
    /// ```
    pub fn quality(self, quality: u8) -> Self {
        self.option("q", quality)
    }

    /// Output format (`f`), e.g. `auto`, `avif`, `webp`, `jpeg`.
    ///
    /// ```
    /// assert_eq!(cf_images::FlexibleVariant::new().format("avif").to_string(), "f=avif");
    /// ```
    pub fn format(self, format: impl fmt::Display) -> Self {
        self.option("f", format)
    }

    /// Device pixel ratio (`dpr`).
    ///
    /// ```
    /// assert_eq!(cf_images::FlexibleVariant::new().dpr(2.0).to_string(), "dpr=2");
    /// ```
    pub fn dpr(self, dpr: f32) -> Self {
        self.option("dpr", dpr)
    }

    /// Crop gravity (`gravity`), e.g. `auto`, `left`, or `0.5x0.5`.
    ///
    /// ```
    /// assert_eq!(cf_images::FlexibleVariant::new().gravity("auto").to_string(), "gravity=auto");
    /// ```
    pub fn gravity(self, gravity: impl fmt::Display) -> Self {
        self.option("gravity", gravity)
    }

    /// Blur radius from 1 to 250 (`blur`).
    ///
    /// ```
    /// assert_eq!(cf_images::FlexibleVariant::new().blur(20).to_string(), "blur=20");
    /// ```
    pub fn blur(self, radius: u8) -> Self {
        self.option("blur", radius)
    }

    /// `true` if no options are set.
    ///
    /// ```
    /// assert!(!cf_images::FlexibleVariant::new().width(1).is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.options.is_empty()
    }
}

impl fmt::Display for FlexibleVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (k, v)) in self.options.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{k}={v}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_replaces_existing_key() {
        let v = FlexibleVariant::new().width(1).height(2).width(3);
        assert_eq!(v.to_string(), "w=3,h=2");
    }

    #[test]
    fn segments_are_encoded() {
        let d = Delivery::new(AccountHash::new("h"));
        let url = d.url(&"a b/c?d".into(), &"public".into());
        assert_eq!(
            url.as_str(),
            "https://imagedelivery.net/h/a%20b/c%3Fd/public"
        );
    }

    #[test]
    fn custom_domain_with_scheme() {
        let d = Delivery::custom_domain(AccountHash::new("h"), "http://localhost:8080/ignored")
            .unwrap();
        assert_eq!(
            d.url(&"i".into(), &"v".into()).as_str(),
            "http://localhost:8080/cdn-cgi/imagedelivery/h/i/v"
        );
        assert!(Delivery::custom_domain(AccountHash::new("h"), "").is_err());
    }
}
