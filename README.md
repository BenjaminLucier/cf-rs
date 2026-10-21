# cf-rs

[![CI](https://github.com/BenjaminLucier/cf-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/BenjaminLucier/cf-rs/actions/workflows/ci.yml)
![MSRV](https://img.shields.io/badge/MSRV-1.85-blue)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

Unofficial Rust clients for Cloudflare media APIs.

The official [`cloudflare`](https://crates.io/crates/cloudflare) crate has no
Images or Stream endpoints, so these crates fill that gap: typed async clients
for uploading, managing and delivering images and video, plus the pieces the
REST API leaves to you, like signed delivery URLs and playback tokens, and
webhook signature verification.

> **Unofficial.** These crates are not affiliated with, endorsed by, or
> supported by Cloudflare, Inc. "Cloudflare" is a trademark of Cloudflare, Inc.
> For the rest of the Cloudflare API, see the official
> [`cloudflare`](https://crates.io/crates/cloudflare) crate.

| Crate | Description | Version | Docs |
|---|---|---|---|
| [`cf-images`](crates/cf-images) | Cloudflare Images: uploads, variants, signing keys, direct creator uploads, delivery and signed URLs | [![crates.io](https://img.shields.io/crates/v/cf-images.svg)](https://crates.io/crates/cf-images) | [docs.rs](https://docs.rs/cf-images) |
| [`cf-stream`](crates/cf-stream) | Cloudflare Stream: videos, uploads (basic, URL, direct, tus creation), captions, audio tracks, downloads, live inputs, watermarks, webhooks, delivery URLs, signed tokens and webhook verification | [![crates.io](https://img.shields.io/crates/v/cf-stream.svg)](https://crates.io/crates/cf-stream) | [docs.rs](https://docs.rs/cf-stream) |

Each crate is versioned and released on its own, and is self-contained: there
is no shared core crate (see [CONTRIBUTING.md](CONTRIBUTING.md#crate-architecture)).
All of them share the same conventions: async `reqwest`-based clients,
`#[non_exhaustive]` public types, deserialization that tolerates additive API
changes, and the same feature names where they apply (`rustls-tls`,
`native-tls`, `tracing`, `signing`).

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for the contribution process and [SECURITY.md](SECURITY.md) for reporting vulnerabilities.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo hack check --workspace --feature-powerset
cargo deny check
```

Live tests against the real API are `#[ignore]`d; see each crate's
`tests/live.rs`, e.g. [`crates/cf-images/tests/live.rs`](crates/cf-images/tests/live.rs).

Releases are automated with [release-plz](https://release-plz.dev); tags look
like `cf-images-v0.1.0` or `cf-stream-v0.1.0`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
