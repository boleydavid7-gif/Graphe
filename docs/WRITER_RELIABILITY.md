# Save, export and mobile-input reliability

Review branch: `fix/writer-save-export-mobile-reliability`, based on main
`8ed5144578dfa7e7ec38a4c37e568fb9ee04413c`. This is a focused correction;
no deployment, merge, new format, schema change or feature milestone is included.

## Confirmed problems and corrections

| Problem | Correction and regression coverage |
| --- | --- |
| A rejected IndexedDB open remained cached, making later Save ineffective. Unexpected connection closure permanently disabled the library. | Clear only the failed/abnormally closed connection; retry uses current Rust bytes and existing etags. Unit and real IndexedDB desktop/touch save-reload-reopen tests. Explicit close/version changes remain terminal. |
| A replacement blank document's Save could join an old session's exhausted two-attempt drain and finish without writing. | After the old drain, start the replacement epoch's bounded request if still current and error-free. Unit test and real delayed-transaction browser test, including reopen. One serialized write lane remains. |
| Ctrl/Cmd+Shift+S reset an active format review; Open could stack another modal. | Suppress application file/find shortcuts while a modal is open. Preserve format, filename and acknowledged warnings. |
| Detached download anchors and one-second Blob revocation offered a fragile browser handoff with no retry. | Connected anchor in the active modal; one immutable prepared copy with native Download file/Open copy links. Release on replacement, dismissal or unmount. Status says ready, not delivered to disk. Local save status is unchanged. |
| Trusted touch tap could focus the input on pointerup and lose focus through compatibility events; stale layout could prevent focus entirely. | Synchronous completed-tap focus plus compatibility-click recovery. Stale hits keep the current Rust selection. Scroll/cancel does not focus. The same nonzero-size input survives pagination rerenders. |
| Proxy/reveal used the layout viewport while a keyboard can shrink/pan the visible viewport. | Observe VisualViewport resize/scroll, clamp input and reveal Rust caret; add bounded trailing padding on the inner editor, not the workspace flex item. Regression covers shrink, pan, near/complete occlusion, rotation and restoration. Independent review caught and corrected padding feedback and retained flex minimum on rotation. |

Rust still owns edits, history, document validation, format conversion and page
geometry. There is no JavaScript document model or undo history. Existing dirty,
composition, stale-origin, original-preservation, conflict and conversion-warning
guards remain active. Storage errors retain in-memory work and pause automatic
retry. A downloaded copy neither commits an IndexedDB record nor changes its etag.

## Workflow coverage

The production bundle is tested through actual Chromium browser input, dialogs,
IndexedDB transactions, downloads and file choosers. Each of `.writer`, DOCX, ODT,
RTF and TXT is exported and reopened in a fresh browser context, retaining sample
Unicode text. PDF is export-only; tests check real downloaded PDF structure.
All six formats test desktop and touch handoff, MIME type, Unicode filename,
delayed native-link retry and byte equality. Existing tests cover editing after
import, mixed formatting, undo/redo, exact originals and format-loss disclosures.

The owner's previously supplied DOCX was also opened through the production UI,
edited, saved locally, reloaded and reopened. Save As exported all six formats;
all five importable formats reopened with the edited text unchanged. Native
export retained the byte-identical original. PDF exported successfully. Private
source/outputs remain outside Git in `/tmp`; committed regressions use public
synthetic content. This verifies that file, not arbitrary DOCX fidelity.

Fault injection is explicit: storage open/abnormal close, delayed real
transactions, blocked automatic anchor click, concurrent stale layout and a
controllable VisualViewport. These do not simulate a real iOS software keyboard.
The browser still determines filename sanitization, destination, preview and
duplicate-name handling. The retry copy intentionally excludes subsequent edits.

## Verification

Commands below ran in this checkout; browser tests use a locally served
production build. The final full browser run includes all 23 new regressions.

| Directory | Exact command | Result |
| --- | --- | --- |
| Repository root | `cargo fmt --all -- --check` | Passed |
| Repository root | `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| Repository root | `cargo test --workspace` | Passed, 581 tests including doctests |
| Repository root | `cargo build --workspace` | Passed |
| Repository root | `cargo check --locked --workspace --target wasm32-unknown-unknown` | Passed |
| Repository root | `node apps/writer/tests/layout-check-native.mjs` | Passed, native layout fingerprint |
| `apps/writer` | `npm run typecheck` | Passed |
| `apps/writer` | `npm run lint` | Passed |
| `apps/writer` | `npm test` | Passed, 44 tests |
| `apps/writer` | `npm run test:build` | Passed, 2 bootstrap tests |
| `apps/writer` | `npm run test:wasm` | Passed, 52 compiled WASM tests |
| `apps/writer` | `npm run build` | Passed, locked Rust release/WASM plus Vite production assets |
| `apps/writer` | `npm run test:browser -- --project=chromium --output=/tmp/writer-reliability-all-results --reporter=list` | Passed, 102 tests; final production bundle including occlusion/rotation fixes |
| Repository root | `pdfinfo /tmp/writer-reliability-browser-export.pdf` | Passed, actual UI export: PDF 1.7, Letter, one page, no JavaScript/encryption |
| Repository root | `pdftoppm -singlefile -scale-to 1100 -png /tmp/writer-reliability-browser-export.pdf /tmp/writer-reliability-pdf` | Passed; Poppler 26.05.0 raster inspected against the running page |
| Repository root | `node /tmp/writer-reliability-private.mjs` | Passed, supplied-file UI probe described above; private artifacts excluded from Git |

Toolchain: Rust 1.100.0, wasm-bindgen 0.2.130, Node 24.22.1; Linux Chromium 155.
The existing 11,840-character typing probe measured 30 events at p50 4.3 ms,
p95 6.5 ms and maximum 8.1 ms. This measures synchronous Rust transaction/DOM
reconciliation, not physical input-to-paint or real-device keyboard latency.
Initial build/test subprocess attempts were denied by the execution sandbox;
the authorized reruns executed successfully. Earlier regression runs exposed
two harness assumptions: Chromium sanitizes ZWJ characters in suggested filenames,
and the production CSP disallows fetching Blob URLs. Tests use a supported
Unicode filename and observe Blob lifecycle plus actual downloaded bytes instead;
the CSP was not weakened. A transient-error test also now injects its initial
failure once across reload rather than failing each fresh page load.

## Verification limits and device follow-up

- **Real iOS Safari and installed Home Screen/PWA:** unavailable here. Physical
  keyboard appearance, native selection handles, Files/Share handoff, background
  eviction and installed-app focus remain unverified. Mobile Chromium proves the
  tested trusted touch/focus and Rust editing workflows, not physical iOS parity.
- **WebKit/Firefox:** `PLAYWRIGHT_BROWSERS_PATH=/workspace/.writer-test-browsers npx playwright install webkit firefox`
  failed with CDN HTTP 403 `Domain forbidden` before engines were installed.
  A separate `PLAYWRIGHT_BROWSERS_PATH=/workspace/.writer-test-browsers npx playwright install firefox`
  attempt also failed with HTTP 403.
  These tests did not pass. New storage regressions are included in both existing
  compatibility projects; new mobile/export regressions are also included in
  WebKit for environments with the engine available.
- **Live application:** a read-only request to `https://writer.proairetos.com`
  was blocked by this environment's CONNECT proxy (HTTP 403, origin HTTP 000).
  No live origin failure, deployed version or production workflow is inferred.
- **GitHub Actions:** check the review PR's actual run status; local passes do
  not establish hosted or macOS/Windows success.
- Browser storage is origin/device-local and subject to eviction. Unload saving
  remains best effort. Exports cannot confirm delivery to disk. Existing format
  compatibility limits and PDF's outline-only, untagged/nonsearchable profile
  remain; this task does not expand them.

Before release, verify on an actual iPhone/iPad in Safari and installed-app mode:
tap blank/existing text from another control; type/delete/select with IME and
emoji; cross pages; rotate/show/hide keyboard; scroll without summoning input;
save, background, reopen; export every format into Files and reopen supported
ones; test visible retry links if automatic handoff produces no file. Record OS
and browser versions and any failures. Do not claim this checklist has run here.

Actual production-preview screenshots: [desktop](images/writer-reliability-desktop.png)
and [mobile](images/writer-reliability-mobile.png). Mobile is Chromium at 390×844,
not a photograph or iOS keyboard capture. Existing fit-width scaling and bundled
font/complex-emoji rendering limits remain; Unicode source text is preserved.
