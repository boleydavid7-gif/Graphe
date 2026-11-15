# Writer stability and file-compatibility audit

Scope: `design/writer-lamplight-polish`, PR #17, starting at
`b7944778f686d31672e6fd701b626d8b5237b5ed`, audited 2026-11-15. No merge,
deployment command, live configuration change, or unrelated application change
is part of this work. This is a stability/compatibility review, not completion of
the separate commercial-licensing/security certification.

## Findings and corrections

| Severity | Finding and root cause | Correction / evidence |
| --- | --- | --- |
| Critical for source fidelity | This branch predates PR #18. DOCX copies regenerated the semantic document without consulting the retained source, dropping descriptive/custom properties and safe auxiliaries. | Reused the reviewed source-aware `WriterFile` assessment/export implementation from `23db8ea`, its adapter/WASM/browser regressions, and preservation documentation. Current content/styles are regenerated; safe static properties/auxiliaries are validated and copied. Original bytes stay immutable. Unsafe active graphs remain guarded; meaningful omissions are disclosed. |
| Important | This branch's lengthy Save As consent panel and blanket warnings superseded the approved compact workflow. | Reused PR #18's filename/format/Save copy workflow and meaningful-loss assessments, retaining this branch's theme, inspector, filename and native MIME fixes. Native source copies disclose that originals may contain removed text. |
| Important | Lamplight dialog entrance animation overrides Save As's viewport-centering transform. Reproduced at 390×844: animation `writer-dialog-arrive`, top 422 rather than centered near 282; keyboard occlusion can hide lower controls. | Higher-specificity animation override for the existing Save As dialog. Browser regressions check actual centering and Save copy reachability after visible-viewport resize/pan. Other dialogs retain their existing presentation. |
| Important | SVG pages specified `touch-action: pan-y`, preventing native pinch zoom. Identical Chromium multi-touch gestures left scale at 1 before correction. | Allow `pan-y pinch-zoom`. Regression fails before the fix and passes afterward; native zoom changes without changing Rust text, revision or page geometry. |
| Important | The tablet formatting inspector unconditionally reset the touch-focus guard, so a touch command forcibly refocused the input after a Safari-like blur. | Use the same pointer-type and keyboard-reset policy as the toolbar. Regression reproduces the unwanted focus before correction, then checks selection/formatting preservation and subsequent keyboard activation. Physical keyboard presentation is not claimed. |
| Important for keyboard access | Closing the inspector removed its focused Close button, leaving focus on the document body. | Return focus to the visible Details toggle before unmounting the panel. A real-browser regression failed before correction and verifies the return target afterward. |
| Important | Mobile document-title field used 15px input text, below Safari's customary input autozoom threshold. | Set 16px on small screens. This is a preventive focus/viewport correction; no physical Safari result is claimed. |

No new storage implementation defect was confirmed. No speculative rewrite,
new document model, schema/database migration, dependency or layout engine was
introduced. Preserve existing origin, revision, composition, validation, source,
loss-confirmation and etag guards.

## Format matrix

These results describe Writer's **declared supported profiles**, not arbitrary
files bearing these extensions. See [FILE_FORMATS.md](FILE_FORMATS.md) and
[SAVE_COPY.md](SAVE_COPY.md) for exact preservation/normalization boundaries.

| Format | Open | Edit | Export | Reopen / integrity |
| --- | --- | --- | --- | --- |
| `.writer` | Pass | Pass | Pass | Full supported native model, IDs, metadata, styles and exact originals retained; native bytes/model unchanged by copies. |
| DOCX | Pass within profile | Pass | Pass | Text/effective supported formatting, paragraphs and geometry survive; source-aware static metadata retained where safe. Exact original in native save. |
| ODT | Pass within profile | Pass | Pass | Text/effective supported formatting and paragraphs survive; exact source retained in native save. Unmodeled source properties cannot all be represented in external copies. |
| RTF | Pass within profile | Pass | Pass | Text/effective supported formatting and paragraphs survive; exact source retained in native save. Unsupported semantics remain guarded. |
| TXT | Pass for documented encodings | Pass | Pass | Unicode text and paragraph order survive. Rich formatting is intentionally lost with a brief warning when present. |
| PDF | Rejected: export only | Unavailable | Pass | Four physical pages, page boxes, vector resources, pagination and raster output independently checked. Not a PDF text editor/importer. |

New Rust matrix: 85 paragraphs, multiple pages, a heading style, mixed bold/
italic/underline, fonts/sizes/color, alignment, emoji, combining marks, Arabic,
Devanagari and Hebrew. For every editable format it imports, edits through a
Rust transaction, saves/reloads native state, exports/reopens, and compares
supported effective formatting/text/source identity. External formats may
repartition runs or regenerate IDs; native persistence preserves exact structure.

New browser matrix: five actual UI workflows, each with a 33-paragraph multi-page
document, styles and mixed Unicode/formatting. Open file → edit → native Save →
reload → library Open → original-format Save copy → Open file is verified with
text, effective-formatting and exact original-byte comparisons. Separate browser
contexts avoid relying on the producer's in-memory state or library.

Existing suites additionally cover empty/oversized/malformed/future-version
files, unsupported features, native identity mismatches and failed imports
preserving live text/history. Unsupported content does not become a silently
editable partial projection.

A supplementary local probe reused the previously supplied failing DOCX on the
final branch build: import, edit, native Save, reload/library reopen, all six
exports and all five editable reimports passed, with byte-identical original
retention. `node /tmp/writer-audit-private.mjs` completed successfully. Private
contents/artifacts remain outside the repository and were not printed. This is
additional workflow evidence, not another test-suite pass count.

## Independent consumers and limitations

Actual **LibreOfficeDev 26.8.0.0.alpha0**, commit
`2c87e51eeaa2b413ff4ae097b2705eea1995d8e5`, opened Writer-generated, Rust-edited
DOCX/ODT/RTF documents. Consumer text extraction matched all six Unicode
paragraphs. Inspection of its generated ODT verified bold, italic, underline,
Tinos and 14pt on the edited phrase. This is evidence for that consumer version,
not certification for every Word/LibreOffice release.

**Important compatibility gap:** LibreOffice-resaved versions of those files
are preserved read-only by current importers. Examples include forced DOCX run
RTL (not represented in the model), ODT configuration/default/outline/note
settings, and unsupported RTF formatting/control words. Exact originals and
diagnostics survive. Opening our exports in LibreOffice is verified; unrestricted
editable interchange after another application resaves them is **not** achieved.
Do not relax structural/content guards merely to make this test editable.

Four-page PDF from `tests/fixtures/layout/layout-v1.writer` passed the existing
strict `pypdf` checker and independent Poppler `pdfinfo`/`pdftoppm`. Maximum
page-box error was 0.000025pt. Raster output was inspected. PDF uses Rust's
outlined glyphs: it is not searchable/selectable text, tagged accessible PDF,
PDF/A or PDF/UA. Font coverage/fallback and complex-script/layout limitations
remain documented; unavailable glyphs produce notices. These tests do not
establish Microsoft Word pagination equality.

## Saving and failure safety

Inspection confirms one serialized manual/autosave lane, immutable Rust capture,
composition pause, session/revision/replacement-epoch completion guards and etag
comparison inside the same IndexedDB read/write transaction as the update.
Success is reported on transaction completion, not `put.onsuccess`.

New desktop and touch Chromium regressions abort a real transaction **after**
successful `put`. They verify no premature Saved state, exact previous committed
bytes, preserved live edits, explicit retry and reload/reopen. Further new tests
inject valid record envelopes containing corrupt native bytes or wrong document
identities; opening cannot replace the active document or modify stored records.
Existing tests cover quota/unavailability, reconnect, multi-tab conflicts and
separate-copy recovery, rapid edits, replacement races, IME saves and rejected
import preservation. No weakening of overwrite protection was required.

Prepared exports retain native download/open links after attempted delivery;
blocked/failed handoffs preserve in-memory work and native save state. Browser
APIs offer no receipt proving a file reached the OS Files app/disk. Downloads
never count as a committed IndexedDB save. Eviction, OS termination, battery
loss and lifecycle flush durability cannot be guaranteed by these tests.

## Mobile evidence

Actual headless Chromium at desktop and touch/mobile viewports executes trusted
tap/key events, native multi-touch zoom, selection/format/history workflows,
save/import/export/reopen and bounded scrolling. Controlled event cases cover
Safari-like nonfocusing button activation, composition, clipboard data,
cancellable/uncancellable autocorrect and visual viewport resize/pan/rotation.
These are browser automation/simulation, **not physical Android Chrome, iOS
Safari, a software keyboard, OS clipboard/Files, or an installed PWA**.

**Incomplete functionality:** the SVG editor does not provide touch long-press
selection or draggable selection handles. Keyboard and Rust pointer selection
work; they do not establish those missing mobile capabilities. Physical keyboard
opening/closing, predictive input, handle dragging and PWA/Files delivery remain
unverified. No attached device, macOS or Windows test host is available here.

Public synthetic document screenshots:

- [Desktop](images/writer-audit-desktop.png)
- [390×844 mobile viewport](images/writer-audit-mobile.png)

## Commands and results

Run frontend commands from `apps/writer`; Rust checks from the workspace root.
Environment uses Node24.22.1, npm11.9.0, stable Rust1.100, wasm-bindgen0.2.130
and installed Chromium155. Rust's installed bin directory was put on PATH.

| Command | Executed result |
| --- | --- |
| `npm ci` | Pass; 182 locked packages installed. |
| `npm run build` | Pass; Rust/WASM, TypeScript and production Vite assets built on the final UI fixes. |
| `npm run typecheck` | Pass. |
| `npm run lint` | Pass, zero warnings. |
| `npm test` | 44 passed, zero failed/skipped. |
| `npm run test:wasm` | 53 compiled-WASM tests passed, zero failed/skipped. |
| `npm run test:build` | 2 passed, zero failed/skipped. |
| `npm run test:browser` | Complete configured run: 150 passed, 157 failed, 2 skipped. 156 failures occurred before test execution because Firefox/WebKit executables are unavailable; the one executed Chromium failure was an outdated mobile-chrome expectation, corrected and verified separately. This command did **not** pass. |
| `npm run test:browser -- --project=chromium --output=/tmp/writer-audit-chromium-final-results --reporter=list` | **154 passed**, zero failed/skipped, 7.7min, on the final frozen production build including all inspector corrections. |
| `cargo fmt --all -- --check` | Pass. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass. |
| `cargo test --workspace` | 609 passed including doctests, zero failed/ignored. |
| `cargo build --workspace` | Pass. |
| `cargo check --locked --workspace --target wasm32-unknown-unknown` | Pass. |
| `node apps/writer/tests/layout-check-native.mjs` | Pass on Linux x64: 4 pages / 79 outlines; shared native/WASM geometry fingerprint `7741b146df0ad6128c653190af74ccad0daa8efad249200a82d7c0b9c03b0236`. |
| `npm audit --json` | Pass: zero reported npm advisories; not a RustSec or commercial-license audit. |

`npx playwright install firefox webkit` and a separate
`npx playwright install webkit` failed because their download destinations return
HTTP403 **Domain forbidden**. No Firefox/WebKit editing result is claimed. The
two skips are the explicitly Chromium-CDP-specific pinch cases. A targeted
non-Chromium run verified that they skip before unsupported context creation.

New targeted checks separately passed: five browser file-integrity workflows,
six mobile cases, four storage cases and three inspector cases. These overlap
the full available-engine suite and must **not** be added to its count. The
inspector/tablet and closing-focus regressions failed before the corrections.

Independent consumer probe commands executed with public fixtures:

```sh
node /tmp/writer-audit-independent-formats.mjs
soffice --version
soffice -env:UserInstallation=file:///tmp/writer-audit-lo-profile --headless --convert-to odt --outdir /tmp/writer-audit-independent-formats/lo-odt-from-docx /tmp/writer-audit-independent-formats/audit.docx
soffice -env:UserInstallation=file:///tmp/writer-audit-lo-profile --headless --convert-to odt --outdir /tmp/writer-audit-independent-formats/lo-roundtrip/odt /tmp/writer-audit-independent-formats/audit.odt
soffice -env:UserInstallation=file:///tmp/writer-audit-lo-profile --headless --convert-to odt --outdir /tmp/writer-audit-independent-formats/lo-odt-from-rtf /tmp/writer-audit-independent-formats/audit.rtf
python /tmp/writer-audit-independent-verify.py
```

Each completed successfully. The temporary generation/inspection scripts are
local audit probes, not committed dependencies or a supported application API.
The reusable Rust and browser matrix tests are committed in the repository.
Additional same-format LibreOffice `--convert-to docx`, `odt`, and `rtf`
resaves followed by `node /tmp/writer-audit-independent-reopen.mjs` returned the
read-only diagnoses described above; those are **not editable-import passes**.

Independent PDF commands, all successful (workspace root):

```sh
cargo run --offline --locked --quiet -p office-formats --example export_pdf -- tests/fixtures/layout/layout-v1.writer /tmp/writer-audit-layout.pdf
cargo run --offline --locked --quiet -p office-layout --example layout_snapshot -- tests/fixtures/layout/layout-v1.writer > /tmp/writer-audit-layout.json
python crates/office-formats/tests/check_pdf.py /tmp/writer-audit-layout.pdf /tmp/writer-audit-layout.json
pdfinfo /tmp/writer-audit-layout.pdf
pdftoppm -f 1 -l 1 -r 85 -png -singlefile /tmp/writer-audit-layout.pdf /tmp/writer-audit-layout-page1
pdftoppm -r 85 -png /tmp/writer-audit-layout.pdf /tmp/writer-audit-layout-all
```

All four page rasters were visually inspected, including paragraph continuation,
explicit breaks and the differently sized A4 section. `pypdf` version6.10.0 was
used by the strict checker. This verifies this fixture, not every possible font
or imported document's visual fidelity.

Initial harness/environment corrections are not product passes: an unconfigured
Rust PATH made native/WASM comparison subprocesses fail; subprocess isolation
blocked an initial build-bootstrap test. Both were rerun with the installed
toolchain and appropriate subprocess access. New test assumptions were corrected
for legitimate mobile focus behavior, the native binary download MIME, hidden
desktop Open shortcuts, and warning-free reexports of already-plain TXT. A
concurrent rebuild briefly removed a WASM asset during a probe; final testing
uses a completed, frozen production build. The superseded initial browser run
was interrupted to rebuild confirmed fixes; only completed final runs count.

## Hosted CI investigation

Branch push run [43683271194](https://github.com/boleydavid7-gif/stuff/actions/runs/43683271194)
and earlier PR run [43682686595](https://github.com/boleydavid7-gif/stuff/actions/runs/43682686595)
failed all Linux/macOS/Windows/browser jobs before executing any steps:
`runner_id: 0`, `steps: []`. Log retrieval returned 404; annotations are not
accessible through the available connector. The underlying scheduler/account
cause cannot be established; do not assert billing, platform or source-code
failure without evidence. No workflow defect explaining these pre-runner
failures was confirmed, and no CI step was removed or weakened. Hosted CI did
**not** pass. Local Linux verification does not replace native macOS/Windows runs.

## Release assessment

The final Chromium performance probe measured 30 trusted keys in an
11,840-character/50-paragraph document: p95 committed Rust SVG geometry 96.2ms,
p95 next-animation-frame callback 135.5ms. This is a warm-font desktop benchmark,
not OS input-to-paint latency or mobile performance. Oversize rejection tests
are not evidence of responsive editing for arbitrarily large documents.

Keep PR #17 unmerged and do not deploy as part of this audit. Passing bounded
desktop workflows supports continued private evaluation; it does not establish
general integration/release readiness. Complete real-device Safari/Android/PWA
validation and mobile range selection, decide the required editable profile for
external-producer files, restore executable hosted cross-platform CI, and perform
the planned commercial/security review before a broad release claim.

No additional reproduced critical data-integrity defect remains in the tested
profiles. This is not a guarantee against defects outside those profiles or
unexecuted environments. All audit corrections are committed on the requested
branch; the final correction commit is identified in PR #17 and the delivery
report rather than embedding a self-referential commit hash in this file.

Optional future concern: library metadata listing currently clones full records
with `getAll`, including document bytes; measure large-library behavior before
changing the established storage boundary.
