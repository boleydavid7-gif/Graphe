# Graphe Step 8A verification

Date: 2026-11-18. Branch: `dev/graphe-step8a-images`, from verified main
`184364ac81b197c1f90a1f9ff836d688368f2f91`. Implementation commit: `e60de7cd60912e2b7f29645dbdef89751b26d5db`.
Draft review: [PR #57](https://github.com/boleydavid7-gif/stuff/pull/57).
No deployment or merge is included.
The separate unfinished audit worktree was preserved. Feature scope and usage
are in [IMAGES.md](IMAGES.md); standards/dependency review is in
[IMAGES_RESEARCH.md](IMAGES_RESEARCH.md).

## Implementation

- Format-neutral body image blocks, immutable shared PNG/JPEG assets, original
  bytes, stable IDs, physical dimensions, alignment and Unicode alternative text.
- Bounded complete raster validation, strict PNG checksums, JPEG framing,
  progressive JPEG, alpha, all eight EXIF orientations and explicit color limits.
- Native envelope/model 6 with strict 1–5 migration, original-source retention
  and aggregate count/byte/pixel preflight before full decoding.
- Existing atomic transactions and exact history for insertion, properties and
  deletion. Rust-selected image identity prevents text/clipboard operations from
  acting on an earlier text caret. Text edits cannot cross image barriers.
- Rust image rectangles, alignment and unsplit pagination, including image-only
  pages. Browser input remains a semantic projection; SVG/PDF use Rust geometry
  and the same normalized pixels. Image bytes stay out of typing snapshots.
- Lamplight insertion/properties controls, trusted image selection without input
  focus, keyboard exit/deletion, native save/reopen and guarded asynchronous reads.
- Dedicated embedded-picture DOCX/ODT profiles; current-content relationships
  cannot resurrect deleted source pictures. PDF raster XObjects and soft alpha
  masks; TXT meaningful-loss warning, RTF explicit unsupported-image error.

Changes span `office-core`, `office-layout`, `office-docx`, `office-formats`,
`office-wasm`, `apps/writer`, fixtures, dependency notices and engineering docs.
No UI framework, storage schema, external image service or additional document
format was introduced.

## Retrospective corrections

Critical risks were addressed before completion: image-crossing destructive text
ranges, aggregate decode allocation before validation, complete-stream checksums,
ambiguous/late image metadata, EXIF orientation consistency, stale asynchronous
insertion, and accidental reuse of deleted media from a retained source.

Dependency research found that initial `kamadak-exif` 0.6.1 was affected by
[GHSA-53gf-vqf6-qvp8](https://github.com/kamadak/exif-rs/security/advisories/GHSA-53gf-vqf6-qvp8),
including bounded raw-TIFF parsing. The final lockfile uses **0.6.2**. Regression
payloads exercise repeated IFD rejection in both PNG and JPEG; the browser test
also rejects the malicious PNG without replacing current work. Complete upstream
licenses are retained and emitted through About notices.

Regression review also corrected a `png` decoder metadata-field assumption that
missed non-sRGB `gAMA` warnings. The browser now discloses approximation on
insertion. Oversized native pictures can be selected and resized/deleted through
text fallback when paged layout is unavailable. Independent static frontend
review found no remaining confirmed critical image-integrity defect; it is not
an accessibility or physical-device certification.

## Executed checks

Commands from repository root, with stable Rust 1.100.0 and locked dependencies:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace` | **795 passed, 0 failed**, including doctests |
| `cargo build --workspace` | Passed |
| `cargo check --locked --workspace --target wasm32-unknown-unknown` | Passed |
| `python3 crates/office-layout/fonts/verify.py` | Passed: 42 fonts and notices; 65 SHA-256 entries also independently checked |

Commands from `apps/writer` with Node 24.22.1:

| Command | Result |
| --- | --- |
| `npm ci` | Passed; 182 packages installed |
| `npm run build` | Passed, including release WASM, TypeScript and Vite production assets |
| `npm run typecheck` | Passed |
| `npm run lint` | Passed |
| `npm test` | **64 passed, 0 failed** |
| `npm run test:wasm` | **79 passed, 0 failed/skipped**, actual compiled WASM including native layout/PDF parity |
| `npm run test:build` | **4 passed, 0 failed** |
| `npm audit --audit-level=moderate` | **Failed**: one existing picomatch advisory propagated across 13 development packages; no patched published release at verification |
| `npm audit --omit=dev --json --cache /tmp/graphe-audit-npm-cache` | Passed: zero production-dependency findings; does not waive full audit failure |
| `npm run test:browser -- --project=chromium` | **222 passed, 0 failed/skipped**, including all 14 image cases; 16.4 minutes |

New Rust regressions: 19 core, 14 DOCX, 12 format, 10 layout and 9 binding tests.
These **64 additions are included in the 795 total**, not extra suite passes.
The 7 new actual-WASM cases are included in 79. All 14 new image browser cases
are included in the complete 222-case Chromium result; focused reruns are not added to totals.

The default process sandbox initially prevented child-process build tests and
reported a failed test-file wrapper. Rerunning with permitted subprocess/network
capabilities executed all four actual cases successfully. An earlier browser
run was deliberately interrupted after 29 passes when the security upgrade
required rebuilding; that incomplete run is not the final suite result.

## Independent file consumers

Actual LibreOfficeDev 26.8 alpha and Poppler 26.05 converted and rendered the
procedural two-section/three-image fixture. Pillow 13 read raster pixels.
`python3 crates/office-formats/tests/verify-image-consumers.py /tmp/graphe-image-consumers-final-062`
passed **7 probes**: our DOCX/ODT, DOCX↔ODT conversion packages, and three two-page
PDF renderings. Verified ordinary raster byte retention, image order, Unicode
alt text, physical alignment, transparency and EXIF6 orientation. These are
independent-consumer checks, not seven additional unit tests or Word certification.

LibreOffice line boxes move pictures approximately 4–9 points relative to our
zero-spacing block profile. LibreOffice-resaved ODT graphic styles remain
read-only; the DOCX producer fixture also has unrelated existing settings and
page-break restrictions. Complete originals remain available. Microsoft Word
execution was unavailable; self-roundtrips are not represented as Word fidelity.

## Bundle and performance

Final production WASM: **26,031,060 bytes**, **183,340 bytes** under Cloudflare's
25 MiB (26,214,400-byte) per-asset ceiling. `build-wasm.mjs` now fails an oversized
artifact. This is a narrow budget requiring review for future features; no
Cloudflare configuration or deployment was changed.

`node scripts/benchmark-images.mjs [output.json]` measures real synchronous WASM
edits/snapshots with 20 warm-up and 100 measured edits per case. On the shared
host, patched-build dispatch median/p95 milliseconds were 0.137/2.370 without
images, 5.455/42.019 for a 1 MiB encoded PNG, and 18.308/66.975 near the 5 MiB
native limit. Ordinary snapshot medians stayed below 0.025 ms. Small 160×80
rasters with valid ancillary metadata isolate source serialization cost; this
is not a photo decode, layout, DOM or end-to-end input benchmark. Concurrent
suites materially affect tails. Retained-source serialization cost remains a
measured limitation; no speculative engine/storage rewrite was introduced.

## Verification limits and remaining scope

Firefox and WebKit launch probes failed because their Playwright executables
are absent. Those suites did **not** pass. Physical iOS Safari, Android Chrome,
installed-app keyboard behavior, screen readers and Microsoft Word were not
available. Chromium uses a real browser with touch/viewport emulation and
synthetic composition/clipboard events; that does not establish device behavior.
Hosted PR CI run [44240061445](https://github.com/boleydavid7-gif/stuff/actions/runs/44240061445)
reported all four jobs failed with no steps or job log URLs exposed by the
connector. That does not establish a billing, runner-allocation or source-code
cause. Hosted checks have **not** passed; local Linux success is not hosted
macOS/Windows coverage. The final PR commit must be checked separately.

A fresh npm advisory scan also failed for the existing development-tool
`picomatch` graph (one advisory, 13 propagated package findings). At verification
there was no published patched version. See DEPENDENCIES.md; the CI audit
threshold was preserved. The PR remains draft pending review, hosted checks and
resolution/review of that development-tool finding. No merge is recommended yet.

Images are static standalone body PNG/JPEG, not floating/wrapped/cropped,
inline-mixed, table-cell, clipboard or arbitrary vector images. Color management
is approximate where disclosed. Native and original-file copies can retain
EXIF/location/deleted source content. Browser complete-native-file budget remains
5 MiB. RTF images remain unsupported; TXT loses raster content only after warning.
PDF text remains outlined and is not searchable/tagged/PDF/A/PDF/UA certified.

Actual application screenshots: [desktop](images/graphe-images-desktop.png) and
[emulated mobile](images/graphe-images-mobile.png). Merge and deployment require
separate approval. No next milestone is begun automatically.
