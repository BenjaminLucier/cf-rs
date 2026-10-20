# cf-rs

Unofficial Rust clients for Cloudflare media APIs.

> **Unofficial.** These crates are not affiliated with, endorsed by, or
> supported by Cloudflare, Inc. "Cloudflare" is a trademark of Cloudflare, Inc.
> For the rest of the Cloudflare API, see the official
> [`cloudflare`](https://crates.io/crates/cloudflare) crate.

| Crate | Description | Docs |
|---|---|---|
| [`cf-images`](crates/cf-images) | Cloudflare Images: uploads, variants, signing keys, direct creator uploads, delivery and signed URLs | [docs.rs](https://docs.rs/cf-images) |

Each crate is versioned and released on its own. All of them share the same
conventions: async `reqwest`-based clients, `#[non_exhaustive]` public types,
deserialization that tolerates additive API changes, and the same feature
names (`rustls-tls`, `native-tls`, `tracing`, `signing`).

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for the contribution process and [SECURITY.md](SECURITY.md) for reporting vulnerabilities.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo hack check --workspace --feature-powerset
cargo deny check
```

Live tests against the real API are `#[ignore]`d; see
[`crates/cf-images/tests/live.rs`](crates/cf-images/tests/live.rs).

The `docs/` directory holds the API reference and the filtered OpenAPI schema
the crates were checked against, plus [`docs/NOTES.md`](docs/NOTES.md) with
known discrepancies between Cloudflare's docs and schema.

Releases are automated with [release-plz](https://release-plz.dev); tags look
like `cf-images-v0.1.0`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
