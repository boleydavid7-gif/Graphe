# Step 7 — Lightweight Writer and Lamplight interface

This milestone builds on merged Steps 1–6 (`86f2ad8`) on the development branch
`dev/writer-lamplight`. The owner's approved
[desktop](design/writer-lamplight-reference.png) and rough
[mobile](design/writer-lamplight-mobile-reference.png) mockups are the primary
visual references; their preservation and constraints are recorded in
[design/README.md](design/README.md).

## Everyday workflow

Writer presents a compact warm-brown/charcoal header, gold lantern and active-tab
accent, a minimal toolbar, warm-white pages and a page/word/zoom status bar.
Write is selected initially. Tabs reuse working operations: Write formatting,
Insert paragraph/soft break, Layout alignment, and Review Find/reading preview.
Document menus retain New, Open, rename, Save, native and DOCX import/export,
original recovery and Close. Essential mobile actions remain available without
populating the interface with future features.

The primary editing surface shows continuous Rust-laid-out pages. Typing,
deletion, paragraph creation, plain-text copy/cut/paste, formatting and undo/redo
use the existing Rust document session and atomic transactions. Existing named
paragraph styles can be assigned or cleared; no style creation, template system,
tables, images, comments or collaboration were added. Direct overrides remain
intact when a style reference changes.

## Engine and input boundary

`office-layout` now projects its existing immutable geometry through hit testing,
caret rectangles, bidi-safe selection rectangles and visual navigation. Logical
positions retain source paragraph IDs and distinct core grapheme, UTF-8 and
UTF-16 values. Leading/trailing affinity distinguishes wrap and bidi edges.
Up/down retain preferred x; line and document endpoints are explicit operations.
Selected zero-advance text and paragraph separators have visible marker geometry
without inventing font advances or document text.

The WASM bridge exposes `layout_action` and `selection_geometry` alongside the
existing `layout_snapshot`. Actions are bounded and strict; expected revisions
reject stale geometry. The TypeScript boundary checks full session/document/
revision origin before using displayed pointer coordinates. Successful visual
actions change selection/runtime affinity, not content revisions or undo history.
Transport failures roll back selection runtime rather than publishing partial
interaction results. Font/layout caches remain outside files and history.

The existing controlled semantic `EditorSurface` remains the browser input and
accessibility projection. In paged mode it is focusable, visually clipped and
positioned at the Rust caret, retaining browser composition and clipboard event
handling. It is not `display:none` and is not a second document. Its DOM may
temporarily buffer IME or an uncancelable input event; final replacements commit
through Rust once. Logical DOM selections from assistive input still normalize
through the core. Visual navigation and page-pointer hits use Rust geometry.

React renders glyph paths, underlines, selection and caret rectangles from Rust.
It performs viewport/unit transforms and scrolling, not shaping, text measurement,
line breaking or pagination. Selection-only changes reuse the immutable glyph
subtree and cached native geometry. Content layout requests coalesce on animation
frames; stale pages cannot be used for hits while newer input is pending.
Horizontal and vertical caret reveal use Rust rectangles. Canceled touch gestures
perform cleanup without moving the cursor or opening the keyboard.

## Autosave and reliable local persistence

`WriterSaveController` is a small browser coordinator around the existing
`WriterStorage.save` transaction. Manual Save and autosave share one serialized
write lane. Autosave runs after 750 ms of inactivity, with a five-second maximum
wait during continuous foreground typing. Timers are best effort in background
tabs. Untouched new blank documents do not create library records automatically.

Each attempt captures identity, revision, title and immutable native bytes.
The expected etag is resolved when the attempt actually starts, after preceding
saves commit. A bounded coalesced follow-up handles newer edits. Completion means
the IndexedDB transaction completed, not just its `put` request. Session/document/
revision and replacement-epoch guards prevent a late result from marking newer
work or another document saved. Reopening an existing record adopts its token;
supported DOCX imports still preserve their original before editing activates.

Errors pause automatic retries, retain dirty in-memory work and show an explicit
retry or separate-copy path. Conflicts never silently overwrite another tab's
save. Active IME work pauses serialization and keeps status unsaved; canceling
composition does not falsely change saved content. New/Open/Close/Discard cancel
unsent timers. A transaction already underway cannot be canceled and may finish
for its original record; it cannot adopt or clear the replacement document.

Visibility-hidden triggers a best-effort pending flush. Before-unload warns for
dirty or composing work, but browser shutdown, crashes and storage eviction are
not durability guarantees. Export native files for backups. No accounts, cloud
sync, new database schema or persisted editing history were added.

## Find and existing styles

Find uses bounded Rust literal, case-sensitive search within paragraphs. It does
not normalize Unicode, case-fold transformed strings or search synthetic
paragraph separators. Results retain the originating session/revision; partial
combining/emoji matches select complete core graphemes and duplicate expanded
ranges are merged. The limit is 1,000 matches and 1,024 query UTF-8 bytes. Enter/
Shift+Enter selects next/previous results, and Escape returns to the document.
Paged Find reveals offscreen results without moving focus out of the query.
In fallback Text view the Rust search and logical selection still work, but
automatic visual reveal while the query is focused lacks page geometry.

The style menu lists existing native/imported definitions. The unstyled option
is labeled Paragraph, rather than inventing a Normal style. Assignment uses the
existing core operation, shares its validation/history and preserves direct
formatting. Rust's unspecified display font is Arimo, matching the pinned layout
catalog; explicit source font requests remain unchanged and may be substituted
with diagnostics.

## Reference fidelity and deliberate deviations

The desktop hierarchy, compact proportions, four tabs, lantern identity, gold
accent, white paper and minimal footer follow the approved image. Mobile places
the title on a second row, keeps essential formatting visible with overflow, and
uses a compact document-action bar.

- Device-save controls replace cloud symbols, because storage is local.
- Bundled Arimo/Tinos/script fonts replace the reference's Calibri request; no
  proprietary font is presented as bundled. Existing source font names survive.
- Only supported controls are shown; numbering, highlighting, advanced insert/
  review features and a second-window command were not invented to fill icons.
- Written instructions require opaque flat surfaces and restrained rounding,
  overriding texture/glow/rounding suggested by the images. The phone's operating-
  system clock and battery are not recreated.
- Physical paper and margins follow the Rust profile/document rather than being
  stretched to match a mockup. Zoom shows its actual presentation scale. On mobile,
  Fit width is an overview; larger zoom permits panning inside the workspace.
- A persistent application status bar supports continuous multi-page scrolling;
  it is not repeated or overlaid inside each physical page.

## Limits and next gate

Tab stops remain a structured unsupported-layout case. The finite font catalog
has no CJK coverage and uses monochrome emoji. An explicit Text view is available
for layout failures or missing glyphs; it reuses the semantic editor and reports
pages unavailable rather than inventing browser pagination. Source, save, export
and undo remain Rust-owned in that mode.

Clipboard is plain text; source styling is not a rich clipboard interchange
format. Word/option-arrow navigation, double-click word selection, mobile drag
handles and advanced assistive navigation are not completed. PageUp/PageDown
scroll rather than moving the logical selection. Synthetic IME and emulated
mobile tests cannot certify native OS input/assistive technology. Word pagination,
default-spacing and exact-line clipping differences from Step 6 remain explicit.
Large full layout JSON/DOM updates remain a measured performance constraint; the
existing 32 MiB response limit can reject a valid large document's paged view.
The browser retains 20 per-command undo snapshots, without typing-history
grouping; this is a count bound rather than an unlimited history guarantee.

The retrospective review found ordinary typing previously created one run per
keystroke. Collapsed input now extends only a nonempty left prefix with exactly
equal direct formatting. It does not merge existing runs, resolve inheritance or
normalize loaded documents. One thousand independent keystrokes now retain one
run; native and actual-WASM regressions verify the behavior and history. This is
a targeted editing correction rather than a new model, storage version or undo
grouping system.

See [research](RESEARCH.md), [decisions](DECISIONS.md) and the
[Step 7 verification record](WRITER_STEP7_VERIFICATION.md) for actual executed
tests and remaining limitations. After Step 7 approval, pause feature development
for the planned comprehensive technical, security, DOCX-compatibility and
commercial-licensing audit. Do not begin Step 8 or deploy automatically.
