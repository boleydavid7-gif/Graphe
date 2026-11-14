# Step 7 verification

This report covers `dev/writer-lamplight`, based on merged Steps 1–6 at
`86f2ad8`. No Step 7 merge or deployment is authorized by this implementation.
The planned comprehensive technical, security, DOCX and commercial-licensing
audit follows approval; this report does not claim to perform that audit.

## Requirement review

| Capability | Implementation and evidence | Practical limit |
| --- | --- | --- |
| Lamplight interface | Actual desktop/mobile captures below; original references preserved in `docs/design/` | Phone uses real physical-page scaling; Fit width is an overview |
| Typing, deletion, paragraphs | Browser input commits existing Rust transactions; real keyboard and page-pointer tests | Semantic input proxy buffers uncommitted composition |
| Pagination, caret and selection | Rust glyph paths, hit tests, visual navigation and selection rectangles | Tabs unsupported; finite fonts; large snapshots bounded |
| Formatting | Existing bold/italic/underline, font family/size, alignment and named style assignment | No style creation, templates or advanced document objects |
| Unicode | Core grapheme/UTF-8/UTF-16 mapping, mixed bidi, combining marks, emoji and complex-script regressions | Pinned font coverage; monochrome emoji, no CJK fonts |
| Copy/cut/paste | Existing Rust edit path, cut only after clipboard success | Plain text rather than rich HTML clipboard |
| Undo/redo | Core history; each transaction remains one command | Browser retains 20 snapshots, without typing grouping |
| New/Open/rename/Save | Existing native serialization and IndexedDB etag transactions | Browser-local data may be evicted; export backups |
| Autosave | Shared manual/automatic lane, 750 ms inactivity / 5 s maximum wait, errors and conflicts retain dirty work | Background/shutdown saving is best effort |
| Native and DOCX workflows | Native save/reopen/export, supported DOCX conversion, exact-source preservation and read-only unsupported cases | Declared Step 5 DOCX profile; no general Word fidelity claim |
| Find | Rust literal case-sensitive results, whole-grapheme selections and paged reveal | Bounded results; fallback Text view lacks geometry-based reveal |
| Accessibility and mobile | Labels, keyboard controls, semantic input projection, visible focus, emulated narrow layouts | Manual screen readers, OS IME and mobile selection handles remain unverified/incomplete |

No alternate JavaScript document, layout or undo engine was introduced. No
dependencies, lockfile changes, native format/schema changes or database schema
changes were needed. Source-bearing documents retain existing preservation and
document-only loader protections.

## Executed local checks

Environment: Linux, stable Rust 1.100.0, matching wasm-bindgen-cli 0.2.130,
Node 24.22.1/npm 11.9.0 and system Chromium 155. Rust commands run from the root;
npm commands run from `apps/writer` against generated release WASM. Custom local
`CARGO_HOME`/`RUSTUP_HOME` select the installed toolchain, not different sources.

| Exact command | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace` | 442 passed; zero failed or ignored |
| `cargo build --workspace` | Passed |
| `cargo check --locked --workspace --target wasm32-unknown-unknown` | Passed |
| `node apps/writer/tests/layout-check-native.mjs` | Passed; complete geometry/outline golden unchanged |
| `npm run typecheck` | Passed |
| `npm run lint` | Passed with warnings forbidden |
| `npm audit --audit-level=moderate` | Passed; registry reported zero vulnerabilities (not a comprehensive security audit) |
| `npm test` | 42 passed, including 22 autosave regressions |
| `npm run test:build` | 2 bootstrap tests passed |
| `npm run test:wasm` | 33 passed against actual compiled release WASM |
| `npm run build` | Passed release WASM, TypeScript and production Vite build |
| `npm run test:browser -- --project=chromium` | 65 passed in 2.3 minutes; zero failed or skipped |

After the Find-only scroll correction, `npm run build`, `npm run typecheck`
and `npm run lint` passed again. The targeted command
`npm run test:browser -- tests/step7.spec.ts --grep 'Find reveals an offscreen' --project=chromium`
passed its one case before the complete 65-test rerun. The compiled WASM was
unchanged by that presentation-only correction.

The browser suite serves the actual production bundle through Vite preview;
engine or layout mocks do not substitute for Rust. It checks real keyboard and
pointer editing, Unicode selections, multi-page rendering, formatting, history,
save/reload/reopen, native and DOCX files, source retention, autosave deadlines,
IME suspension, conflicts, quotas and failure recovery. Programmatically induced
clipboard/composition failures exercise recovery, not every OS/browser behavior.

An initial frontend baseline batch did not execute because automatic approval
review hit an account usage limit. Later approved builds and the tests reported
above did execute. Earlier partial browser runs exposed both outdated harness
assumptions and real focus/reveal problems; their failures are not counted as
passes. The preceding full run had 64 passes and one failed Find reveal
expectation: the result was barely visible at the bottom while another page
dominated the viewport. Explicit offscreen Find now centers its Rust caret;
ordinary typing retains minimal scrolling. The final full run passed all 65.

## Browser compatibility and CI

Local `npx playwright install firefox webkit` and a separate WebKit installation
attempt were authorized but all official mirrors returned HTTP 403
(`Domain forbidden`). The four Firefox/WebKit smoke cases **could not run
locally**. No alternate download/proxy bypass was used. CI installs all three
engines: full Chromium coverage and two portable editing/persistence tests each
in Firefox and WebKit. The review PR's checks are the source for final-commit
remote results; configured tests alone are not evidence that they passed.

Manual OS IME, real touch hardware, screen readers, exhaustive browser/OS
compatibility and comparisons against Microsoft Word remain untested. Synthetic
IME, actual WASM, automated Chromium and emulated mobile checks do not certify
those capabilities.

## Defects corrected and regression evidence

- RTL line-transition direction now follows shaped endpoints and explicit
  affinity. Native tests cover multi-page walks, wrap boundaries and bidi ranges.
- Page hits carry their displayed session/document/revision through the client
  and native guard. Canceled touches do not mutate selection or focus.
- Layout refresh cannot steal title/Find focus. New/Open/import commit focus
  synchronously; pointer Save preserves ongoing writing focus.
- Reading-preview status follows its visible pages. Find leaves preview for
  editing and reveals native selection geometry without stealing query focus.
- Autosave has a maximum wait, immutable bytes, serialized etag writes, stale
  completion guards, explicit error recovery and active-composition suspension.
- Ordinary typing previously produced one run per key: a failing 1,000-key
  regression demonstrated the budget risk. Exact-format collapsed insertion now
  extends only its nonempty left prefix. Eight native regressions and real WASM
  verify one resulting run, Unicode, retained source partitions and undo/redo.
- Exact rendering memo comparisons cover all rendered glyph/outline attributes;
  unchanged pages retain their glyph subtrees without a second layout model.

These are targeted corrections. They do not regroup undo, normalize loaded run
partitions, change serialization or add speculative architectural layers.

## Performance sample

Two sequential warm-font samples used the same 11,840-character/50-paragraph
document and 30 trusted keys, awaiting every visible revision. Timings begin at
`beforeinput`; the DOM endpoint is the committed SVG revision with ready state.
The second endpoint is its next animation-frame callback, **not painted pixels**.

| Measurement | Before: median / p95 / max | After: median / p95 / max |
| --- | --- | --- |
| Committed SVG DOM | 94.0 / 132.9 / 136.6 ms | 90.6 / 109.0 / 113.3 ms |
| Next animation-frame callback | 120.6 / 162.2 / 171.8 ms | 124.8 / 144.4 / 151.8 ms |

The measured tail improved; medians were similar and the frame median increased.
One machine and 30 events do not establish a general speedup. Approximately
0.1-second updates at this size remain an important performance limitation for
the later audit. Synthetic Rust/input-only timings omit visible layout and must
not be presented as complete typing latency. No hardware-dependent latency
threshold was added to CI.

## Actual application screenshots and design comparison

- [Desktop, 1280×720](images/writer-lamplight-desktop.png)
- [Mobile, 420×844](images/writer-lamplight-mobile.png)
- [Additional desktop editing evidence](images/writer-lamplight-editing-desktop.png)
- [Additional mobile editing evidence](images/writer-lamplight-editing-mobile.png)

The primary captures show the same real `project-notes.writer` fixture imported,
rendered by Rust and saved locally. The original desktop/mobile reference pixels
are preserved in PNG; see [design notes](design/README.md) for source dimensions.
The implementation follows the compact header/tab/toolbar hierarchy, flat warm
surfaces, lantern, gold accent and white page. Necessary differences are device
save instead of cloud, bundled fonts instead of Calibri, supported commands only,
actual page geometry, a persistent status bar and honest mobile zoom. The full
page is small at phone Fit width; larger zoom pans inside the workspace.

## Review disposition

The final local browser gate passed; final-commit CI is required before approval.
No known unresolved document-integrity defect has been identified by the targeted reviews. Important
remaining limits include full-layout transport/rendering cost, bounded history,
font/tab coverage, advanced accessibility and real mobile/IME certification. See
[the contract](WRITER_STEP7.md) for the complete supported scope. Do not begin
Step 8, merge or deploy automatically.
