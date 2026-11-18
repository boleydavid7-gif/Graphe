# External pinned font delivery

Research date: 2026-11-18. Scope: move the existing font bytes out of the browser
WASM asset without changing fonts, layout authority, document data or deployment
access settings. Research is separate from the final build/test results.

## Existing implementation and alternatives

Reviewed `office-layout` font registration, `fonts/manifest.json`, layout-profile
rules, browser initialization, stylesheet font previews and existing asset
builds. The current `writer-fonts-v6` manifest contains 42 fonts totaling
19,533,340 bytes; all recorded lengths and SHA-256 values matched local files.
Its SHA-256 is `25e065fd2d421b1fec34793b1bc67a3794f0060c78e0b1044027f46d3ffa5f9f`.
Preserve these files, the canonical manifest, original registration order,
substitutions, family strings and OFL notices.

Embedding everything keeps startup self-contained but enlarges every WASM
download and approaches Cloudflare's [25 MiB individual static-asset limit](https://developers.cloudflare.com/workers/platform/limits/).
Host/discovered fonts would change deterministic pagination. Third-party font
services, R2, a service worker or a new storage library are unnecessary for this
change. Choose versioned same-origin static font assets, existing browser APIs,
and the existing Rust font/layout engine. Verify the final production output;
gzip size does not establish compliance with the asset limit.

## Integrity and Rust authority

Compile expected filenames, lengths, digests and catalog identity from the pinned
manifest. Rust independently checks every supplied font before registration.
Reject missing/extra/duplicate files, wrong lengths/digests and catalog mismatches.
Stage a complete font collection, preserving original registration order; publish
it atomically only after all 42 fonts validate and register. Failed initialization
must not replace a working collection or silently select host/fallback fonts.
Fonts and caches remain runtime resources outside documents and undo history.

Use mature RustCrypto `sha2` **0.11.1**, with default features disabled, rather
than a handwritten SHA-256 implementation or a browser-only trust check.
The published archive is MIT OR Apache-2.0, requires Rust 1.88 and supports a
portable software backend on `wasm32`. JavaScript can use native
[`crypto.subtle.digest('SHA-256', bytes)`](https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto/digest)
for early cache/network checks; it does not replace Rust validation. WebCrypto
requires a secure context and buffers the input, so apply byte limits first.

The initial 0.10.9 candidate was rejected after direct archive/source review:
its AArch64 round-constant loads use the one-element-reference pattern corrected
in upstream [PR #922](https://github.com/RustCrypto/hashes/pull/922), released in
0.11.1. Do not mistake an older indexed documentation page for the latest release.
The historical [SHA-2 AVX2 advisory](https://rustsec.org/advisories/RUSTSEC-2021-0100.html)
concerns older versions; the new lockfile still requires an actual advisory check.

Verified published archives for the new SHA-2 dependency chain use MIT OR
Apache-2.0: `digest` 0.11.3, `crypto-common` 0.2.2, `block-buffer` 0.12.1,
`hybrid-array` 0.4.15, `typenum` 1.20.1 and native-target `cpufeatures` 0.3.1.
The final lockfile determines exact versions and enabled targets. Preserve their
complete license texts alongside existing notices. Optional random-number,
assembly, MAC and allocation facilities are not needed. This review does not
replace the planned comprehensive commercial/security audit.

## Browser fetching, caching and failure behavior

The [Cache API](https://developer.mozilla.org/en-US/docs/Web/API/Cache) is available
in window scope without a service worker. Use a catalog/version-specific cache
and pinned same-origin paths. Cache entries do not automatically expire or honor
HTTP freshness rules; always recheck their exact length and SHA-256 on use.
Cache poisoning must not become trusted font registration.

Fetch only pinned local URLs with [`mode: 'same-origin'`](https://developer.mozilla.org/en-US/docs/Web/API/Request/mode),
`credentials: 'same-origin'`, [`redirect: 'error'`](https://developer.mozilla.org/en-US/docs/Web/API/Request/redirect)
and cancellation. Reject failed/partial/opaque responses. Stream a bounded body;
`Content-Length` is an early hint, not proof of actual size. Abort/cancel reads
on excess bytes, cancellation or failure. Synthetic cached `Response` objects
have type `default`, whereas same-origin network responses are `basic`; both
can be readable, but opaque responses cannot support integrity checks.

Await every [`Cache.put`](https://developer.mozilla.org/en-US/docs/Web/API/Cache/put)
promise. Separate puts are not a catalog-wide transaction: write a completion
marker last and revalidate entries even when that marker exists. Preserve
previous versions and valid partial entries on failure. Never delete a shared
version cache merely because one tab's initialization failed; another tab may
have just completed it. Limit cleanup to owned, verified-invalid keys.

Cache unavailability, quota failures and eviction are normal browser outcomes.
An online session can use verified fetched bytes while warning that offline font
availability was not established. A cold offline start with missing/corrupt fonts
must fail clearly rather than change pagination. Browser storage is
[best effort and origin-scoped](https://developer.mozilla.org/en-US/docs/Web/API/Storage_API/Storage_quotas_and_eviction_criteria);
font-cache space shares storage pressure with documents. Do not claim permanent
availability or silently change existing document-saving safeguards.

Keep catalog completion distinct from document-save completion: existing
IndexedDB success still waits for the
[transaction complete event](https://developer.mozilla.org/en-US/docs/Web/API/IDBTransaction/complete_event).
Cached fonts can be reused offline once the application is already available;
without a service worker this does **not** promise that a fresh offline navigation
can load the HTML, JavaScript and WASM application shell. No new permissions,
service worker, CSP, DNS or access-policy changes are included.

## Acceptance risks and tests

Critical risks are accepting unverified/partial fonts, changing registration
order or silently laying out with a different catalog. Important risks are
cross-tab cache cleanup, stale initialization completion, misleading offline
claims, transient memory duplication and stylesheet previews fetching a separate
uncached font URL. Prefer shared versioned asset paths; previews remain separate
from Rust document measurement.

Test altered bytes/lengths, duplicate/missing fonts, atomic failed registration,
native/WASM geometry equality, cold/warm cache, cache corruption and eviction,
network/auth redirect failures, oversized streams, aborted loads, quota/cache
unavailability and retries. Verify root/subpath production asset loading and
unchanged canonical font files/notices. Offline tests must state whether they
exercise only font reuse, an already loaded page, or a fresh navigation.
Emulated browser coverage does not establish physical iOS/PWA behavior.

Additional primary sources: [RustCrypto SHA-2](https://github.com/RustCrypto/hashes/tree/sha2-v0.11.1/sha2),
[published 0.11.1 archive](https://static.crates.io/crates/sha2/sha2-0.11.1.crate),
[Fetch response types](https://developer.mozilla.org/en-US/docs/Web/API/Response/type).
