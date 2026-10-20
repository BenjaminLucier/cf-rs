# Changelog

All notable changes to this crate are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-10-20

### Added

- `Client` / `ClientBuilder` with API-token auth, custom base URL and request
  timeout. Like the official `cloudflare` crate, the client builds and owns its
  own `reqwest::Client`.
- Images: `upload` (bytes, streams, or URL; custom ID, metadata, creator,
  signed-URL requirement), `get`, `update`, `delete`, `blob`, `list` (v2,
  with metadata filters), `list_all` / `into_stream` (follows continuation
  tokens), and the deprecated `list_v1`.
- Variants: `list`, `get`, `create`, `update`, `delete`.
- Signing keys: `list`, `create`, `delete`.
- Usage statistics: `Client::stats`.
- Direct creator uploads: `direct_uploads().create()`.
- `Delivery` URL builder for `imagedelivery.net` and custom domains, with
  `FlexibleVariant` options.
- `UrlSigner` for signed delivery URLs (feature `signing`).
- `Error` with status/code helpers (`is_not_found`, `is_rate_limited`,
  `is_unauthorized`, `retry_after`).
- Features: `rustls-tls` (default), `native-tls`, `signing` (default),
  `tracing`.

[Unreleased]: https://github.com/BenjaminLucier/cf-rs/compare/cf-images-v0.1.0...HEAD
[0.1.0]: https://github.com/BenjaminLucier/cf-rs/releases/tag/cf-images-v0.1.0
