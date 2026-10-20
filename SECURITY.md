# Security policy

## Supported versions

Only the latest minor release of each crate receives security fixes.

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub:
[Report a vulnerability](https://github.com/BenjaminLucier/cf-rs/security/advisories/new).
If you can't use GitHub, email benjaminlucier@gmail.com instead. Please don't
open a public issue or PR for a suspected vulnerability.

You can expect an acknowledgement within 3 working days. Fixes are coordinated
with you, and details are disclosed publicly once a fixed release is available,
or after 90 days at the latest.

## Scope

In scope: the code in this repository, in particular

- handling of API tokens and signing keys (they must never appear in logs,
  `Debug` or error output),
- signed delivery URL generation (`UrlSigner`),
- request construction and response parsing.

Out of scope:

- Vulnerabilities in Cloudflare's services or API. Report those to Cloudflare
  through their [vulnerability disclosure program](https://hackerone.com/cloudflare).
- Vulnerabilities in dependencies that don't affect this code. Report them
  upstream; we pick up fixes through Dependabot and `cargo deny`.

## Safe harbour

Good-faith research that follows this policy, avoids accessing other people's
data, and gives us reasonable time to fix issues will not be pursued by the
maintainers. This doesn't cover testing against Cloudflare's systems, which
is governed by Cloudflare's own policy.
