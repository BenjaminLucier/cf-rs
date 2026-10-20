//! Identifier newtypes.
//!
//! These wrap plain strings so that, for example, an image ID can't be passed
//! where a variant name is expected. They don't validate their contents:
//! Cloudflare is the source of truth for what is a valid identifier. Empty
//! identifiers are rejected with [`Error::InvalidInput`](crate::Error::InvalidInput)
//! when a request is sent.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! string_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        #[non_exhaustive]
        pub struct $name(String);

        impl $name {
            /// Wraps a string.
            ///
            #[doc = concat!("```\nlet v = cf_images::", stringify!($name), "::new(\"abc\");\n```")]
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// Returns the identifier as a string slice.
            ///
            #[doc = concat!("```\nassert_eq!(cf_images::", stringify!($name), "::new(\"abc\").as_str(), \"abc\");\n```")]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Returns the inner `String`.
            ///
            #[doc = concat!("```\nlet s: String = cf_images::", stringify!($name), "::new(\"abc\").into_inner();\n```")]
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

    };
}

string_newtype! {
    /// A Cloudflare account ID: the 32-character hex identifier used in API
    /// paths (`/accounts/{account_id}/...`).
    ///
    /// Find it in the dashboard sidebar, or under **Images > Overview**.
    /// This is **not** the [`AccountHash`] used in delivery URLs.
    ///
    /// ```
    /// let id = cf_images::AccountId::new("023e105f4ecef8ad9ca31a8372d0c353");
    /// assert_eq!(id.as_str(), "023e105f4ecef8ad9ca31a8372d0c353");
    /// ```
    AccountId
}

string_newtype! {
    /// The account hash used in image delivery URLs
    /// (`https://imagedelivery.net/{account_hash}/...`).
    ///
    /// It is shown on the **Images > Overview** page of the dashboard and in
    /// the `variants` URLs of every [`Image`](crate::Image). It is a different
    /// value from the [`AccountId`]: the API uses the account ID, and the
    /// delivery network uses the account hash.
    ///
    /// ```
    /// let hash = cf_images::AccountHash::new("ZWd9g1K7eljCn_KDTu_MWA");
    /// assert_eq!(hash.to_string(), "ZWd9g1K7eljCn_KDTu_MWA");
    /// ```
    AccountHash
}

string_newtype! {
    /// An image identifier: either a Cloudflare-generated UUID or a custom ID
    /// set at upload time. Custom IDs may contain `/`.
    ///
    /// ```
    /// let id = cf_images::ImageId::from("2cdc28f0-017a-49c4-9ed7-87056c83901");
    /// assert_eq!(id, "2cdc28f0-017a-49c4-9ed7-87056c83901");
    /// ```
    ImageId
}

string_newtype! {
    /// A variant name such as `public` or `thumbnail` (alphanumeric, at most
    /// 99 characters). Variant names double as variant IDs in the API.
    ///
    /// ```
    /// let v = cf_images::VariantName::new("thumbnail");
    /// assert_eq!(v.as_str(), "thumbnail");
    /// ```
    VariantName
}

string_newtype! {
    /// The name of an image signing key (alphanumeric, at most 20 characters).
    /// Every account has a key named `default`.
    ///
    /// ```
    /// let k = cf_images::SigningKeyName::new("default");
    /// assert_eq!(k.as_str(), "default");
    /// ```
    SigningKeyName
}

pub(crate) fn require_non_empty(kind: &str, value: &str) -> crate::Result<()> {
    if value.is_empty() {
        Err(crate::Error::InvalidInput(format!(
            "{kind} must not be empty"
        )))
    } else {
        Ok(())
    }
}

/// The wire value of a string enum, or `None` if it can't be serialized
/// (an `Unknown` variant).
pub(crate) fn wire_str<T: serde::Serialize>(value: &T) -> Option<String> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => Some(s),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newtypes_are_transparent_in_json() {
        let id: ImageId = serde_json::from_str("\"abc\"").unwrap();
        assert_eq!(id, "abc");
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"abc\"");
    }
}
