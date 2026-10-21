# Changelog

All notable changes to this crate are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/BenjaminLucier/cf-rs/compare/cf-stream-v0.1.0...cf-stream-v0.1.1) - 2026-10-21

### Fixed

- *(cf-stream)* page oldest-first video lists past the inclusive `after` ([#12](https://github.com/BenjaminLucier/cf-rs/pull/12))

### Other

- say what gap the crates fill ([#11](https://github.com/BenjaminLucier/cf-rs/pull/11))

## [0.1.0] - 2026-10-21

### Added

- `Client` / `ClientBuilder` with API-token auth, custom base URL and request
  timeout. Like the official `cloudflare` crate, the client builds and owns its
  own `reqwest::Client`.
- Videos: `get`, `update`, `delete`, `list` (filters, `range`/`total`),
  `list_all` / `into_stream` (pages by creation time, deduplicating
  boundaries), basic `upload` (bytes or streams), `copy` from a URL, `clip`,
  `storage_usage` and API-side `create_token`.
- Direct creator uploads: `direct_uploads().create()`, and tus upload
  creation with `create_tus()` (including `direct_user`). Sending tus chunks
  is left to a tus client; see the crate docs and the `stream_tus_upload`
  example.
- Captions: `list`, `get`, `upload` (WebVTT), `generate` (AI), `delete`, `vtt`.
- Audio tracks: `list`, `copy`, `update`, `delete`.
- Downloads: `get`, and per-type `create` / `delete` (MP4 and M4A).
- Signing keys: `create`, `list`, `delete`.
- Live inputs: `list`, `get`, `create` (with an idempotency key), `update`,
  `delete`, `enable`, `disable`, `rotate_keys`; outputs: `list`, `create`,
  `update`, `delete`. Stream keys and passphrases are `SecretString`s.
- Live status: `live_inputs().lifecycle()` fetches whether an input is
  broadcasting (and which video holds the broadcast) from the delivery
  domain, without sending the API token; `Delivery::lifecycle` builds the
  URL.
- Watermark profiles: `create` (file upload or URL), `list`, `get`, `delete`.
- Webhooks: `get`, `set`, `delete`.
- `Delivery` URL builder for `customer-<code>.cloudflarestream.com`:
  HLS/DASH manifests, player iframe, preview page, still and animated
  thumbnails, and downloads.
- `TokenSigner` for offline RS256 signed playback tokens, with
  `AccessRule`s (feature `signing`).
- `WebhookVerifier` for `Webhook-Signature` verification (feature
  `webhooks`).
- `Error` with status/code helpers (`is_not_found`, `is_rate_limited`,
  `is_unauthorized`, `retry_after`).
- Re-exports of `reqwest` and `secrecy`, so callers can match the versions
  used in the public API.
- Features: `rustls-tls` (default), `native-tls`, `signing` (default),
  `webhooks` (default), `tracing`.

[Unreleased]: https://github.com/BenjaminLucier/cf-rs/compare/cf-stream-v0.1.0...HEAD
[0.1.0]: https://github.com/BenjaminLucier/cf-rs/releases/tag/cf-stream-v0.1.0
