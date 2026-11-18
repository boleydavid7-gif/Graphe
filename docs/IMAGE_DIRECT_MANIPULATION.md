# Graphe direct image manipulation — initial in-flow profile

## User interactions
- Select an image to display an understated selection border and eight point handles.
- Drag a corner handle to resize with original aspect ratio preserved, or a side
  handle to adjust only width or height. Edits clamp to current page's content area.
- Touch with two fingers on an image to pinch-resize; this intentionally prevents
  document pinch zoom *only while the gesture begins over the image*. Touch anywhere
  else on the page to continue ordinary document zoom and panning.
- Drag the selected image horizontally to the closest left, center, or right body
  alignment. Drag it above/below other content to change its in-flow block order.
- Image Properties remains available for exact point dimensions, alt text and
  alignment. All completed gestures become one undoable Rust transaction.
- The preview never writes temporary widths or coordinates into native document
  state. The authoritative page layout may reflow after dropping.

## Scope and limitations
Images remain **in flow**. They can be moved around paragraph/table/image block
boundaries in their original section, but cannot float over text, move to arbitrary
X/Y pixels, wrap text around their borders, or move to another section. Images
are never split across pages, preserving existing layout behavior. No new
native schema, dependency, imported layout claim or non-Rust layout engine.
Printed/pdf images use committed Rust layout as before.

## Verification
- Native transaction test: reorder/align, exact image asset reuse, undo/redo,
  invalid target rejection
- Compiled WASM test: one-step move, stale-target rejection, export+undo
- Browser test: drag handle resize and horizontal drag, native export+undo
- Manual on real iPhone/Android: pinch on image, drag on image, release after
  viewport scroll, rotate/reload, text selection outside image
- Cloudflare build is necessary but not a substitute for a full test suite

## Mobile gesture smoothing (2026-11-18)

Gesture coordinates retain the initial SVG-to-client transform so keyboard or
viewport movement does not change the resize distance mid-gesture. Pointer
previews are coalesced to one animation frame, avoiding intermediate single-finger
updates during two-finger pinch. Resize previews use the same left/center/right
anchor as Rust layout. A completed preview remains until a new immutable Rust
page snapshot acknowledges the edit (or an undo/replacement); rejected/no-op and
cancelled gestures clear it. Reselecting an already-selected image restores
command focus without scheduling an unnecessary layout refresh.

No document model, layout engine, undo history or persistence policy changed.
Verified: TypeScript, ESLint, production Vite build, 87 frontend unit tests and
18 actual Chromium image workflows passed. The new regression moves the SVG
origin during a resize, verifies stable dimensions/center anchoring and checks
the committed geometry. Its one fixed-unit tolerance is 1/64 point; Rust integer
centering can round by half that unit. Existing pinch, drag, undo/reopen and
DOCX/ODT/PDF workflows pass in that 18-test suite. Physical iOS Safari/PWA and
Android testing remain unavailable; emulation does not certify device behavior.
