# Graphe images: Step 8A contract

Graphe supports embedded static PNG and JPEG pictures as standalone document-body
blocks. This is a bounded first image profile, not arbitrary Office drawing
support. Research, dependency evaluation and standards references are recorded in
[IMAGES_RESEARCH.md](IMAGES_RESEARCH.md). Verification results belong in the
milestone report; this document describes the implemented behavior.

## Model, assets and native files

`office-core::Block::Image` contains a stable image ID, immutable `ImageAsset`,
positive width and height in integer twips (1/20 point), alternative text and
left, center or right alignment. It has no DOCX or ODF fields. Image blocks and
paragraphs/tables share the existing ordered section body; images are not text
characters and do not consume UTF-8, UTF-16 or grapheme offsets. Table cells
continue to contain paragraphs only.

An asset retains its exact original encoded bytes. Rust validates and decodes it
once, applies EXIF orientation, and caches canonical RGBA8 pixels and a static PNG
for presentation. Clones and undo snapshots share these immutable buffers. Normal
validation and typing do not decode the asset again. Browser snapshots contain
IDs and properties, not repeated base64 image payloads; the revision-checked
WASM `image_png` endpoint supplies canonical presentation bytes separately.

The provisional UTF-8 JSON `.writer` envelope and model schema are now **6**.
Image assets serialize as the declared raster format and standard padded base64
original bytes; render caches and Blob URLs are not serialized. Formats 1–5 use
their explicit strict migration paths. Old version labels cannot accept image
fields, unknown fields are rejected, and unsupported future versions fail.
Full `WriterFile` APIs retain imported originals independently of semantic edits.

Resource limits are checked in Rust, including aggregate native-file preflight
before full raster decoding:

| Resource | Limit |
| --- | --- |
| Browser image insertion | 2 MiB encoded input per image |
| Core/imported raster | 8 MiB encoded input per image |
| Raster width or height | 8,192 pixels |
| Decoded raster | 16,000,000 pixels per image |
| Image blocks per document | 32 |
| Original raster bytes per document | 16 MiB |
| Decoded pixels per document | 16,000,000, or 64 MB of RGBA samples |
| Alternative text | 8,192 UTF-8 bytes; only LF and TAB control characters |
| EXIF payload | 256 KiB |
| Complete native file, including retained original source | 32 MiB |

The browser prototype has a stricter **5 MiB complete native document** budget,
including JSON/base64 and any retained imported source. Multiple source copies
can reach it before the core raster limits. Failed insertions/imports preserve
the current session and report the limit; this milestone does not increase the
storage or embedding-bridge budgets.

These limits bound supported input; they are not a total-process memory promise.
Encoded originals, derived PNGs, temporary decoder buffers, layout, undo records
and browser presentation consume additional memory. Format packaging and output
limits also apply. Declared MIME types/extensions cannot override the actual
signature, dimensions and complete decode. Malformed data, animation, ambiguous
orientation and unsupported metadata fail explicitly rather than being repaired.
No external image URL is fetched.

## Editing and presentation

Choose **Insert → Image…** at a collapsed body-paragraph caret. Rust inserts the
picture after that paragraph and supplies a following text paragraph when needed,
as one atomic undoable transaction. Insertion into a table cell or a noncollapsed
text selection is refused. The initial size uses 96 pixels per inch and is
proportionally fitted within the section's usable page area. The asynchronous
file-read completion rechecks engine/session/document/revision, exact selection
and composition state before committing.

Select a picture to access **Image properties…**, **Delete image** and **Back to
text**. Properties change physical width/height, aspect-ratio locking,
alternative text and alignment through existing Rust transactions. With the
ratio unlocked, explicit nonuniform sizes are supported. Invalid or oversized
resize requests leave the document unchanged. Undo and redo restore image edits
and the corresponding selection state.

If page layout is unavailable, text view includes explicit image-selection
buttons. An oversized native/imported picture can therefore be selected and
resized or deleted without replacing the document or silently shrinking it.

Image selection is separate from Rust text selection. While selected, text
commands cannot act on a retained earlier text caret. Delete/Backspace removes
the image; Enter, Escape or arrows explicitly return to adjacent body text. If
there is no adjacent paragraph in that direction, this deliberate action creates
one through an atomic transaction. Merely opening an image-only document does
not add paragraphs. Image clipboard copy/cut/paste is not supported and reports
that limitation without modifying content. Text replacement cannot cross an
image or table structural boundary; it fails rather than deleting those blocks.

Rust owns placement and pagination. `PageLayout.images` contains stable source
IDs and exact page-local rectangles in 1/64-point units, without pixel data. An
image fits intact on one page, never splits, and never resizes implicitly during
layout. Oversized imported/native geometry produces a structured layout error;
the document remains available for recovery. Images have zero intrinsic spacing:
the preceding paragraph's spacing-after and following paragraph's spacing-before
supply gaps. A preceding gap that cannot fit at a page boundary is suppressed
with a layout notice. Left/center/right alignment uses the section's usable text
area, not browser measurements.

SVG and PDF paint the same canonical pixels in Rust's rectangles. A semantic
noneditable image projection exposes alternative text without including it in
paragraph UTF-16 offset calculations. The paged SVG handles object selection;
hit-testing inside the image returns no fabricated text cursor. Browser Blob
URLs are presentation caches keyed by session and immutable image ID, revoked
on deletion, session replacement or disposal. The existing composition buffering,
save lane, revision/etag checks and original-source safeguards remain in force.

## Format boundaries

| Format | Image behavior |
| --- | --- |
| `.writer` | Preserves supported image properties and exact original raster bytes; saving/reopening uses schema/envelope 6. |
| DOCX | Imports a supported dedicated body paragraph containing one embedded DrawingML `wp:inline` PNG/JPEG picture; exports the same standalone profile with package-local image relationships, physical extent, alignment and description. |
| ODT | Imports/exports the supported dedicated paragraph containing one embedded `draw:frame`/`draw:image` with `text:anchor-type="as-char"`, physical dimensions and description. |
| PDF | Export only. Places compressed raster XObjects and alpha masks at Rust page geometry. Existing outlined-text/search, tagging, PDF/UA and PDF/A limitations remain. |
| RTF | Export of documents containing images is explicitly refused. Embedded RTF pictures are not newly enabled by this milestone. |
| TXT | After meaningful-loss confirmation, replaces each picture with `[Image: alternative text]`; pixels, size and placement are lost. The current document is unchanged. |

DOCX/ODT imports validate package-local targets, media declarations, complete
rasters and structural placement. Known picture-carrier text formatting,
spacing/indentation or descriptive metadata can be normalized with notices and
the exact original retained. Content-bearing, unknown or structurally unsupported
drawings remain preserved read-only; the importer does not expose an editable
partial document. Real LibreOffice resaves can add graphic styles, effects,
wrapping and related settings outside this profile; those cases remain guarded
rather than claiming universal LibreOffice image roundtrip support. A dedicated
picture's unsupported page-break semantics also remain guarded when they cannot
be represented by the current section model.
DOCX export of a nonfinal section ending directly in an image also returns an
explicit error: carrying section properties would otherwise require an extra
paragraph. Add a following body paragraph when that supported structure is wanted.

DOCX/ODT copies use original raster bytes when their interpretation matches the
canonical presentation. Images with applied orientation or approximated color
interpretation use normalized PNG copies, with an export disclosure. Profile,
gamma/high-precision or other unsupported color interpretation can be
approximated in the preview and PDF; this is not full color management. Exact
original raster bytes remain in the native file. Regenerated external copies do
not graft deleted source media or old thumbnails back into current content.

## Preservation, privacy and remaining scope

Preserving an original is a recovery guarantee, not a sanitization operation.
Original PNG/JPEG bytes can contain EXIF location, device information, timestamps
and other metadata. A native file with a retained complete source can also
contain deleted text or pictures from that source. Exported DOCX/ODT may retain
original image metadata when original rasters are reused. Do not describe any
of these workflows as stripping location or other private metadata. Downloading
the original returns the preserved file, not the edited projection.

This step does not implement SVG/vector input, GIF/WebP/HEIC, animation, remote
images, inline pictures mixed with text, floating or wrapped objects, image
tables/cells, cropping, arbitrary transforms, image filters, drag resizing or
clipboard image insertion. Alternative text is editable; its presence alone does
not establish accessibility compliance. Automated browser interaction and
synthetic composition coverage do not establish physical iOS/Android/PWA or
screen-reader behavior. Those device and assistive checks must be reported
separately from executed automated checks.
