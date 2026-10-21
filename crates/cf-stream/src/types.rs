//! Identifier newtypes.
//!
//! These wrap plain strings so that, for example, a video ID can't be passed
//! where a live input ID is expected. They don't validate their contents:
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
            #[doc = concat!("```\nlet v = cf_stream::", stringify!($name), "::new(\"abc\");\n```")]
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// Returns the identifier as a string slice.
            ///
            #[doc = concat!("```\nassert_eq!(cf_stream::", stringify!($name), "::new(\"abc\").as_str(), \"abc\");\n```")]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Returns the inner `String`.
            ///
            #[doc = concat!("```\nlet s: String = cf_stream::", stringify!($name), "::new(\"abc\").into_inner();\n```")]
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

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
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
    /// Find it in the dashboard sidebar. This is **not** the
    /// [`CustomerCode`] used in delivery URLs.
    ///
    /// ```
    /// let id = cf_stream::AccountId::new("023e105f4ecef8ad9ca31a8372d0c353");
    /// assert_eq!(id.as_str(), "023e105f4ecef8ad9ca31a8372d0c353");
    /// ```
    AccountId
}

string_newtype! {
    /// The customer code in delivery URLs
    /// (`https://customer-{code}.cloudflarestream.com/...`).
    ///
    /// It is shown on the Stream page of the dashboard and appears in the
    /// `playback`, `preview` and `thumbnail` URLs of every [`Video`](crate::Video).
    ///
    /// ```
    /// let code = cf_stream::CustomerCode::new("m033z5x00ks6nunl");
    /// assert_eq!(code.to_string(), "m033z5x00ks6nunl");
    /// ```
    CustomerCode
}

string_newtype! {
    /// A video's unique identifier (`uid`), a 32-character hex string.
    ///
    /// ```
    /// let id = cf_stream::VideoId::from("ea95132c15732412d22c1476fa83f27a");
    /// assert_eq!(id, "ea95132c15732412d22c1476fa83f27a");
    /// ```
    VideoId
}

string_newtype! {
    /// A live input's unique identifier.
    ///
    /// ```
    /// let id = cf_stream::LiveInputId::new("66be4bf738797e01e1fca35a7bdecdcd");
    /// assert_eq!(id.as_str(), "66be4bf738797e01e1fca35a7bdecdcd");
    /// ```
    LiveInputId
}

string_newtype! {
    /// The identifier of a live input output (a restreaming destination).
    ///
    /// ```
    /// let id = cf_stream::OutputId::new("baea4d9c515887b80289d5c33cf01145");
    /// assert_eq!(id.as_str(), "baea4d9c515887b80289d5c33cf01145");
    /// ```
    OutputId
}

string_newtype! {
    /// A watermark profile's unique identifier.
    ///
    /// ```
    /// let id = cf_stream::WatermarkId::new("ea95132c15732412d22c1476fa83f27a");
    /// assert_eq!(id.as_str(), "ea95132c15732412d22c1476fa83f27a");
    /// ```
    WatermarkId
}

string_newtype! {
    /// An additional audio track's unique identifier.
    ///
    /// ```
    /// let id = cf_stream::AudioTrackId::new("ea95132c15732412d22c1476fa83f27a");
    /// assert_eq!(id.as_str(), "ea95132c15732412d22c1476fa83f27a");
    /// ```
    AudioTrackId
}

string_newtype! {
    /// A Stream signing key's identifier (the `kid` of signed tokens).
    ///
    /// ```
    /// let id = cf_stream::SigningKeyId::new("ee15edd5e5e8bb0a7c0b1a0c3a8b1c35");
    /// assert_eq!(id.as_str(), "ee15edd5e5e8bb0a7c0b1a0c3a8b1c35");
    /// ```
    SigningKeyId
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
        let id: VideoId = serde_json::from_str("\"abc\"").unwrap();
        assert_eq!(id, "abc");
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"abc\"");
    }
}
