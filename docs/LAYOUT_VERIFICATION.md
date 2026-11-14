# Step 6 verification — 2026-11-14

Work is on `dev/typography-layout-engine`, based on the completed DOCX
prerequisite `db975fb` in PR #10. The prerequisite passed its push and PR GitHub
Actions runs before Step 6 resumed. Neither branch is merged or deployed here.
The [layout profile](LAYOUT.md), [research](RESEARCH.md) and
[decisions 034–036](DECISIONS.md) describe the implementation and limits.

## Implemented boundary

`office-layout` owns pinned-font resolution, shaping, metrics, kerning/ligatures,
Unicode breaking/bidi, mixed runs, paragraph geometry and pagination. Physical
pages support Letter, A4, custom dimensions/margins and section overrides.
Paragraph continuation, page-break-before, alignment/justification, signed
indentation, spacing and native soft breaks use the existing model.

The revision-bound WASM API returns fixed geometry, shared font outlines,
decorations, explicit logical-position mappings and diagnostics. It is separate
from ordinary editor snapshots and history. The browser page preview draws those
paths without font measurement, wrapping or pagination in JavaScript. The
semantic editor remains available. Shift+Enter uses a validated Rust soft-break
transaction; Enter still creates a paragraph.

## Executed checks

Environment: Linux x86_64, stable Rust 1.100.0, wasm32-unknown-unknown,
wasm-bindgen-cli 0.2.130, Node 24.22.1, npm 11.9.0, Chromium 155.0.8059.79.
Rust commands used these local toolchain paths:

```sh
export CARGO_HOME=/workspace/.office-rust/cargo
export RUSTUP_HOME=/workspace/.office-rust/rustup
export PATH=/workspace/.office-rust/cargo/bin:$PATH
```

Final commands from the repository root:

| Exact command | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed, no warnings |
| `cargo test --workspace` | **399 passed**, no failures or ignored tests |
| `cargo build --workspace` | Passed |
| `cargo check --locked --workspace --target wasm32-unknown-unknown` | Passed |
| `node apps/writer/tests/layout-check-native.mjs` | Linux x64 fingerprint passed |
| `git diff --check` | Passed |

The total is 206 core, 117 DOCX, 35 layout, 36 native WASM-adapter and 5 doctests.
Layout tests include different faces/sizes, variable weight and real italic,
kerning across run boundaries, supported default ligatures, Arabic contextual
shaping, Hebrew/Devanagari/emoji/combining marks, bidi affinities, paragraph and
page geometry, native reopen, control failures and resource bounds. No document,
source, selection or history is changed by successful or failed layout reads.

Commands from `apps/writer`:

| Exact command | Actual result |
| --- | --- |
| `npm run build` | Release WASM, TypeScript and production Vite build passed |
| `npm run typecheck` | Passed |
| `npm run lint` | Passed |
| `npm test` | **20 passed** |
| `npm run test:build` | **2 passed** |
| `npm run test:wasm` | **25 passed**, actual compiled release WASM and generated ABI |
| `npm run test:browser -- tests/layout.spec.ts` | **6 passed**, final production build |
| `npm run test:browser` | **41 passed**, real Chromium, zero failures/skips |

The native helper
`cargo run --offline --locked --quiet -p office-layout --example layout_snapshot -- tests/fixtures/layout/layout-v1.writer`
produces the complete reference JSON. The actual-WASM suite executes this helper
and compares every serialized geometry/outline value byte for byte, then checks
the committed SHA-256 golden. The realistic fixture covers four pages, custom and
A4 geometry, 79 shared outlines, five selected font faces, long-paragraph
continuation, styles, justification, Unicode and page breaks. This proves parity
for the tested profile and inputs, not universal cross-platform determinism.
The standalone native fingerprint check is also configured on every Rust CI
platform, so Linux, Windows and macOS must match that same reviewed golden.

The final full Chromium run took 1.1 minutes. Its existing 35 regressions and
six new layout cases passed after rebuilding the focus fixes. New tests verify
the SVG vector baseline, production font notices, preview/edit transitions,
immediate keyboard input, delayed title typing, formatting and undo/redo updates,
native save/reload/reopen, soft-break identity/offsets, imported DOCX geometry,
structured tab failure, missing-glyph diagnostics and 420 px fit-width behavior.
The [actual desktop preview](images/writer-layout-desktop.png) was refreshed from
that final production build. The 83,850-byte SVG vector baseline checks geometry
and paths rather than assuming identical browser pixel antialiasing. Historical
application screenshots were restored unchanged.

The ABI suite also verifies every bundled font and license against the pinned
provenance manifest using Node's built-in SHA-256 implementation. Production
assets include the canonical 28.1 kB font notice. The release WASM asset is
10,805.72 kB, 5,266.21 kB gzip; the font catalog contributes 6,884,780 bytes before
WASM/bundle compression. No proprietary fonts or complete editing engine were
added. New direct low-level dependencies are Parley 0.13, Skrifa 0.49 and libm
0.2.16, with pinned transitive versions in Cargo.lock. Advisory/license review is
documented in DEPENDENCIES, not represented as an exhaustive security certification.

## Review corrections

Integration and independent tests discovered a panic for valid hanging indents:
the first line's available width exceeded Parley's declared overall maximum.
The maximum now covers all line widths while retaining source indentation.
Underlines also needed their upward font offsets converted to downward page
coordinates. Both defects have independent regression coverage.

Further corrections make sub-resolution line advances explicit errors, bound
labels before amplification, preserve font variations in outline-cache keys and
make diagnostic truncation observable. Exact-height ink overflow, suppressed
page-top spacing, missing glyphs and synthesized/unavailable styles are reported.
The style cascade is shared in Rust with the existing browser projection.

Browser verification also identified preview focus races. Focus restoration now
belongs to explicit preview transitions, without deferred callbacks stealing
typing or title focus during revision refresh. Terminal soft-break DOM markers
add no text or offsets. The final immediate-input and delayed-title browser
regressions pass without waiting for deferred focus restoration.

Initial tests incorrectly assumed this Arabic font had a one-glyph lam-alef
ligature. Actual glyph/GSUB behavior disproved that test assumption; the corrected
tests retain Arabic contextual-joining checks and prove supported Devanagari and
emoji ligatures. No discretionary shaping feature was enabled to force a test
result. Initial local formatting differences were applied with `cargo fmt --all`.
Sandboxed Node subprocess isolation initially prevented test execution; approved
local runs execute the actual engine. Only final executed passes are counted.

## Measurements and limits

Executed `node scripts/measure-layout.mjs` against the release WASM in Node/V8.
These one-off measurements include Rust layout, JSON serialization and ABI copying;
they exclude JSON parsing, network transfer, browser paint and OS input latency.
They are evidence for future optimization, not CI performance thresholds.

| ASCII input units | First layout | Cached JSON copy | Output |
| --- | --- | --- | --- |
| 1,000 | 27.5 ms | 0.8 ms | 256,809 bytes, 1 page |
| 10,000 | 72.5 ms | 7.8 ms | 2,596,711 bytes, 4 pages |
| 50,000 | 295.5 ms | 32.4 ms | 13,119,177 bytes, 18 pages |
| 100,000 | 464.3 ms | 73.5 ms | 26,273,135 bytes, 35 pages |
| 200,000 | 586.7 ms | Not cached | Structured `layout_too_large`; source unchanged |

WASM initialization took 110.1 ms in this local Node run. The first small layout
also initializes the font catalog. Native debug measurement of 200,000 units
including parse/fonts/layout/JSON took 3.576 s, about 96 MiB peak RSS, 69 pages
and roughly 50 MiB output. The browser's 32 MiB response limit intentionally
rejects that extreme snapshot while retaining editable/savable work. Preview is
on demand; these results are not typing-latency measurements. Large full JSON
snapshots and synchronous preview work are important later optimization targets.

The existing browser typing measurement on 11,840 characters, 50 paragraphs and
30 input events measured p50 2.4 ms, p95 5.3 ms and maximum 6.3 ms for synchronous
Rust transaction/DOM reconciliation. This also excludes operating-system input
and paint; it does not turn the preview benchmark into a typing benchmark.

Tabs currently return `unsupported_layout`; no tab-stop approximation is made.
CJK coverage, color emoji, all script/style combinations, dictionary breaking,
hyphenation, widow/orphan/keep rules, advanced sections and Word pagination remain
limited or absent. Native font/style requests and original bytes are preserved.
Writer defaults for unspecified spacing and exact-box ink placement differ from
Word in documented cases. Future source-default materialization belongs in the
DOCX adapter. No major model rewrite was introduced to conceal these limits.

Not performed: Microsoft Word runtime/pagination comparison, full Unicode or
upstream shaper conformance suites, other browser engines, manual OS IME or
assistive-technology testing, or universal native-platform layout parity. The
existing editor's browser CSS is not replaced by a page-based editing surface.
That requires Step 7 interaction/accessibility work and must not be inferred from
the read-only preview. No unresolved critical defect remains from the completed
native/ABI/browser audit. Final remote CI outcomes are recorded on the PR checks.

GitHub Actions checks the three-OS engine matrix and complete WASM/frontend/browser
pipeline. Remote results must be read from the final commit's checks. Building
locally does not establish Cloudflare deployment or private-access restrictions.
