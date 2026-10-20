//! Variant endpoints. A variant is a named set of resizing options, such as
//! `thumbnail` or `hero`, applied when an image is delivered.

use std::collections::BTreeMap;

use reqwest::Method;
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::{Error, Result};
use crate::transport::ApiRequest;
use crate::types::VariantName;

/// Variant endpoints. Created by [`Client::variants`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Variants<'a> {
    client: &'a Client,
}

/// How `width` and `height` are interpreted when resizing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Fit {
    /// Shrink to fit within the box, never enlarge.
    ScaleDown,
    /// Resize to fit within the box, preserving aspect ratio.
    Contain,
    /// Resize to fill the box, cropping if needed.
    Cover,
    /// Shrink and crop to fill the box, never enlarge.
    Crop,
    /// Fit within the box and pad the remaining space.
    Pad,
    /// A value this version of the crate doesn't recognize. Can't be sent.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// Which EXIF metadata to keep in delivered images.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum MetadataBehavior {
    /// Keep all metadata.
    Keep,
    /// Keep only copyright information.
    Copyright,
    /// Strip all metadata.
    None,
    /// A value this version of the crate doesn't recognize. Can't be sent.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// Resizing options of a variant.
///
/// ```
/// use cf_images::{Fit, MetadataBehavior, VariantOptions};
///
/// let options = VariantOptions::new(Fit::Cover, 300, 300).metadata(MetadataBehavior::Keep);
/// assert_eq!(options.width, 300);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct VariantOptions {
    /// How the dimensions are interpreted.
    pub fit: Fit,
    /// Maximum width in pixels (at least 1).
    pub width: u32,
    /// Maximum height in pixels (at least 1).
    pub height: u32,
    /// EXIF metadata handling.
    pub metadata: MetadataBehavior,
}

impl VariantOptions {
    /// Options with the given fit and box; metadata defaults to
    /// [`MetadataBehavior::None`] (strip everything).
    ///
    /// ```
    /// let options = cf_images::VariantOptions::new(cf_images::Fit::ScaleDown, 1366, 768);
    /// ```
    pub fn new(fit: Fit, width: u32, height: u32) -> Self {
        Self {
            fit,
            width,
            height,
            metadata: MetadataBehavior::None,
        }
    }

    /// Sets the EXIF metadata handling.
    ///
    /// ```
    /// use cf_images::{Fit, MetadataBehavior, VariantOptions};
    ///
    /// let options = VariantOptions::new(Fit::Crop, 100, 100).metadata(MetadataBehavior::Copyright);
    /// ```
    pub fn metadata(mut self, metadata: MetadataBehavior) -> Self {
        self.metadata = metadata;
        self
    }
}

/// A variant definition.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub struct Variant {
    /// The variant name, used in delivery URLs.
    pub id: VariantName,
    /// Resizing options.
    pub options: VariantOptions,
    /// If `true`, this variant serves images without a signature even when
    /// the image requires signed URLs.
    #[serde(rename = "neverRequireSignedURLs")]
    pub never_require_signed_urls: Option<bool>,
}

/// A variant to create with [`Variants::create`].
///
/// ```
/// use cf_images::{Fit, NewVariant, VariantOptions};
///
/// let variant = NewVariant::new("thumbnail", VariantOptions::new(Fit::Cover, 200, 200))
///     .never_require_signed_urls(true);
/// ```
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct NewVariant {
    id: VariantName,
    options: VariantOptions,
    #[serde(
        rename = "neverRequireSignedURLs",
        skip_serializing_if = "Option::is_none"
    )]
    never_require_signed_urls: Option<bool>,
}

impl NewVariant {
    /// A variant named `id` (alphanumeric, at most 99 characters).
    ///
    /// ```
    /// use cf_images::{Fit, NewVariant, VariantOptions};
    ///
    /// let v = NewVariant::new("hero", VariantOptions::new(Fit::Cover, 1920, 1080));
    /// ```
    pub fn new(id: impl Into<VariantName>, options: VariantOptions) -> Self {
        Self {
            id: id.into(),
            options,
            never_require_signed_urls: None,
        }
    }

    /// Lets this variant serve private images without a signature.
    ///
    /// ```
    /// use cf_images::{Fit, NewVariant, VariantOptions};
    ///
    /// let v = NewVariant::new("og", VariantOptions::new(Fit::Cover, 1200, 630))
    ///     .never_require_signed_urls(true);
    /// ```
    pub fn never_require_signed_urls(mut self, value: bool) -> Self {
        self.never_require_signed_urls = Some(value);
        self
    }
}

/// Changes to a variant, for [`Variants::update`]. The API requires the full
/// set of options on every update.
///
/// ```
/// use cf_images::{Fit, UpdateVariant, VariantOptions};
///
/// let update = UpdateVariant::new(VariantOptions::new(Fit::ScaleDown, 1366, 768));
/// ```
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct UpdateVariant {
    options: VariantOptions,
    #[serde(
        rename = "neverRequireSignedURLs",
        skip_serializing_if = "Option::is_none"
    )]
    never_require_signed_urls: Option<bool>,
}

impl UpdateVariant {
    /// An update setting the variant's options.
    ///
    /// ```
    /// use cf_images::{Fit, UpdateVariant, VariantOptions};
    ///
    /// let update = UpdateVariant::new(VariantOptions::new(Fit::Pad, 500, 500));
    /// ```
    pub fn new(options: VariantOptions) -> Self {
        Self {
            options,
            never_require_signed_urls: None,
        }
    }

    /// Changes whether this variant serves private images without a signature.
    ///
    /// ```
    /// use cf_images::{Fit, UpdateVariant, VariantOptions};
    ///
    /// let update = UpdateVariant::new(VariantOptions::new(Fit::Pad, 500, 500))
    ///     .never_require_signed_urls(false);
    /// ```
    pub fn never_require_signed_urls(mut self, value: bool) -> Self {
        self.never_require_signed_urls = Some(value);
        self
    }
}

#[derive(Deserialize)]
struct VariantEnvelope {
    variant: Variant,
}

#[derive(Deserialize)]
struct VariantList {
    #[serde(default)]
    variants: BTreeMap<String, Variant>,
}

fn to_json<T: Serialize>(value: &T) -> Result<serde_json::Value> {
    serde_json::to_value(value).map_err(|e| Error::InvalidInput(e.to_string()))
}

impl<'a> Variants<'a> {
    pub(crate) fn new(client: &'a Client) -> Self {
        Self { client }
    }

    /// Lists all variants, keyed by variant name.
    ///
    /// `GET /accounts/{account_id}/images/v1/variants`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// for (name, variant) in client.variants().list().await? {
    ///     println!("{name}: {}x{}", variant.options.width, variant.options.height);
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn list(&self) -> Result<BTreeMap<String, Variant>> {
        let list: VariantList = self
            .client
            .inner
            .json(ApiRequest::get(["v1", "variants"]))
            .await?;
        Ok(list.variants)
    }

    /// Fetches one variant.
    ///
    /// `GET /accounts/{account_id}/images/v1/variants/{variant_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let variant = client.variants().get(&"public".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn get(&self, id: &VariantName) -> Result<Variant> {
        let request = ApiRequest::get(["v1", "variants", id.as_str()]);
        let envelope: VariantEnvelope = self.client.inner.json(request).await?;
        Ok(envelope.variant)
    }

    /// Creates a variant.
    ///
    /// `POST /accounts/{account_id}/images/v1/variants`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// use cf_images::{Fit, NewVariant, VariantOptions};
    ///
    /// let options = VariantOptions::new(Fit::Cover, 200, 200);
    /// let variant = client.variants().create(NewVariant::new("thumb", options)).await?;
    /// # Ok(()) }
    /// ```
    pub async fn create(&self, variant: NewVariant) -> Result<Variant> {
        crate::types::require_non_empty("variant name", variant.id.as_str())?;
        let request = ApiRequest::new(Method::POST, ["v1", "variants"]).json(to_json(&variant)?);
        let envelope: VariantEnvelope = self.client.inner.json(request).await?;
        Ok(envelope.variant)
    }

    /// Updates a variant.
    ///
    /// `PATCH /accounts/{account_id}/images/v1/variants/{variant_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// use cf_images::{Fit, UpdateVariant, VariantOptions};
    ///
    /// let update = UpdateVariant::new(VariantOptions::new(Fit::Contain, 400, 400));
    /// let variant = client.variants().update(&"thumb".into(), update).await?;
    /// # Ok(()) }
    /// ```
    pub async fn update(&self, id: &VariantName, update: UpdateVariant) -> Result<Variant> {
        let request =
            ApiRequest::new(Method::PATCH, ["v1", "variants", id.as_str()]).json(to_json(&update)?);
        let envelope: VariantEnvelope = self.client.inner.json(request).await?;
        Ok(envelope.variant)
    }

    /// Deletes a variant.
    ///
    /// `DELETE /accounts/{account_id}/images/v1/variants/{variant_id}`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// client.variants().delete(&"thumb".into()).await?;
    /// # Ok(()) }
    /// ```
    pub async fn delete(&self, id: &VariantName) -> Result<()> {
        let request = ApiRequest::new(Method::DELETE, ["v1", "variants", id.as_str()]);
        self.client
            .inner
            .json::<IgnoredAny>(request)
            .await
            .map(drop)
    }
}
