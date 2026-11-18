# External pinned fonts

The browser loads the same 42 original fonts from versioned same-origin static
assets instead of embedding them in WASM. Canonical files, manifest, source
provenance and licenses remain in `crates/office-layout/fonts/`, unchanged.
Catalog identity remains `writer-fonts-v6`; registration order, fallback,
metrics, glyph identifiers, pagination and document schemas are unchanged.

`office-layout` compiles manifest descriptors and their SHA-256 identity. Native
Rust retains its embedded catalog. WASM stages bounded external bytes, checks
exact ID/length/SHA-256 and usable font structure, then installs the full catalog
atomically. Missing fonts make layout, selection geometry and PDF fail explicitly.
PDF and editor share this immutable catalog. Registration accepts any arrival
order but constructs the original engine order, which differs from manifest order.

Vite publishes original files at `/fonts/writer-fonts-v6/<manifest-sha256>/`,
including manifest, checksum inventory, notices and licenses. CSS previews use
these same font paths, without a second set of hashed font binaries. Build hooks
validate pinned bytes and reject any static asset above 25 MiB. Windows checkout
attributes preserve byte-exact metadata. Cloudflare settings and security headers
are unchanged; no R2, remote font service or service worker is introduced.

`engine.ts` waits for the font loader before creating the editable session.
Four concurrent downloads, bounded streams, retries and timeouts control startup.
WebCrypto checks every cache/network read; Rust independently verifies registration.
Compressed HTTP transfer lengths are distinguished from decoded font lengths.
Verified bytes also initialize browser FontFace objects, keeping semantic input
and previews available from cached fonts without another network request.

CacheStorage is versioned by manifest digest. Valid partial downloads survive
failure; corrupted entries alone are evicted/refetched. Readiness/completion is
published only after complete Rust finalization. Older versions and other apps'
caches are retained. Storage denial/quota failure permits verified online use
with an explicit offline-cache warning. Retry does not clear saved documents.
Browser cache eviction can require downloads again. This caches fonts, not an
entire offline app shell; no claim of cold offline PWA startup is made.

The original WASM measured 26,031,060 bytes; the first external-font build measured
6,535,911 bytes (about 75% smaller). Final results are recorded below after checks.
Research: [EXTERNAL_FONTS_RESEARCH.md](EXTERNAL_FONTS_RESEARCH.md). New verification
library notices are in `crates/office-layout/THIRD_PARTY_NOTICES.txt` and About.
No merge or deployment is authorized by this report.

## Verification (2026-11-18)

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: 800 passed, zero failures (includes 5 new catalog tests).
- `cargo build --workspace`: passed.
- WASM-target strict Clippy: passed.
- `npm ci`: completed; existing development dependency advisory remains documented.
- `npm run build`: passed, release WASM + TypeScript + Vite.
- `npm run typecheck` and `npm run lint`: passed.
- `npm test`: 87 passed (includes 23 cache-loader tests).
- `npm run test:wasm`: 85 passed, including 6 catalog lifecycle cases, native/WASM
  geometry/metrics/outlines and PDF parity; existing DOCX/ODT and image cycles pass.
- `npm run test:build`: 10 passed, including real Vite dev/build/preview and asset limits.
- Focused Chromium external-font suite: 12 passed.
- Full Chromium stress run (`npm run test:browser -- --project=chromium --workers=4`):
  204 passed, 30 failed. Failures included five-second cold-start waits and
  transient save-state/timing assertions under concurrent load.
- Serial recheck (`npx playwright test --project=chromium --last-failed --workers=1`):
  all 30 passed. Thus every one of the 234 distinct cases passed in executed
  coverage, but the four-worker suite was not clean and was not counted as passing.
  No fresh complete single-worker run was performed. The PR remains draft.
- Canonical font verifier: 42 unchanged fonts, 19,533,340 bytes, notices verified.

Final WASM is **6,535,911 bytes**, down **19,495,149 bytes (74.9%)** from
26,031,060. Headroom below 25 MiB is now **19,678,489 bytes**. All 79 emitted
assets are below that limit. There are exactly 42 original TTFs under `/fonts/`
and zero additional TTFs under `/assets/`. No font was removed or converted.

Browser regressions detected competing CSS font rules causing extra requests and
failed offline previews. The initialization removes those URL rules before using
verified binary FontFace objects; the corrected complete 12-case rerun passes.
A derived-notice guard initially changed an expected error message; that assertion
contract was restored and all ten build tests pass. Full compressed-transfer
length handling is covered by unit tests. Native and WASM font loading remain
independent of document content/history.

Disk exhaustion interrupted test-process startup; only disposable earlier Rust
compilation caches were cleared, preserving all source/documents. Firefox,
WebKit, physical iOS/Android and hosted cross-platform CI are not claimed passed.
Cached-font tests keep the application shell available and block font network;
they do not certify a wholly offline application. Existing development-only
picomatch advisory and hosted-CI availability limitations remain; no audit check
was disabled. Merge and deployment require approval.

Review: https://github.com/boleydavid7-gif/stuff/pull/58 . No merge or deployment.
