# Writer dependency review

Reviewed on 2026-11-14. This inventory records upstream dependency licenses;
the Office engine's own license remains undecided. No Office engine or existing
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
no complete Office engine is embedded. No typography dependency or font asset is
introduced by the retained Step 6 research. Parley/HarfRust/Skrifa remain evaluated
candidates, not committed dependencies. See RESEARCH for probe limits and
[base64 upstream](https://github.com/marshallpierce/rust-base64) for codec sources.

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
