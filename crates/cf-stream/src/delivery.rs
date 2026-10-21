//! Delivery URL builder (no HTTP).
//!
//! Videos are served from `https://customer-{code}.cloudflarestream.com/{id}/...`,
//! where `{id}` is a video ID, a live input ID, or a signed token that
//! replaces the ID for videos requiring signed URLs. The customer code is
//! **not** the account ID: see [`CustomerCode`].

use std::fmt;
use std::time::Duration;

use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::downloads::DownloadType;
use crate::error::{Error, Result};
use crate::types::CustomerCode;

/// Builds playback, thumbnail and download URLs for one account.
///
/// ```
/// use cf_stream::{CustomerCode, Delivery};
///
/// # fn run() -> cf_stream::Result<()> {
/// let delivery = Delivery::new(CustomerCode::new("m033z5x00ks6nunl"))?;
/// let hls = delivery.hls("ea95132c15732412d22c1476fa83f27a");
/// assert_eq!(
///     hls.as_str(),
///     "https://customer-m033z5x00ks6nunl.cloudflarestream.com/ea95132c15732412d22c1476fa83f27a/manifest/video.m3u8"
/// );
/// # Ok(()) }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Delivery {
    base: Url,
}

impl Delivery {
    /// Delivery through `customer-{code}.cloudflarestream.com`.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if the code is empty or contains anything but
    /// ASCII letters and digits.
    ///
    /// ```
    /// # fn run() -> cf_stream::Result<()> {
    /// let delivery = cf_stream::Delivery::new(cf_stream::CustomerCode::new("m033z5x00ks6nunl"))?;
    /// assert!(cf_stream::Delivery::new(cf_stream::CustomerCode::new("evil.example.com/")).is_err());
    /// # Ok(()) }
    /// ```
    pub fn new(code: CustomerCode) -> Result<Self> {
        let code = code.as_str();
        if code.is_empty() || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(Error::InvalidInput(format!(
                "customer code must be ASCII letters and digits, got {code:?}"
            )));
        }
        let base = Url::parse(&format!("https://customer-{code}.cloudflarestream.com/"))
            .map_err(|e| Error::InvalidInput(format!("invalid customer code {code:?}: {e}")))?;
        Ok(Self { base })
    }

    /// Delivery from another origin, such as a mock server in tests.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if `base` isn't an http(s) URL with a host.
    ///
    /// ```
    /// # fn run() -> cf_stream::Result<()> {
    /// let base = "http://127.0.0.1:8080/".parse().expect("valid URL");
    /// let delivery = cf_stream::Delivery::with_base(base)?;
    /// assert_eq!(delivery.watch("abc").as_str(), "http://127.0.0.1:8080/abc/watch");
    /// # Ok(()) }
    /// ```
    pub fn with_base(mut base: Url) -> Result<Self> {
        if base.cannot_be_a_base()
            || base.host_str().is_none()
            || !matches!(base.scheme(), "http" | "https")
        {
            return Err(Error::InvalidInput(format!(
                "delivery base must be an http(s) URL with a host, got {:?}",
                base.as_str()
            )));
        }
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        base.set_query(None);
        base.set_fragment(None);
        Ok(Self { base })
    }

    /// The HLS manifest (`{id}/manifest/video.m3u8`).
    ///
    /// ```
    /// # fn run() -> cf_stream::Result<()> {
    /// let delivery = cf_stream::Delivery::new(cf_stream::CustomerCode::new("code"))?;
    /// assert!(delivery.hls("uid").as_str().ends_with("/uid/manifest/video.m3u8"));
    /// # Ok(()) }
    /// ```
    pub fn hls(&self, id_or_token: impl AsRef<str>) -> Url {
        self.build(id_or_token.as_ref(), &["manifest", "video.m3u8"])
    }

    /// The DASH manifest (`{id}/manifest/video.mpd`).
    ///
    /// ```
    /// # fn run() -> cf_stream::Result<()> {
    /// let delivery = cf_stream::Delivery::new(cf_stream::CustomerCode::new("code"))?;
    /// assert!(delivery.dash("uid").as_str().ends_with("/uid/manifest/video.mpd"));
    /// # Ok(()) }
    /// ```
    pub fn dash(&self, id_or_token: impl AsRef<str>) -> Url {
        self.build(id_or_token.as_ref(), &["manifest", "video.mpd"])
    }

    /// The Stream Player embed URL (`{id}/iframe`), for an `<iframe>`.
    /// Player options such as `autoplay` or `muted` can be appended as query
    /// parameters.
    ///
    /// ```
    /// # fn run() -> cf_stream::Result<()> {
    /// let delivery = cf_stream::Delivery::new(cf_stream::CustomerCode::new("code"))?;
    /// let mut url = delivery.iframe("uid");
    /// url.query_pairs_mut().append_pair("muted", "true");
    /// assert!(url.as_str().ends_with("/uid/iframe?muted=true"));
    /// # Ok(()) }
    /// ```
    pub fn iframe(&self, id_or_token: impl AsRef<str>) -> Url {
        self.build(id_or_token.as_ref(), &["iframe"])
    }

    /// The preview page (`{id}/watch`).
    ///
    /// ```
    /// # fn run() -> cf_stream::Result<()> {
    /// let delivery = cf_stream::Delivery::new(cf_stream::CustomerCode::new("code"))?;
    /// assert!(delivery.watch("uid").as_str().ends_with("/uid/watch"));
    /// # Ok(()) }
    /// ```
    pub fn watch(&self, id_or_token: impl AsRef<str>) -> Url {
        self.build(id_or_token.as_ref(), &["watch"])
    }

    /// The live status URL (`{id}/lifecycle`): JSON saying whether the
    /// input is broadcasting and which video holds the current broadcast.
    /// It needs no API token and allows any origin, so a web player can
    /// poll it directly. Fetch it with
    /// [`LiveInputs::lifecycle`](crate::LiveInputs::lifecycle).
    ///
    /// ```
    /// # fn run() -> cf_stream::Result<()> {
    /// let delivery = cf_stream::Delivery::new(cf_stream::CustomerCode::new("code"))?;
    /// assert!(delivery.lifecycle("input-id").as_str().ends_with("/input-id/lifecycle"));
    /// # Ok(()) }
    /// ```
    pub fn lifecycle(&self, id_or_token: impl AsRef<str>) -> Url {
        self.build(id_or_token.as_ref(), &["lifecycle"])
    }

    /// A JPEG thumbnail (`{id}/thumbnails/thumbnail.jpg`).
    ///
    /// ```
    /// use std::time::Duration;
    /// use cf_stream::{Delivery, CustomerCode, ThumbnailFit, ThumbnailOptions};
    ///
    /// # fn run() -> cf_stream::Result<()> {
    /// let delivery = Delivery::new(CustomerCode::new("code"))?;
    /// let url = delivery.thumbnail(
    ///     "uid",
    ///     &ThumbnailOptions::new().time(Duration::from_secs(12)).height(270).fit(ThumbnailFit::Crop),
    /// );
    /// assert!(url.as_str().ends_with("/uid/thumbnails/thumbnail.jpg?time=12s&height=270&fit=crop"));
    /// # Ok(()) }
    /// ```
    pub fn thumbnail(&self, id_or_token: impl AsRef<str>, options: &ThumbnailOptions) -> Url {
        let mut url = self.build(id_or_token.as_ref(), &["thumbnails", "thumbnail.jpg"]);
        options.apply(&mut url, false);
        url
    }

    /// An animated GIF thumbnail (`{id}/thumbnails/thumbnail.gif`), using
    /// the options' [`duration`](ThumbnailOptions::duration) and
    /// [`fps`](ThumbnailOptions::fps).
    ///
    /// ```
    /// use std::time::Duration;
    /// use cf_stream::{CustomerCode, Delivery, ThumbnailOptions};
    ///
    /// # fn run() -> cf_stream::Result<()> {
    /// let delivery = Delivery::new(CustomerCode::new("code"))?;
    /// let url = delivery.animated_thumbnail(
    ///     "uid",
    ///     &ThumbnailOptions::new().time(Duration::from_millis(1500)).duration(Duration::from_secs(4)).fps(10),
    /// );
    /// assert!(url.as_str().ends_with("/uid/thumbnails/thumbnail.gif?time=1.5s&duration=4s&fps=10"));
    /// # Ok(()) }
    /// ```
    pub fn animated_thumbnail(
        &self,
        id_or_token: impl AsRef<str>,
        options: &ThumbnailOptions,
    ) -> Url {
        let mut url = self.build(id_or_token.as_ref(), &["thumbnails", "thumbnail.gif"]);
        options.apply(&mut url, true);
        url
    }

    /// A download (`{id}/downloads/default.mp4` or `.../audio.m4a`). Create
    /// the rendition first with [`Downloads::create`](crate::Downloads::create).
    /// `filename` (without extension) sets the name the browser saves it as.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] if `filename` is empty, longer than 120
    /// characters, or contains anything but ASCII letters, digits, `-` and
    /// `_`.
    ///
    /// ```
    /// use cf_stream::{CustomerCode, Delivery, DownloadType};
    ///
    /// # fn run() -> cf_stream::Result<()> {
    /// let delivery = Delivery::new(CustomerCode::new("code"))?;
    /// let url = delivery.download("uid", DownloadType::Default, Some("keynote_2024"))?;
    /// assert!(url.as_str().ends_with("/uid/downloads/default.mp4?filename=keynote_2024"));
    /// # Ok(()) }
    /// ```
    pub fn download(
        &self,
        id_or_token: impl AsRef<str>,
        kind: DownloadType,
        filename: Option<&str>,
    ) -> Result<Url> {
        let file = match kind {
            DownloadType::Default => "default.mp4",
            DownloadType::Audio => "audio.m4a",
        };
        let mut url = self.build(id_or_token.as_ref(), &["downloads", file]);
        if let Some(name) = filename {
            let valid = !name.is_empty()
                && name.len() <= 120
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if !valid {
                return Err(Error::InvalidInput(format!(
                    "download filename must be 1 to 120 ASCII letters, digits, `-` or `_`, got {name:?}"
                )));
            }
            url.query_pairs_mut().append_pair("filename", name);
        }
        Ok(url)
    }

    fn build(&self, id: &str, rest: &[&str]) -> Url {
        let mut url = self.base.clone();
        // `base` is always an http(s) URL, so it can be a base.
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty().push(id).extend(rest);
        }
        url
    }
}

/// How a thumbnail is fitted to the requested size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ThumbnailFit {
    /// Fill the size, cropping as needed (the default).
    Crop,
    /// Fit within the size, without cropping.
    Clip,
    /// Stretch to the size.
    Scale,
    /// Fit within the size and fill the rest with black.
    Fill,
    /// A value this version of the crate doesn't recognize. Ignored when
    /// building URLs.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// Options for [`Delivery::thumbnail`] and
/// [`Delivery::animated_thumbnail`]. Unset options use Cloudflare's
/// defaults (the first frame, 640×640, cropped).
///
/// ```
/// use std::time::Duration;
///
/// let options = cf_stream::ThumbnailOptions::new().time(Duration::from_secs(30)).width(320);
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct ThumbnailOptions {
    time: Option<Duration>,
    width: Option<u32>,
    height: Option<u32>,
    fit: Option<ThumbnailFit>,
    duration: Option<Duration>,
    fps: Option<u32>,
}

impl ThumbnailOptions {
    /// No options.
    ///
    /// ```
    /// let options = cf_stream::ThumbnailOptions::new();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// The position in the video to take the thumbnail from.
    ///
    /// ```
    /// let o = cf_stream::ThumbnailOptions::new().time(std::time::Duration::from_secs(5));
    /// ```
    pub fn time(mut self, at: Duration) -> Self {
        self.time = Some(at);
        self
    }

    /// Width in pixels.
    ///
    /// ```
    /// let o = cf_stream::ThumbnailOptions::new().width(480);
    /// ```
    pub fn width(mut self, px: u32) -> Self {
        self.width = Some(px);
        self
    }

    /// Height in pixels.
    ///
    /// ```
    /// let o = cf_stream::ThumbnailOptions::new().height(270);
    /// ```
    pub fn height(mut self, px: u32) -> Self {
        self.height = Some(px);
        self
    }

    /// How the image is fitted to the size. [`ThumbnailFit::Unknown`] is
    /// ignored.
    ///
    /// ```
    /// let o = cf_stream::ThumbnailOptions::new().fit(cf_stream::ThumbnailFit::Clip);
    /// ```
    pub fn fit(mut self, fit: ThumbnailFit) -> Self {
        self.fit = Some(fit);
        self
    }

    /// Length of an animated thumbnail (default 5 seconds). Ignored for
    /// still thumbnails.
    ///
    /// ```
    /// let o = cf_stream::ThumbnailOptions::new().duration(std::time::Duration::from_secs(3));
    /// ```
    pub fn duration(mut self, length: Duration) -> Self {
        self.duration = Some(length);
        self
    }

    /// Frames per second of an animated thumbnail (default 8). Ignored for
    /// still thumbnails.
    ///
    /// ```
    /// let o = cf_stream::ThumbnailOptions::new().fps(12);
    /// ```
    pub fn fps(mut self, fps: u32) -> Self {
        self.fps = Some(fps);
        self
    }

    fn apply(&self, url: &mut Url, animated: bool) {
        let mut pairs: Vec<(&str, String)> = Vec::new();
        if let Some(at) = self.time {
            pairs.push(("time", Seconds(at).to_string()));
        }
        if let Some(px) = self.width {
            pairs.push(("width", px.to_string()));
        }
        if let Some(px) = self.height {
            pairs.push(("height", px.to_string()));
        }
        if let Some(fit) = self.fit.as_ref().and_then(crate::types::wire_str) {
            pairs.push(("fit", fit));
        }
        if animated {
            if let Some(length) = self.duration {
                pairs.push(("duration", Seconds(length).to_string()));
            }
            if let Some(fps) = self.fps {
                pairs.push(("fps", fps.to_string()));
            }
        }
        if !pairs.is_empty() {
            url.query_pairs_mut().extend_pairs(pairs);
        }
    }
}

/// A duration rendered as seconds with an `s` suffix, e.g. `12s` or `1.5s`.
struct Seconds(Duration);

impl fmt::Display for Seconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.subsec_nanos() == 0 {
            write!(f, "{}s", self.0.as_secs())
        } else {
            write!(f, "{}s", self.0.as_secs_f64())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delivery() -> Delivery {
        Delivery::new(CustomerCode::new("abc123")).unwrap()
    }

    #[test]
    fn rejects_bad_codes() {
        for code in ["", "a.b", "a/b", "a b", "evil.com#"] {
            assert!(Delivery::new(CustomerCode::new(code)).is_err(), "{code}");
        }
    }

    #[test]
    fn ids_are_one_encoded_segment() {
        let url = delivery().watch("a/b c");
        assert_eq!(
            url.as_str(),
            "https://customer-abc123.cloudflarestream.com/a%2Fb%20c/watch"
        );
    }

    #[test]
    fn signed_tokens_work_as_ids() {
        let token = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ1aWQifQ.c2ln-_";
        assert_eq!(
            delivery().hls(token).as_str(),
            format!("https://customer-abc123.cloudflarestream.com/{token}/manifest/video.m3u8")
        );
    }

    #[test]
    fn still_thumbnails_ignore_animation_options() {
        let options = ThumbnailOptions::new()
            .duration(Duration::from_secs(2))
            .fps(4)
            .fit(ThumbnailFit::Unknown);
        assert!(delivery().thumbnail("u", &options).query().is_none());
    }

    #[test]
    fn download_filenames() {
        let d = delivery();
        assert!(
            d.download("u", DownloadType::Audio, None)
                .unwrap()
                .as_str()
                .ends_with("/u/downloads/audio.m4a")
        );
        assert!(d.download("u", DownloadType::Default, Some("")).is_err());
        assert!(
            d.download("u", DownloadType::Default, Some("a.mp4"))
                .is_err()
        );
        assert!(
            d.download("u", DownloadType::Default, Some(&"a".repeat(121)))
                .is_err()
        );
        assert!(
            d.download("u", DownloadType::Default, Some(&"a".repeat(120)))
                .is_ok()
        );
    }

    #[test]
    fn custom_base() {
        let d = Delivery::with_base("http://localhost:1/prefix".parse().unwrap()).unwrap();
        assert_eq!(d.iframe("u").as_str(), "http://localhost:1/prefix/u/iframe");
        assert!(Delivery::with_base("mailto:a@b".parse().unwrap()).is_err());
    }
}
