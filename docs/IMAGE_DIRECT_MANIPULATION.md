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
