# DOCX conversion verification — 2026-11-14

Branch: `dev/docx-conversion`, based on main `a504e2f`. The owner directed us to
complete missing semantic DOCX conversion before Step 6. This work does not
implement typography, merge to main or deploy. The supported profile and
preservation rules are in [DOCX_CONVERSION.md](DOCX_CONVERSION.md).

## Executed checks

Linux x86_64, stable Rust 1.100.0, wasm32-unknown-unknown, wasm-bindgen-cli 0.2.130,
Node 24.22.1, npm 11.9.0, Chromium 155.0.8059.79. Rust commands used:

```sh
export CARGO_HOME=/workspace/.office-rust/cargo
export RUSTUP_HOME=/workspace/.office-rust/rustup
export PATH=/workspace/.office-rust/cargo/bin:$PATH
```

Before changes, all four engine checks passed: 256 tests including doctests,
formatting, strict Clippy and workspace build. Remote main matched `a504e2f`;
inspection confirmed that only Step 5A, not semantic import/export, existed.

Final commands from the repository root:

| Exact command | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed, no warnings |
| `cargo test --workspace` | **355 passed**, zero failures/ignored |
| `cargo build --workspace` | Passed |
| `cargo check --locked --workspace --target wasm32-unknown-unknown` | Passed |
| `git diff --check` | Passed |

The Rust total is 206 core tests, 117 DOCX tests, 27 native WASM-adapter tests
and 5 doctests. It includes strict legacy migration, generic page geometry,
native original preservation, filesystem failure cases, XML/property mappings,
style toggles, sections, deterministic export, unsupported-content gates and
failure atomicity. Six new adapter tests verify source lifetime, native reopen,
history/fork/New behavior, edited Word export, source revalidation and complete
source-bearing byte budgets before commit.

Final commands from `apps/writer`:

| Exact command | Actual result |
| --- | --- |
| `npm run typecheck` | Passed |
| `npm run lint` | Passed |
| `npm test` | **20 passed** |
| `npm run test:build` | **2 passed** |
| `npm run build` | Release Rust WASM, TypeScript and production Vite build passed |
| `npm run test:wasm` | **12 passed**, actual compiled WASM/ABI without an engine mock |
| `npm run test:browser -- tests/docx.spec.ts` | **8 passed**, actual Chromium |
| `npm run test:browser` | **35 passed**, actual Chromium against production preview |

The complete browser run took 41.4 seconds. New cases verify that editing waits
for the actual IndexedDB preservation transaction to complete, rather than just
its request; supported import; quota/unsupported failures preserving prior work;
native save/reopen; a fresh browser process; exact original recovery; native
backup disclosure; regenerated current-content Word export; and distinct IDs for
repeated imports. Existing editing, Unicode, formatting, undo/redo, conflicts,
clipboard, IME and responsive regressions also passed.

On 11,840 characters/50 paragraphs, 30 synchronous input events measured p50
2.6 ms, p95 3.6 ms, maximum 6.1 ms. This measures Rust transaction/DOM
reconciliation, not operating-system input-to-paint latency. Retained originals
are not re-encoded into base64 on each edit; cached envelope overhead participates
in the pre-commit size check. Final WASM was 1,602.87 kB (512.18 kB gzip), JS
261.22 kB (80.75 kB gzip). Generated assets remain ignored build outputs.

The actual [DOCX import screenshot](images/writer-docx-desktop.png) was captured
in Chromium from the production build at a 1440×1000 viewport using
`supported-formatting.docx`. It shows imported mixed formatting/Unicode, Word
export, original recovery and saved-source status. Historical screenshots remain
unchanged.

## Independent application checks

Used **LibreOfficeDev 26.8.0.0.alpha0**, build `2c87e51…`, as an external test
application only. No Office engine was embedded or added as a dependency.
Isolated reference artifacts remained under `/tmp/writer-docx-export-interop`.
Executed:

```sh
cargo run --offline --manifest-path /tmp/writer-docx-export-interop/Cargo.toml
soffice -env:UserInstallation=file:///tmp/writer-docx-export-interop/profile --headless --convert-to fodt --outdir /tmp/writer-docx-export-interop/converted /tmp/writer-docx-export-interop/current.docx
python3 /tmp/writer-docx-export-interop/verify.py current
```

A separate `portable.docx` case used the same conversion and
`python3 /tmp/writer-docx-export-interop/verify.py portable`. Both opened with
the expected three paragraphs, Unicode/combining/emoji/scripts, soft line breaks,
tabs, direct formatting, alignment, spacing, Letter/A4/landscape dimensions and
margins (within one twip of reference conversion). Nonfatal font-cache warnings
did not prevent conversion. FODT is a reference artifact, not an added format
adapter or user-facing export feature.

The first CR-containing reference failed paragraph-count fidelity: XML round
trips preserved the character but the application introduced another paragraph.
Independent probes also found default-space boundary trimming and literal LF/TAB
differences. The importer now returns preserved read-only outcomes for ambiguous
cases, and DOCX export rejects literal CR rather than changing its meaning.
Explicit DOCX soft-break/tab elements remain supported. Native CR data is unchanged.

LibreOffice also differs from Microsoft's documented style-toggle rules in a
tested inherited-style case. The adapter retains standards-based toggle
translation; this is not evidence of identical behavior in every application.

An additional `python3 /tmp/docx-property-semantics/probe.py` generated a nine-
paragraph reference and converted it with `soffice --headless
-env:UserInstallation=file:///tmp/docx-property-semantics/profile --convert-to fodt
--outdir /tmp/docx-property-semantics /tmp/docx-property-semantics/inheritance.docx`.
LibreOffice inherited exact/minimum rules for a line value without a rule,
while using multiple spacing without a parent. The strict editable profile now
requires complete `line`/`lineRule` pairs rather than assuming cross-application
inheritance. Opposite first-line/hanging overrides and partial left/right
indentation behaved as expected and needed no code change. This does not claim
a Microsoft Word runtime result or a defect in the normative specification.

## Review corrections and limitations

Independent review corrected whitespace/control fidelity gates and strengthened
the source constructor's actual retained-byte invariant. Regression tests cover
these cases. No remaining confirmed critical issue was identified in the review.
Other implementation failures were corrected before final checks: response-enum
Clippy sizing, missing Node test imports, and legacy version assertions. Two new
browser tests initially used stale fixture/inspection assumptions; those tests
were corrected and the complete suite reran successfully.

Legacy decoding reuses only currently unchanged text-run, character-format and
metadata records. Future fields in those types require frozen legacy definitions
and regressions before accepting a new schema; the source comment records this
maintenance constraint.

Initial sandbox browser execution could not start preview, so no test ran in
that attempt. The original commands subsequently executed successfully with
local permission. All final tables above describe executed passing checks.

Not performed: Microsoft Word testing, native Windows/macOS execution in this
Linux workspace, other browser engines, manual OS IME/assistive technology,
full OOXML schema certification, exhaustive interoperability, or Word-equivalent
typography/pagination. The format profile is deliberately strict: many ordinary
Word files use unsupported themes, parts, properties or content and therefore
remain preserved read-only. It is not general Word compatibility.

Full edited DOCX reconstruction is provided only for the declared supported
model. Native IDs, exact run partitioning, unspecified-property distinctions
and future style-editing intent are not universal DOCX guarantees. Empty
nonfinal native sections and XML-illegal text are explicit export errors. A
fully empty Word body with explicit defaults remains read-only because the
model has no global default-style selection for future first typing.

Native files retain original text/metadata even when later removed from the
editable document. The UI discloses this; original recovery and regenerated
Word export are distinct. Unsupported raw DOCX is retained only in the current
read-only dialog/file buffer, not silently saved to the library. The original
input file is never overwritten. Supported DOCX is saved locally before editing;
native Writer imports retain their explicit-save behavior.

The 5 MiB browser limit includes the native document, source metadata and base64
original, even though generic native APIs allow 32 MiB files/16 MiB originals.
No cloud storage, deployment configuration, additional format adapter or
typography dependency was added. Upstream licensing review adds only scalar
`base64 =0.23.1`; this does not select the project's pending license.

GitHub Actions runs the three-OS engine matrix, WASM adapter check and complete
frontend/browser pipeline on the review branch. Remote CI must be read from
the final commit's checks; local execution above is not an inferred CI result.
