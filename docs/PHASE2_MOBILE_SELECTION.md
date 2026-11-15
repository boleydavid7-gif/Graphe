# Phase 2, work area 1: mobile text selection

Starting revision: latest PR #17 head `e501d76bf58396e5f4fa10833f8292b830326e4b`,
including audit `f78e3ba` and the Lamplight bridge also merged through PR #19.
Main at the start was `a5fbe23`. Work stays on `dev/writer-stability-phase2`;
prior audit corrections and the integration bridge are preserved.

## Problem and approach

SVG glyph paths cannot supply the native browser's rich-text selection handles.
The previous editor supported taps and mouse dragging, but lacked a touch word
hold and endpoint dragging. Rust already provides directed selections, checked
Unicode positions, physical hit testing and both endpoint rectangles.

Keep those representations authoritative. Add two small Rust/WASM layout
actions, presentation-only pointer handling and a compact Lamplight clipboard
bar. No schema, migration, dependency, independent JavaScript text model or
layout engine is introduced. The approach and risks were presented before code.

## Rust contract

```ts
{ type: 'select_word', pageNumber, x, y }
{ type: 'drag_endpoint', endpoint: 'anchor' | 'focus', pageNumber, x, y }
```

Coordinates retain the integer 1/64-point page units. Existing revision, request
shape/size, layout validation and rollback rules apply. Selection changes do not
alter content, serialization, originals, revisions or undo/redo history.

Word selection uses full paragraph text across formatting runs and the pinned
`unicode-segmentation` dependency. The touched Rust-shaped grapheme cell selects
its containing Unicode word. Nonword emoji, punctuation and whitespace select
one complete extended grapheme. Empty paragraphs remain collapsed. Default
Unicode word boundaries are not locale dictionary breaking; no browser segmenter
or JavaScript string-length cursor model is substituted.

Dragging changes only the requested logical endpoint and affinity. The other
endpoint stays fixed through crossings and reverse ranges. Bidi highlights and
physical page continuation remain Rust output. Selection/copy can span sections;
existing editing operations may reject cross-section joins. Preserve those
errors rather than clipping or partially applying edits.

## Browser interaction

A stationary 550ms page press requests Rust word selection. Movement beyond
12 CSS pixels, scrolling, composition, stale identity/revision/selection or a
second finger cancels a pending hold. A timer does not request keyboard focus.
Trusted release or **Edit selected text** uses the existing native input.

Handles project Rust endpoint rectangles and keep usable touch targets across
paper/viewport scales. Logical identities and DOM nodes survive crossing through
a collapsed range. Scrolling, keyboard and zoom updates reproject rectangles;
they never calculate text layout. Dragging supports edge autoscroll. Native page
panning/pinch remain available; handles own single-finger drags from gesture
start while permitting pinch. Page gestures permit both panning axes. Above 1x
browser magnification, release inner scroll containment so gestures can pan the
visual viewport; normal 1x containment remains unchanged.

Copy, Cut, Paste and Edit selected text use existing Rust text/command APIs.
Cut waits for clipboard-write success. Empty clipboard text cannot accidentally
delete a selection. After browser I/O, exact live identity, revision and directed
selection must still match. Composition, dismissal and replacement cancel
destructive completions. Permission/unsupported API failures preserve content
and show an error or disabled control with a native keyboard/device fallback.

Handles are pointer projections; the semantic editor and keyboard selection are
the assistive-input path. This does not claim OS-native handles, a loupe or
accessibility compliance.

## Corrections found during implementation

- In actual Chromium, restoring a range inside a blurred contenteditable focused
  it during the timer. Restore immediately only when already focused; trusted
  activation owns new focus.
- At fit-width scale, 44px targets overlap on short words. Separate vertical
  placement and resolve the nearest physical handle without swapping endpoint
  identities. Keep the action bar outside handle hit areas.
- Forced focus can change mobile chrome/workspace height during dragging and
  invalidate an edge target. Avoid that focus and suppress focus-caret reveal
  while a handle owns the drag.
- Native horizontal reading after pinch was blocked by both the page's vertical-
  only touch policy and workspace containment. The identical native gesture
  panned only after both policies were corrected; keep containment at normal zoom.
- Existing tap-focus regressions caught cancellation during synchronous focus
  events. Finish a tap before focusing; a completed tap on stale geometry may
  focus the same live document/session, but cannot issue a stale hit. Keep that
  bounded release phase separate from strict word/handle mutation guards.

## Files and regression coverage

- `crates/office-wasm/src/layout.rs`: additive layout actions and word selection;
  `crates/office-wasm/tests/touch_selection.rs`: ten native regressions.
- `apps/writer/src/PagedEditor.tsx`: bounded holds, captured endpoint dragging,
  scrolling, viewport projection, cancellation and input/IME integration.
- `App.tsx`: live engine-view callback, returning no active view after close.
- `types.ts` and `styles.css`: action types and Lamplight handle/action styling.
- `SelectionActions.tsx` and `selectionClipboard.ts`: plain-text clipboard bar
  with live operation guards; `selectionClipboard.test.ts`: deferred-I/O tests.
- `tests/touch-selection-wasm.test.mjs` and `tests/touch-selection.spec.ts`:
  compiled ABI and trusted Chromium touch regressions. `package.json` only adds
  the new ABI test to its existing script; dependency lists/lockfiles are unchanged.
- Architecture, decisions, research and engineering rules document the boundary.

## Research and tradeoffs

- [W3C Pointer Events 3](https://www.w3.org/TR/pointerevents3/): establish touch
  policy before gesture start; use capture and respect native cancellation.
  Changing touch-action after a hold cannot reclaim an assigned native pan.
- [Unicode UAX #29](https://www.unicode.org/reports/tr29/): default word and
  extended-grapheme boundaries; retain the pinned engine implementation.
- [MDN VisualViewport](https://developer.mozilla.org/en-US/docs/Web/API/VisualViewport):
  keyboard/pinch visibility differs from document layout.
- [WebKit user activation](https://webkit.org/blog/13862/the-user-activation-api/):
  focus and clipboard actions must respect browser activation/permission policy.

Visible semantic DOM text was considered for native selection handles. Its
browser reflow would compete with Rust pagination. Retain the existing input/IME
projection and Rust geometry instead. No extra editor/gesture framework is needed.

## Verification and limitations

All available work-area checks passed on the frozen production build:

| Command | Executed result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass. |
| `cargo test --workspace` | 619 passed, zero failed/ignored, including ten new native selection regressions. |
| `cargo build --workspace` | Pass. |
| `cargo check --locked --workspace --target wasm32-unknown-unknown` | Pass. |
| `npm run typecheck` | Pass. |
| `npm run lint` | Pass, zero warnings. |
| `npm test` | 61 passed, zero failed/skipped, including seventeen deferred clipboard cases. |
| `npm run test:wasm` | 58 passed, zero failed/skipped, including five new compiled-ABI cases. |
| `npm run test:build` | 2 passed, zero failed/skipped. |
| `npm run build` | Pass; actual Rust/WASM initialization and production asset loading exercised by browser tests. |
| `npm run test:browser -- --project=chromium --output=/tmp/writer-phase2-mobile-browser-final-results --reporter=list` | 171 passed, zero failed/skipped, 8.3min. Includes seventeen trusted-touch cases and the existing full file workflow matrix. |
| `node apps/writer/tests/layout-check-native.mjs` | Pass; existing four-page native/WASM layout fingerprint unchanged. |

Frontend commands run from `apps/writer`; Rust/parity commands from the workspace
root. Node24, stable Rust1.100, wasm-bindgen0.2.130 and installed Chromium155
were used with the existing toolchain on PATH. Targeted cases overlap those
suite totals and are not added again. Intermediate runs found interaction
defects described above; the final full run passed the original regressions
unchanged. The superseded failing full run was stopped to correct focus recovery.

The browser cases cover word/emoji selection, endpoint crossing, fourteen-page
selection, stationary-finger autoscroll, native pan/pinch, post-pinch handle use
and mouse recovery, stale revisions and same-revision selection changes, viewport
and IME simulation, formatting/history, successful/failed clipboard operations
and asynchronous stale cut/paste. Save bytes/etags remain unchanged by selection.
The existing matrix also verifies import/edit/native-save/reload/export/reopen.

[Actual public example screenshot](images/writer-touch-selection-mobile.png).

One test-driver issue was corrected independently of product behavior: after
pinch, Playwright bounding boxes use visual coordinates while DOM/SVG measurements
use layout coordinates. CDP touch injection subtracts visible-viewport offsets
once; it does not segment or independently lay out text.

No physical iOS Safari, Android Chrome, installed PWA, software keyboard, loupe or
OS clipboard/device test is established here. CDP touch gestures are actual
Chromium input; keyboard viewport/composition cases use controlled events.

Firefox/WebKit executables are unavailable here; no pass is claimed for those
engines or hosted cross-platform CI. Their restoration is a subsequent work area.
Top-level viewport behavior also differs from embedded frames: this work preserves
the Lamplight bridge but does not certify physical embedded-host keyboard behavior.
Clipboard transfer is plain text, retaining the existing paste profile; rich
clipboard formats and an OS-native loupe are outside this change.

External-format compatibility, embedded-text PDF, large-library performance and
hosted CI restoration are subsequent work areas, not completed by this change.
