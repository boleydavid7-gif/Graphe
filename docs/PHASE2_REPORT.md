# Writer stability and compatibility: Phase 2 report

Work stays on `dev/writer-stability-phase2`, based on latest audited PR #17 head
`e501d76bf58396e5f4fa10833f8292b830326e4b`. Starting main was `a5fbe23`; it later
advanced independently with CSS-only Lamplight work. No rebase, merge, deployment
command, live configuration change or unrelated application change was made.
PR #17 remains draft/open. The shared Lamplight bridge is preserved.

## Work areas, handled in order

| Area | Confirmed problem and implementation | Verification and remaining limits |
| --- | --- | --- |
| Mobile selection | SVG pages lacked touch word hold/endpoint handles. Add Rust-authoritative word and directed-endpoint actions, projected handles, scrolling/zoom handling and guarded plain-text clipboard controls. No JS text model/history. | Native/WASM/real Chromium touch cases pass. Physical iOS/Android/PWA, OS keyboard/clipboard/loupe and assistive behavior remain unverified. [Details/files](PHASE2_MOBILE_SELECTION.md). |
| External compatibility | Routine producer settings and active run RTL were treated as content blockers. Classify exact known inactive settings and disclosed formatting approximations, with independent notices/blockers and exact originals. Extend meaningful copy warnings across formats. | Actual audited LibreOffice DOCX/ODT resaves now editable. Known RTF formatting improves; complex-script RTF and other ODT variants remain read-only. Independent LibreOffice consumption passes for edited exports. [Details/files](PHASE2_COMPATIBILITY.md). |
| PDF investigation | Existing export has no text. Evaluate six Type 3 variants, exact shaping provenance and mature alternatives; reject the experiment after independent rendering failures. | Production PDF/layout bytes are unchanged and nine PDF tests pass. Search/select remain unfinished; no tagged/PDF-A/accessibility claim. One generated variant was not independently tested. [Evidence](PHASE2_PDF.md). |
| Performance | Metadata listing eagerly cloned all bodies and made redundant owned copies. Measurements justify cursor traversal plus shared validation without metadata-side copies. Keep load/save ownership and transaction completion. Add manual reproducible probes. | Four new browser workflows and three unit regressions pass. Library costs improve; large-document layout/transport/SVG latency remains important and needs reviewed follow-up. [Details/files/data](PHASE2_PERFORMANCE.md). |
| Cross-platform CI | Hosted jobs fail before runner allocation; diagnostic annotations are inaccessible. Keep every job/test and add the twelve new portable cases to Firefox/WebKit coverage. Retry browser acquisition and launch probes. | Hosted restoration remains blocked, cause unclassified. Firefox/WebKit downloads return 403; no application tests execute in those engines. [Exact evidence/actions](PHASE2_CI.md). |

Critical integrity corrections include rejecting malformed ODT text in property,
page-layout and master-page containers; checking settings namespaces/shapes,
property-child context, script-font references and signed integer bounds before
broader normalization. Saturated notices cannot conceal later content/protection
findings. Regression coverage accompanies each correction. There are no known
unresolved critical production defects established by the executed checks; this
does not certify untested platforms or general file-format fidelity.

No production dependency, database migration, core/native schema change,
independent frontend document/layout model or undo history was introduced. Native
IDs, exact originals, source-aware DOCX copies, serialized saves, revision/etag
checks, IME safety, failed-import preservation and meaningful-loss confirmation
remain protected. The rejected PDF prototype and its metadata are not shipped.

## Format workflow matrix

These results apply to the declared supported profiles and tested fixtures,
not arbitrary files sharing an extension. External formats can repartition runs
or regenerate IDs; native persistence preserves full supported structure and IDs.

| Format | Open/edit | Native save/reload | Export/reopen |
| --- | --- | --- | --- |
| `.writer` | Pass | Exact supported model, identifiers, styles, metadata and original bytes | Pass. |
| DOCX | Pass within profile; audited LibreOffice resave now editable with notices | Exact original retained | Supported text/formatting survive; validated safe source auxiliaries retained; known approximations disclosed. |
| ODT | Pass within profile; audited resave now editable with notices | Exact original retained | Supported text/effective formatting survive; unsupported authored metadata, active outline/notes/list content and unfamiliar settings remain guarded. |
| RTF | Pass within profile, including new validated language/font/layout controls | Exact original retained | Supported text/formatting survive. Complex LibreOffice script/font/list/note cases remain preserved read-only. |
| TXT | Pass for strict supported Unicode/BOM encodings | Exact nonempty original retained | Unicode text preserved; meaningful rich-format loss disclosed. |
| PDF | Explicitly export-only | No editable PDF import | Rust pages/vector output verified. Search/select, tagging and archival conformance are not implemented. |

Independent LibreOfficeDev 26.8.0.0.alpha0, commit
`2c87e51eeaa2b413ff4ae097b2705eea1995d8e5`, consumed the edited DOCX/ODT/RTF
exports: exact Unicode paragraphs and tested bold/italic/underline/Tinos/14pt
survived. No actual Microsoft Word runtime is available. See the compatibility
report for commands, source hashes and remaining profiles.

## Final available checks

Cargo/parity commands run from the root; npm commands from `apps/writer`.
Stable Rust 1.100, Node 24, matching wasm-bindgen 0.2.130 and Chromium 155 were used.

| Exact command | Actual result |
| --- | --- |
| `npm ci` | Passed; locked dependency installation. |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed. |
| `cargo test --workspace` | 650 passed, zero failed/ignored. |
| `cargo build --workspace` | Passed. |
| `cargo check --locked --workspace --target wasm32-unknown-unknown` | Passed. |
| `npm run typecheck` | Passed. |
| `npm run lint` | Passed, zero warnings. |
| `npm test` | 64 passed, zero failed/skipped. |
| `npm run test:wasm` | 65 passed, zero failed/skipped. |
| `npm run test:build` | 2 passed, zero failed/skipped. |
| `npm audit --audit-level=moderate` | Passed, zero reported vulnerabilities; not a security/commercial audit. |
| `npm run build` | Passed. |
| `npm run build:cloudflare` | Passed; build only, no deployment. |
| `node apps/writer/tests/layout-check-native.mjs` | Passed; original four-page geometry/outline fingerprint unchanged. |
| `npm run test:browser -- --project=chromium --output=/tmp/writer-phase2-final-browser-results --reporter=list` | 183 passed, zero failed/skipped, 10.1min. Actual frozen production bundle. |

Final available-suite logs are `/tmp/writer-phase2-final-*.log`. Targeted tests,
earlier full runs and manual benchmark/consumer probes overlap these results or
provide separate evidence; they are not added to suite pass totals. Intermediate
test-driver failures and the rejected PDF experiments are disclosed in their
work-area reports. No unexecuted test is counted as passed.

Firefox discovery lists 85 tests and WebKit 103; neither full project executes
because required binaries could not be downloaded. Launch probes fail before
app assertions; remaining probe cases did not run. Hosted Linux/Windows/macOS/
Writer jobs have zero runners/steps and cannot be treated as executed checks.
There is no physical-device, installed PWA or OS input-to-paint claim.

## Measured performance and readiness

Final actual-source interleaved library medians improve from 111.2 to 66.6ms
(1,000 small files), 210.8 to 154.3ms (100 about-1MiB files), and 483.5 to 302.2ms
(200 large files). Observed binary backing deltas reduce too; they are controlled
samples, not total memory/absolute peaks. Small libraries may be slightly slower.
All records are still read; there is no metadata-store migration.

Large-document key-to-committed-SVG p50/p95 is 172/188ms at ten pages,
360/460ms at 29 and 486/611ms at 42. The 42-page example has 251,222 DOM nodes.
All sixty measured keys survive; the response-limit failure preserves native work.
These are important performance limits. Review bounded incremental layout,
transport and viewport presentation with the committed probes before a broader
architectural change. Existing memoization remains.

Code improvements are ready for review, **not a general release certification**.
Required follow-up: obtain/fix the actual hosted-run annotation cause, provision
Firefox/WebKit and run their complete suites, and test selection/keyboard/IME/
save workflows on physical iOS/Android/PWA. Searchable PDF needs a separately
reviewed exact-instance CID/subsetting design; tagging/PDF-A are separate.
The planned comprehensive commercial/security audit remains pending. No merge,
deployment, ProairetOS integration work or subsequent feature milestone begins
automatically.

Implementation commits: `2bccd2c` selection, `2a6b2cf` compatibility,
`9648a46` PDF findings, `6f0bc41` measured storage/performance. CI coverage and
this report are a separate focused review commit. Final SHA and any hosted retry
outcome are reported with delivery.


## Approved main reconciliation verification

The owner explicitly approved merging the audited branch into main. First
reconciled audited `f33e086` with main `efde8a8`, preserving the full Phase 2 tree
and main's trusted parent bridge, embedding headers, root assets and mobile theme
rules. Excluded selection handles from the added generic themed hover rule.
Corrected the obsolete subpath assertion/docs to match main's deliberate root
asset restoration; included those checks in `test:build` after the CI build.

Actual combined-tree commands (Cargo from root, npm from `apps/writer`):

| Command | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed. |
| `cargo test --workspace` | 650 passed, zero failed/ignored. |
| `cargo build --workspace` | Passed. |
| `cargo check --locked --workspace --target wasm32-unknown-unknown` | Passed. |
| `npm run typecheck` | Passed. |
| `npm run lint` | Passed after correcting an inherited missing Node URL import. |
| `npm test` | 64 passed, zero failed/skipped. |
| `npm run test:wasm` | 65 passed, zero failed/skipped. |
| `npm run test:build` | 4 passed, zero failed/skipped, including real built asset/WASM checks. |
| `npm run build` | Passed; actual Rust/WASM production bundle. |
| `node apps/writer/tests/layout-check-native.mjs` | Passed with configured Cargo environment and subprocess access; original four-page fingerprint unchanged. |
| `npm run test:browser -- --project=chromium --output=/tmp/writer-main-final-browser-results --reporter=list` | 187 passed, zero failed/skipped, 10.5min. |

The first full Chromium attempt had 182 passes and one test synchronization
failure. Its trace showed a raw DOM cursor assigned during pending undo layout;
layout then restored the authoritative Rust cursor. The test now awaits layout
readiness and verifies both Rust and DOM UTF-16 endpoints. The unchanged engine
passed five repetitions of that case and the complete final 187-case run above.
Targeted repeats overlap the complete suite and are not extra suite passes.

New embedding tests serve the unchanged production preview through intercepted
HTTPS origins matching the intended same-site host topology. An initial HTTPS
to HTTP loopback fixture was blocked by browser local-network protection; no
browser protections were disabled. An unrelated synthetic cross-site origin
showed a storage failure, while the intended same-site fixture passed saving,
reload and reopening. These tests do not verify live hosting/access restrictions,
physical devices, or every third-party storage policy.

Logs: `/tmp/writer-main-integration-*.log`, `/tmp/writer-main-unicode-repeat.log`,
`/tmp/writer-main-embedded-*.log` and `/tmp/writer-main-final-browser.log`.
Rust layout/model/editing/storage implementations remain the audited versions.
No new dependency, native schema or database migration was introduced.
Historical audit screenshots are retained; incidental screenshot rewrites from
repeat test execution are excluded from the merge.

Main advanced during verification with PR #25/#26 (`2ad7865`); their final
integration and affected-suite results are recorded separately below. The
187-case result above applies to the `efde8a8` reconciliation, not those later
UI commits. Hosted CI, unavailable browser engines and real-device limitations
remain as disclosed in this report and PHASE2_CI.md; no unexecuted check is a pass.
No deployment command was run.
