# Implementation notes

Discrepancies between Cloudflare's docs, its OpenAPI schema
(`docs/openapi-images.yaml`, upstream commit in `docs/SCHEMA_REVISION`), and
this crate's behavior. When the two disagree, the crate follows the documented
or observed behavior and a test pins it down.

Status legend: **verified** = covered by docs and wiremock tests; **unverified**
= not yet confirmed against the live API (the `#[ignore]`d live tests in
`crates/cf-images/tests/live.rs` exercise these).

## Schema vs. docs

1. **Variant list is a map, not a `hero` object.** The schema models
   `GET /v1/variants` `result.variants` as an object with a single `hero`
   property (an example that leaked into the type). In reality it is a map keyed
   by variant name. Modelled as `BTreeMap<String, Variant>`.
   Test: `variants::list_returns_map_keyed_by_name`.

2. **Image ID length.** `images_image_identifier` says `maxLength: 32`, but
   custom IDs may be up to 1024 characters and contain `/` (direct upload
   schema and upload docs). `ImageId` is an unbounded string.

3. **`uploaded` timestamp format.** The schema example is
   `2014-01-02 02:20:00.123000+00:00` (space separator). The API returns
   ISO 8601 (`2014-01-02T02:20:00.123Z`). Exposed as a `String` with no
   date-time dependency, so either format parses.

4. **`metadata` in multipart bodies.** The upload and direct-upload schemas
   type `metadata` as an object, but multipart form fields are strings. The
   crate sends JSON text (`metadata={"key":"value"}`), as the docs' curl
   examples do. Test: `images::upload_bytes_sends_all_multipart_fields`.

5. **Direct upload `expiry`.** The direct creator upload guide shows `expiry`
   sent as a JSON body (`--data '{"expiry":"..."}'`), while its other examples
   and the schema use `multipart/form-data`. The crate sends every field,
   `expiry` included, as multipart form fields, formatted as RFC 3339 UTC with
   whole seconds (`2021-09-14T16:00:00Z`). **Unverified** live:
   `live::direct_upload_url` sets an expiry.

6. **Blob endpoint.** `GET /v1/{image_id}/blob` returns `image/*` bytes on
   success and a JSON envelope on 4xx. The crate also treats a 2xx
   `application/json` body with `"success": false` as an error, and builds
   `Error::Api` with empty `errors` for non-JSON error bodies.
   Tests: `images::blob_*`.

7. **`per_page` range for list v2.** The schema says 10 to 10 000 (default
   1000); the method page prose says "up to 1000 results per page". The crate
   passes the value through and lets the API decide.

8. **Delete responses.** The reference index types the result as "unknown or
   string"; the schema example is `{}`. The crate ignores the result
   (`{}`, `null`, and `""` all work). Test: `images::delete_accepts_empty_object_and_null_results`.

9. **`draft` field.** Not in the schema. Documented in the direct creator
   upload guide ("set to `true` until a creator uploads an image"). Exposed as
   `Image::draft: Option<bool>`.

10. **Numbers.** Variant `width`/`height` and stats counts are `number` in the
    schema; they are integers in practice and modelled as `u32` / `u64`.

11. **Required variant fields.** The schema marks `options.fit`, `width`,
    `height`, `metadata` and the variant `id` required, so they are not
    `Option`. Unknown `fit`/`metadata` values deserialize to `Unknown`.
    Test: `variants::unknown_enum_values_deserialize_to_unknown`.

## Behavior choices

12. **`/` in path IDs.** Each path segment is percent-encoded, so the custom ID
    `products/42` is sent as `/images/v1/products%2F42`. This matches how
    Cloudflare's official SDKs encode path parameters. **Unverified** live:
    `live::custom_id_with_slash`. Delivery URLs, by contrast, keep the `/`
    (`imagedelivery.net/{hash}/products/42/{variant}`), as Cloudflare documents
    for custom IDs.

13. **`requireSignedURLs` update changes the ID.** The schema says changing it
    "returns a new UID". `UpdateImage::require_signed_urls` documents this; the
    live test re-reads the ID from the response.

14. **Custom IDs and signed URLs** can't be combined (upload/direct-upload
    docs). The crate doesn't enforce it; Cloudflare returns an error.

15. **Signed URLs.** Implemented per the Worker sample on "Serve private
    images": `exp` (Unix seconds) is set first, the HMAC-SHA256 is computed over
    `pathname + "?" + searchParams.toString()` using the key's UTF-8 bytes
    (not base64-decoded, even though key values look like base64), and the
    lowercase hex digest is appended as `sig`. The crate removes any existing
    `exp`/`sig` before signing (the JS sample would keep a stale `sig` in the
    signed string). Query parameters are re-serialized as
    `application/x-www-form-urlencoded`, as `URLSearchParams` does
    (`%20` becomes `+`). Test vectors were cross-checked with OpenSSL and a Node
    port of the sample; see `src/signing.rs`.

16. **Rate limits.** `is_rate_limited()` is based on HTTP 429 only;
    `retry_after()` reads `Retry-After` in seconds.

17. **`is_unauthorized()`** is true for 401 and 403, because Cloudflare
    returns 403 (code 10000) for invalid tokens as well as missing permissions.

18. **The client owns its HTTP client.** As in the official `cloudflare` crate
    (`async_api::Client::new` builds its own `reqwest::Client` from a small
    `ClientConfig`), `cf-images` doesn't accept a user-supplied
    `reqwest::Client`. The only knobs are `base_url` and `timeout`. Retries are
    left to the caller (see the crate docs for a loop that honors
    `Retry-After`). Adding `ClientBuilder::http_client` later would be
    non-breaking if a real need comes up.

## Not covered (present in the schema, absent from the reference index)

- `POST /v1/direct_upload` (deprecated v1 direct upload; use v2).
- `GET /v1/variants/{variant_id}/flat` (deprecated).
- `/v2/sourcingkit/*` (Sourcing Kit migrations and sources): a separate
  migration product surface; a candidate for a later minor release.

`GET /v2/metadata/keys` *is* covered (`Images::metadata_keys`) even though the
reference index doesn't list it.

## Dependencies

- `sha2` 0.11.1 requires Rust 1.88, above the 1.85 MSRV. The workspace uses
  resolver 3, which picks `sha2` 0.11.0 (MSRV 1.85) in `Cargo.lock`.
  Downstream users on newer toolchains get 0.11.1 automatically.
- reqwest 0.13's `rustls` feature uses `aws-lc-rs`, which needs a C compiler
  and CMake to build. Use `native-tls` (with `default-features = false`) to avoid
  it.
- reqwest is used with `default-features = false` and only `json`,
  `multipart`, `stream` and `http2` (plus the TLS feature chosen by
  `rustls-tls`/`native-tls`). `system-proxy` is left out: proxies from
  `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY` are still honored, but OS-level proxy
  settings (macOS System Settings, the Windows registry) aren't read. `charset`
  is left out because response bodies are parsed as JSON or kept as bytes.
