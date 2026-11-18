# Writer dependency review

Reviewed on 2026-11-14. This inventory records upstream dependency licenses;
Graphe's proprietary software notice is in the repository root LICENSE. No Office engine or existing
rich-text editor is embedded. The npm lockfile and Cargo.lock pin the resolved
graphs, including platform-specific optional packages.

## Runtime and binding dependencies

| Dependency | Version | License | Purpose |
| --- | --- | --- | --- |
| React / React DOM | 19.3.1 | MIT | UI presentation and lifecycle |
| wasm-bindgen | 0.2.130 | MIT OR Apache-2.0 | Small Rust/WebAssembly binding boundary |
| base64 | 0.23.1 | MIT OR Apache-2.0 | Native original-source encoding with strict standard padding |

The JavaScript production graph also contains React's MIT-licensed scheduler.
Existing Serde and Unicode segmentation libraries remain the engine's source
of serialization and grapheme behavior. IndexedDB uses native browser APIs;
there is no storage wrapper, JavaScript document model, or browser undo library.

`base64` is pinned exactly with default features disabled and `std` enabled.
It is a maintained low-level codec, with no additional runtime dependency and
Rust 1.71 as its declared minimum. Its upstream metadata and installed licenses
were reviewed; native/WASM checks exercise the retained-source path. Encoding
budgets and canonical decoding are enforced by the native-file layer rather
than a handwritten base64 algorithm. Project licensing remains pending.
The [upstream 0.23.1 tag](https://github.com/marshallpierce/rust-base64/releases/tag/v0.23.1)
and [RustSec package/advisory record](https://rustsec.org/advisories/RUSTSEC-2017-0004.html)
were checked: the historical encoding overflow affects old pre-0.5.2 versions,
not the selected version. This is a targeted advisory review, not `cargo audit`.

## DOCX adapter dependencies (Steps 5A–5C)

| Dependency | Exact version | License | Purpose |
| --- | --- | --- | --- |
| zip | 9.5.1 | MIT | Checked stored/deflate ZIP reads; other compressors/encryption disabled |
| quick-xml | 0.42.0 | MIT | Bounded token preflight before DOM work |
| roxmltree | 0.21.2 | MIT OR Apache-2.0 | Namespace-aware XML well-formedness and OPC metadata |

These dependencies belong exclusively to `office-docx`, with default features
disabled. The deflate backend uses `flate2` (MIT OR Apache-2.0) and `zlib-rs`
(Zlib). The ZIP crate requires Rust 1.88; quick-xml requires 1.86; the workspace
uses stable 1.100.0 on the review date. A native and WebAssembly compile were
checked. `zip` enables a WASM getrandom backend in its dependency graph; this
does not add parser dependencies to `office-core`. The browser conversion bridge
now depends on `office-docx`, so the adapter is compiled into its WASM build;
JavaScript bindings remain confined to `office-wasm`.

Two XML parsers are deliberate: a bounded token pass limits namespace/attribute
work before the DOM parser, which provides stricter namespace/name validation.
Targeted preflight checks correct invalid scalar/namespace acceptance observed
in roxmltree 0.21.2. See [DOCX.md](DOCX.md) for the explicit limits and tradeoffs.

Versions and licenses were checked in registry metadata and source manifests.
The graph was compared manually with the official RustSec database at commit
`4ef2df8e806a65e2f1ecc14f56f51249062b23d6`; applicable affected ranges do not
include the selected versions. In particular quick-xml 0.42.0 includes fixes for
RUSTSEC-2026-0194/0195 (older attribute/namespace DoS). This was not a `cargo audit`
run. OSV access returned HTTP 403; no claim of an OSV scan is made. Recheck
advisories when updating versions or preparing a public release.

Conversion reuses these maintained ZIP/XML libraries for writing and parsing;
no complete Office engine is embedded. The earlier Step 6 probes did not add
runtime dependencies; the resumed milestone's stack is recorded below. See
RESEARCH for probe limits and
[base64 upstream](https://github.com/marshallpierce/rust-base64) for codec sources.

## Typography and layout dependencies (Step 6)

| Dependency | Selected version | License | Responsibility |
| --- | --- | --- | --- |
| Parley / Fontique / Parley Engine / Parley Data | 0.13.0 | MIT OR Apache-2.0 | Text analysis, font matching, shaping integration and line fitting |
| HarfRust | 0.15.0 | MIT | OpenType shaping; maintained HarfBuzz port |
| Skrifa | 0.49.0 | MIT OR Apache-2.0 | Font metrics, variations and glyph outlines |
| read-fonts / font-types | 0.46.0 / 0.12.7 | MIT OR Apache-2.0 | Low-level font tables and types |
| ICU4X segmenter/properties/normalizer and data | 2.3.0 | Unicode-3.0 | Unicode analysis/data used by the layout stack |
| ICU provider / locale core | 2.4.0 | Unicode-3.0 | Shared Unicode data infrastructure |
| Parley Emoji | 0.13.0 | MIT | Emoji property/sequence data; separate attribution |
| libm / core_maths | 0.2.16 / 0.1.1 | MIT | Shared math implementation without host math selection |

Direct layout dependencies are pinned and `Cargo.lock` records the resolved
graph. Parley's default `system` feature is disabled; explicit font registration
and `libm` are deliberate native/WASM choices. The stack's highest declared
minimum Rust version is 1.88, below the checked stable workspace toolchain.
ICU4X's grapheme data is Unicode 17; its default line-break constructors used by
Parley implement Unicode 15.1. The optional `complex-scripts` dictionary feature
is not enabled in this initial graph; dictionary-assisted language breaking is
not a general guarantee. This is not a claim of Unicode 18 conformance.
See [research](RESEARCH.md) for alternatives and selected upstream revisions.

The isolated 56-package probe graph's registry manifests/licenses and official
RustSec entries were reviewed at database commit
`4ef2df8e806a65e2f1ecc14f56f51249062b23d6`. Applicable historical records for
hashbrown, once_cell, smallvec, spin and zerovec/derive are patched by the selected
versions; the old spin maintenance advisory is withdrawn. No crate-specific
entry was present for Parley, Fontique, HarfRust, Skrifa or read-fonts at that
commit. All 56 reviewed versions were also found in the current workspace lockfile;
the layout feature tree contains neither host `system` discovery nor optional
`complex-scripts` dictionaries. This was a targeted manual review, not
`cargo audit` or a guarantee of font/parser security. Recheck on updates.

### Bundled font assets

The catalog's unmodified Arimo 1.341, Tinos 1.340, Noto Sans Arabic 2.012,
Noto Sans Devanagari 2.007, Noto Sans Hebrew 3.001 and Noto Emoji 3.002 use
**SIL Open Font License 1.1**. Arimo includes real upright/italic variable faces;
Tinos includes four real static styles. Noto Emoji supplies monochrome outlines,
not a color-emoji renderer. CJK coverage is not bundled. Each exact file's source
revision, byte count and SHA-256 is in
[`fonts/manifest.json`](../crates/office-layout/fonts/manifest.json), alongside
the full family-specific notices.

Licensing was checked independently against official pinned
[Arimo](https://github.com/googlefonts/Arimo/blob/4a6255f269916ae7ad3fc2706b0935e7621396b8/OFL.txt),
[Tinos](https://github.com/googlefonts/tinos/blob/3b4482a99b80ea5fc75f187b1be3120a3f5905b3/OFL.txt)
and [Noto](https://github.com/google/fonts/tree/29b4ec6df2522a8aee279414b78a07910b065874/ofl)
notices. Bundling/embedding requires retaining the copyright and complete font
license texts, including in production browser assets when fonts are embedded in
WASM. Fonts remain OFL-licensed; that requirement does not relicense the engine
or documents produced with them. No separate Reserved Font Name declaration was
found after the copyright in these selected notices. Future modification,
subsetting or conversion needs its own review. The project's license stays
undecided.

## Additional format adapter dependencies (Step 7.5)

The owner's Step 7.5 instruction authorizes these concrete file adapters and
merging after verification. The project license remains undecided. This section
records a targeted dependency review; it is not the planned comprehensive
technical, security or commercial-licensing audit.

| Dependency | Exact selected version | Upstream declaration | Purpose |
| --- | --- | --- | --- |
| pdf-writer | 0.15.0 | MIT OR Apache-2.0 | Bounded low-level PDF objects/streams from existing Rust layout |
| svgtypes | 0.16.2 | Apache-2.0 OR MIT | SVG glyph-path and transform parsing; `libm`, default features off |
| rclip-rtf | 0.1.0 | MIT OR Apache-2.0 | Borrowing RTF byte tokenizer only; default/optional features off |
| rclip-core | 0.1.0 | MIT OR Apache-2.0 | The tokenizer dependency's allocation-free foundational types |

The resolved additional registry graph contains twelve packages. The remaining
eight are arrayvec 0.7.8, autocfg 1.5.1, euclid 0.22.14, kurbo 0.13.1,
num-traits 0.2.19, polycool 0.4.0 and siphasher 1.0.4 (MIT/Apache-2.0
alternatives), plus ryu 1.0.23 (Apache-2.0 OR BSL-1.0). The **Apache-2.0 option**
is selected for these additions, including ryu. Existing ZIP/XML/layout
dependencies are reused; no new frontend runtime dependency or complete Office
engine is added. Optional host/system fonts and RTF codepage tables are not
enabled. Native/WASM checks must establish actual target support rather than
inferring it from a manifest.

The full additional license texts and upstream attributions are retained in
[`THIRD_PARTY_NOTICES.txt`](../crates/office-formats/THIRD_PARTY_NOTICES.txt),
including each selected version, registry package checksum and published VCS
metadata. Its production asset is accessible through **Format library notices**.
Existing font and dependency notices are still applicable; this file is not a
replacement inventory for older dependencies.

**Explicit licensing/maturity limitation:** the published `rclip-rtf`/`rclip-core`
0.1.0 packages and their repository roots at their published VCS commits contain
no standalone license/NOTICE files. Their normalized and upstream manifests
declare `MIT OR Apache-2.0`; the declared Apache-2.0 option is used and the
standard full text is retained. Published author metadata identifies Felix
Schütt. No missing copyright notice was invented and this is not legal clearance.
The source revisions are
[`371c7203faeb3a5f13604a574a7a2e37bba48226`](https://github.com/fschutt/rich-clipboard/tree/371c7203faeb3a5f13604a574a7a2e37bba48226)
for rclip-rtf and
[`2be8cabe96655f98bec3d5dc9781f1713cd46246`](https://github.com/fschutt/rich-clipboard/tree/2be8cabe96655f98bec3d5dc9781f1713cd46246)
for rclip-core. Recheck upstream licensing evidence and the low-maturity parser
dependency during the planned commercial/security audit and on upgrades.

The RTF wrapper does not use the clipboard semantic reader or accept silent
replacement decoding: it adds format/resource/Unicode/unknown-content checks
around the tokenizer. No assertion of a complete `cargo audit`, OSV scan or
global dependency-license clearance is made by this review.

Sources: installed exact-version Cargo manifests and license/attribution files;
[`pdf-writer`](https://github.com/typst/pdf-writer),
[`svgtypes`](https://github.com/linebender/svgtypes),
[`rclip-rtf` package source](https://docs.rs/crate/rclip-rtf/0.1.0/source/),
[`rclip-core` package source](https://docs.rs/crate/rclip-core/0.1.0/source/).

## Frontend development dependencies

| Dependency | Version | License | Purpose |
| --- | --- | --- | --- |
| Vite | 8.3.8 | MIT | Development server and production asset bundling |
| @vitejs/plugin-react | 6.3.0 | MIT | React integration |
| TypeScript | 6.0.3 | Apache-2.0 | Static checking |
| ESLint / @eslint/js | 10.15.0 / 10.0.1 | MIT | Linting |
| typescript-eslint | 8.72.2 | MIT | TypeScript lint rules |
| eslint-plugin-react-hooks | 7.1.1 | MIT | Hook correctness rules |
| eslint-plugin-react-refresh | 0.5.7 | MIT | Component export checks |
| Vitest | 5.1.0 | MIT | Frontend unit tests |
| @playwright/test | 1.65.1 | Apache-2.0 | Real Chromium integration tests |
| fake-indexeddb | 6.2.8 | Apache-2.0 | Isolated storage unit tests |
| @types/react / @types/react-dom | 19.3.0 | MIT | React declarations |
| @types/node | 24.19.2 | MIT | Node build/test declarations |

Node 24 is the shared local/CI runtime. TypeScript 7 was considered but the
current TypeScript ESLint peer range is `<6.1`; TypeScript 6.0.3 satisfies the
published compatibility ranges. Vite, its React plugin, Vitest, and Playwright
support Node 24. These maintained upstream projects are used for their defined
low-level build/test roles.

The transitive npm graph includes MIT, Apache-2.0, ISC, BSD-2-Clause,
BSD-3-Clause, 0BSD, and BlueOak-1.0.0 packages. Two noteworthy build-only cases:

- Lightning CSS and its optional native binaries use **MPL-2.0**. They are
  unmodified build tools, not linked into Writer's browser runtime. Preserve
  their notices/source obligations if redistributing those tools or modifying
  their covered source files.
- `caniuse-lite` browser-compatibility data uses **CC-BY-4.0**. It is build-time
  data; preserve its attribution if redistributing the data or toolchain.

Upstream license texts remain in installed packages. Review distribution notices
again before a public release; no public licensing choice is inferred here.

## Security and supply chain

`npm audit` initially identified the moderate development-only
[minimatch globparts amplification advisory](https://github.com/advisories/GHSA-g3hv-75wg-m249)
through TypeScript ESLint's minimatch 10 dependency. A scoped npm override pins
`@typescript-eslint/typescript-estree`'s minimatch to **11.0.8**. The dependency's
actual use is the unchanged named `minimatch()` API with the `dot` option; the
replacement supports Node 24. Keep the override until upstream's range permits
the fixed major version, and check it when updating the toolchain.
After clean lockfile regeneration, `npm ls --depth=0` and `npm ls minimatch`
passed. `npm audit --json` reported **zero vulnerabilities** across 212 locked
package entries on the review date; this is a point-in-time advisory check.

The binding CLI must match wasm-bindgen **0.2.130**. Generated bindings and
binary assets are build outputs rather than checked-in sources. GitHub Actions
check-out and Node setup actions are pinned to commit hashes. No Cloudflare CLI,
deployment token, telemetry service, or external font service is required.

Sources: package registry manifests and installed license texts;
[React license](https://github.com/facebook/react/blob/main/LICENSE),
[Vite license](https://github.com/vitejs/vite/blob/main/LICENSE),
[TypeScript license](https://github.com/microsoft/TypeScript/blob/main/LICENSE.txt),
[Playwright license](https://github.com/microsoft/playwright/blob/main/LICENSE),
[wasm-bindgen licenses](https://github.com/wasm-bindgen/wasm-bindgen),
[Lightning CSS](https://github.com/parcel-bundler/lightningcss), and
[caniuse-lite](https://github.com/browserslist/caniuse-lite).

## Step 8A raster dependencies (2026-11-18)

| Dependency | Resolved version | License | Role |
| --- | --- | --- | --- |
| png | 0.18.1 | MIT OR Apache-2.0 | Bounded PNG decode and canonical RGBA8 PNG encoding |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | Strict JPEG decode, no host/browser codec |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | JPEG byte reader/options |
| kamadak-exif | 0.6.2 | BSD-2-Clause | Bounded raw TIFF orientation parsing |

New transitive packages are `mutate_once` 0.1.2 (BSD-2-Clause), `adler2` 2.0.1
(0BSD OR MIT OR Apache-2.0), `fdeflate` 0.3.7 (MIT OR Apache-2.0),
`miniz_oxide` 0.8.9 and 0.9.1 (MIT OR Zlib OR Apache-2.0), and `simd-adler32`
0.3.10 (MIT). `office-formats` also directly uses the already locked `flate2`
1.1.11 for PDF raster/alpha compression; no additional compression framework.

All upstream LICENSE files for these new resolved packages are retained in
`crates/office-core/THIRD_PARTY_NOTICES.txt`, linked by About and emitted as a
production asset. This is permissive low-level code with commercial-compatible
license choices, subject to retaining applicable notices; it does not decide this
project's license or complete the planned commercial/security audit.

Defaults are disabled for PNG/JPEG where practical. Mature `image` was considered
but rejected for this narrow profile because of broader unconditional processing
dependencies and bundle cost. The selected EXIF release fixes GHSA-53gf-vqf6-qvp8, which also affects
bounded raw-TIFF parsing in 0.6.1. Regression coverage rejects repeated IFDs in
both PNG and JPEG. It also postdates the old RUSTSEC-2021-0143 container fix. See IMAGES_RESEARCH.md for primary sources and remaining review
limits. No new frontend dependency was introduced.

### Existing development-tool advisory detected during final verification

A fresh `npm audit --audit-level=moderate` on 2026-11-18 failed with 13 high-severity
package findings originating from **one** advisory:
[GHSA-fc5q-39xc-vjpf / CVE-2026-112366](https://github.com/advisories/GHSA-fc5q-39xc-vjpf).
The locked `picomatch` 4.0.7 is a development dependency through Vite, Vitest,
TypeScript ESLint and their glob helpers; no image work added this dependency.
The official advisory lists versions through 4.0.7 as affected and no patched
release; direct registry review also found latest 4.0.7. There is no safe published
version override to apply at this check date. Do not disable or lower the CI audit
threshold to hide this result. Track a verified upstream fix and rerun build,
lint and tests when updating. This finding is separate from the fixed EXIF issue.
`npm audit --omit=dev --json --cache /tmp/graphe-audit-npm-cache` passed with
zero findings; the production tree contains React, React DOM and scheduler.
No document-input-to-glob runtime path was found. That narrower result does not
waive the full development-tool audit failure.

## External font verification

RustCrypto sha2 0.11.1 (default features disabled) supplies SHA-256. New resolved
transitives are digest, block-buffer, crypto-common, cpufeatures, hybrid-array and
typenum. Full upstream license texts are retained in
`crates/office-layout/THIRD_PARTY_NOTICES.txt` and linked by About. Browser hashing
uses WebCrypto, with no new npm dependency. See EXTERNAL_FONTS_RESEARCH.md for
version/security review. Canonical font licenses and provenance remain unchanged.
