# Document-menu reliability

Baseline: main `22dfe38b5935aa1c7931ed3690800b51774719b1`.
Branch: `fix/writer-document-menu-actions`. The owner authorized merging this
focused fix after verification; no deployment commands or new features are included.

## Action trace

| Visible control | Actual implementation and protection |
| --- | --- |
| Document options | Native `details`/`summary` disclosure; outside pointer, Escape and known-outside keyboard focus dismiss it. |
| New document | `documentAction` → `newDocument` → unsaved guard → Rust `new_document` → session replacement. |
| Open document | `showLibrary` → IndexedDB list; saved entry → load/native validation; Open file → native file chooser → existing Rust adapter/import preservation. |
| Save As… | Rust `assessExport` → format/loss review → revision-bound `exportFormat` → prepared download. All six existing formats remain available. |
| Export | Full native serialization and prepared download; retained-original privacy disclosure when a source is present. |
| Export Word | DOCX assessment → loss review when required → Rust DOCX export and prepared download. |
| Download original | Conditional on a retained source; Rust `exportOriginal` → exact-byte download. |
| Page preview | Toggle existing Rust page-layout preview; return restores editing selection/focus. |
| Close | Unsaved guard → clear current save association → closed-document state. |
| Font licenses / Format library notices | Real Vite-bundled text resources opened through native links. |
| Mobile More formatting | Show/hide existing font/size/alignment controls; changes use Rust transactions and appear in native export. |

Local Save is the existing header button/shortcut, and rename is the editable
title; neither is an additional document-menu item. No visible command was a
stub, and no new command was added. Initialization, busy operations, composition
and closed-document states continue to disable inapplicable actions. Disclosures
may reveal disabled controls during composition; they do not modify the document.

## Defects and focused corrections

1. **Swallowed activation:** null-target blur closed the disclosure before a
   pointer-activated item received its click. Safari intentionally does not focus
   every clicked button. Ignore ambiguous blur, retaining existing dismissal for
   outside pointer, Escape and known outside keyboard focus. No delayed clicks or
   forced pointerdown focus are introduced.
2. **Clipped commands:** the panel extended beyond short windows and the app's
   overflow-hidden boundary. Bound and scroll the existing panel with CSS and
   VisualViewport resize/scroll bounds, including keyboard pan. Do not scroll the
   entire application to expose hidden commands.
3. **Lost dialog return focus:** Open cancellation left body focus because passive
   cleanup closed an already-removed dialog. Use layout cleanup while connected.
   Command activation supplies the visible summary as opener. Successful session
   replacement closes owned dialogs topmost first before capturing focus, so
   native return focus cannot invalidate the replacement's first keystroke.

All changes remain in App interaction code and menu CSS. Rust document state,
editing, layout, validation, undo/redo, conversion consent, etags and original
preservation are unchanged. No new dependency, command layer or custom ARIA menu.
The existing [save/export safeguards](WRITER_RELIABILITY.md) remain in place.

## Verification

Tests run against locally served production assets in real Linux Chromium, using
native mouse/keyboard and trusted touch activation. Every menu option is exercised
through the visible disclosure; downloads, file selection, IndexedDB and popups
use the real implementation. Tests assert no uncaught JavaScript errors.
Regressions include cancellation, unsaved decisions, disabled composition states,
native source bytes, notice-resource contents, scrolling, keyboard focus and
actual typing/formatting after save/export and document replacement. The 26 new
cases also cover library/unsaved dialog stacks with Cancel, Discard and Save and
continue, checking both saved documents and the first input in the opened target.

The controlled focus-policy regression reproduces a null-target blur between
trusted pointer activation and click; it is not a physical Safari test. Controlled
VisualViewport tests represent keyboard occlusion/pan. Only unsaved-decision tests
delay autosave debounce/deadline timers to keep their prompts deterministic;
explicit Save still serializes Rust state through real IndexedDB transactions.

Before fixing, all four targeted desktop/touch activation and clipping regressions
failed. End-to-end testing additionally caught Open-cancel focus loss on desktop
and touch. An initial fixture mismatch used a deliberately unsupported DOCX;
the test was corrected to the supported public fixture without changing the parser.

| Directory | Exact command | Result |
| --- | --- | --- |
| Repository root | `cargo fmt --all -- --check` | Passed |
| Repository root | `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| Repository root | `cargo test --workspace` | Passed, 581 tests including doctests |
| Repository root | `cargo build --workspace` | Passed |
| Repository root | `cargo check --locked --workspace --target wasm32-unknown-unknown` | Passed |
| `apps/writer` | `npm run typecheck` | Passed |
| `apps/writer` | `npm run lint` | Passed |
| `apps/writer` | `npm test` | Passed, 44 tests |
| `apps/writer` | `npm run test:wasm` | Passed, 52 compiled WASM tests |
| `apps/writer` | `npm run test:build` | Passed, 2 tests |
| `apps/writer` | `npm run build` | Passed, Rust/WASM, TypeScript and Vite production build |
| `apps/writer` | `npm run test:browser -- --project=chromium --output=/tmp/writer-menu-all-results --reporter=list` | Passed, 128 tests including all 26 new menu regressions; zero uncaught JavaScript errors in the new cases |

Actual production-preview screenshots: [desktop](images/writer-menu-desktop.png)
and [mobile](images/writer-menu-mobile.png). The mobile capture is a 390×844
Chromium touch context, not a physical iOS capture.
Exact handlers are in `apps/writer/src/App.tsx`; focused
regressions are in `apps/writer/tests/document-menu.spec.ts` and included in the
Firefox/WebKit compatibility projects as well as Chromium.

## Limits

- No physical iPhone/iPad, Safari or installed Home Screen environment is available.
  Native keyboard appearance, selection handles and Files handoff remain unverified.
- `PLAYWRIGHT_BROWSERS_PATH=/workspace/.writer-test-browsers npx playwright install webkit firefox`
  failed at the official WebKit download with CDN HTTP 403 `Domain forbidden`.
  WebKit/Firefox execution could not run here; no pass is claimed.
- Hosted GitHub Actions status must be reported separately from local passes.
  Prior runs could not allocate runners; do not infer hosted/macOS/Windows success.
- These local production tests do not verify the deployed application's version
  or live Cloudflare behavior. No deployment or access-setting change is included.
- Existing format fidelity, PDF, font, best-effort lifecycle save and origin-local
  storage limitations remain as previously documented.

Before release, test every option on an actual iPhone/iPad in Safari and installed
mode, including with the keyboard visible, rotating, scrolling to the last item,
native file delivery, cancellation and the first edit after opening a document.
Research sources and the native-disclosure decision are in
[RESEARCH.md](RESEARCH.md#document-menu-activation-and-reachability) and
[DECISIONS.md](DECISIONS.md#047-keep-menu-activation-independent-of-pointer-focus).
