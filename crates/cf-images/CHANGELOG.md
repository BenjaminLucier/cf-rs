# Changelog

All notable changes to this crate are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/BenjaminLucier/cf-rs/compare/cf-images-v0.1.0...cf-images-v0.1.1) - 2026-10-26

### Fixed

- raise MSRV to 1.87 and update ctutils (RUSTSEC-2026-0385) ([#18](https://github.com/BenjaminLucier/cf-rs/pull/18))

### Other

- *(cf-images)* record live-verified pagination, blob and stats behaviour ([#13](https://github.com/BenjaminLucier/cf-rs/pull/13))
- say what gap the crates fill ([#11](https://github.com/BenjaminLucier/cf-rs/pull/11))
- *(cf-images)* document API quirks and unverified assumptions ([#10](https://github.com/BenjaminLucier/cf-rs/pull/10))
- *(cf-images)* prefix example names ([#7](https://github.com/BenjaminLucier/cf-rs/pull/7))
- fix live test PNG and cleanup deadlock ([#6](https://github.com/BenjaminLucier/cf-rs/pull/6))
- add CI and license badges ([#2](https://github.com/BenjaminLucier/cf-rs/pull/2))

## [0.1.0] - 2026-10-21

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
- Re-exports of `reqwest` and `secrecy`, so callers can match the versions
  used in the public API.
- Features: `rustls-tls` (default), `native-tls`, `signing` (default),
  `tracing`.

[Unreleased]: https://github.com/BenjaminLucier/cf-rs/compare/cf-images-v0.1.0...HEAD
[0.1.0]: https://github.com/BenjaminLucier/cf-rs/releases/tag/cf-images-v0.1.0
