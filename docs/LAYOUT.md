# Typography and layout — profile 1

Step 6 adds `office-layout`, a native/WASM Rust subsystem around the existing
format-neutral model. It does not change the document schema, native format,
editing transactions or retained originals. This branch is based on the DOCX
conversion prerequisite in PR #10; neither branch is merged or deployed here.

## Responsibility boundary

`office-core` owns semantic documents, validation, positions and editing.
`office-docx` imports supported properties and preserves unsupported originals.
`office-layout` resolves fonts/styles, shapes text, breaks lines and places them
on pages. `office-wasm` binds an immutable layout result to the live session and
revision. React draws the supplied geometry and glyph outlines. React does not
measure text, choose line breaks or paginate.

The existing semantic editor remains available. The page preview is a read-only
projection for this milestone; a new page-based editing surface, visual bidi
navigation and advanced accessibility belong to later work. Browser CSS in the
existing editor does not define layout-engine results.

## Reproducible typography

The low-level stack is Parley 0.13, HarfRust and Skrifa with `libm` and a pinned
font catalog. Host-font discovery is disabled. Font files, exact upstream
revisions, hashes and OFL notices live in
[`crates/office-layout/fonts`](../crates/office-layout/fonts). Production browser
assets must include the notices because the WASM module embeds these fonts.

The catalog provides Arimo sans, Tinos serif, and script/emoji fallbacks. A font
substitution is a layout decision, never a rewrite of the stored font family.
Proprietary fonts are not bundled. Missing coverage must remain observable through
diagnostics. CJK and color-emoji presentation are not general guarantees of this
initial catalog.

Shaping uses real font metrics, glyph positioning, kerning, ligatures and
bidirectional/script processing. A glyph is not a Unicode scalar, grapheme or
text run. Formatting boundaries retain the semantic model's exact run data;
they do not create cursor boundaries within combining or emoji sequences.

Layout profile 1 pins its Unicode/library behavior. The core's extended grapheme
rules use Unicode 17. ICU4X 2.3 default line-break data used by Parley is Unicode
15.1; do not describe all algorithms as using the latest Unicode version.
Changing fonts, Unicode data or shaping libraries requires reviewing the catalog/
profile identity and regression expectations.

## Geometry and positions

Public output positions use signed integer **1/64-point** units; a point is
1/72 inch. Model twips and half-points are converted at the layout boundary.
Rounding is part of the layout profile. The model retains its original integer
values and optional-property distinctions. SVG glyph paths and their transforms
are produced by Rust, so the browser does not reshape preview text.

Twip conversions and measured coordinates round to nearest, with ties away from
zero. Outline coordinates round to 0.001 font design unit. A positive line
advance below the output resolution is an explicit error. Underline thickness
has a one-output-unit visibility minimum; that drawing policy does not alter
stored font metrics or formatting.

Each page carries its section ID, physical dimensions and usable content
rectangle. Lines retain paragraph IDs, logical text ranges, glyph placements,
decorations and cursor mappings. Core grapheme indices, UTF-8 byte offsets and
UTF-16 code-unit offsets remain distinct. A snapshot is transient and cannot be
used as an editing anchor after its session revision changes.

Letter and A4 defaults and custom geometry use the same layout path. Explicit
section dimensions and margins override layout defaults; orientation does not
rotate dimensions implicitly. Header/footer offsets are preserved model data,
not implemented header/footer content. Pagination supports paragraph continuation
and paragraph page-break-before. The model has no section-break-kind property;
the layout policy must document its section transition behavior.

Profile 1 starts each section on a new page. Paragraphs continue line by line
when the remaining body height is exhausted. Page-break-before starts a new
page when the current page already has content. Adjacent paragraph spacing uses
`max(previous.after, current.before)`. A gap that cannot fit before a line moved
to a page top is suppressed and diagnosed; advanced Word baseline/spacing and
contextual-spacing rules are not implemented.

The default profile is Letter with one-inch margins, Arimo 12 pt, 1.5 times
natural line height and 8 pt after paragraphs. These are independent Writer
defaults, not all OOXML application defaults. Multiple spacing scales natural
font metrics, minimum spacing can expand, and exact spacing keeps its requested
advance. Glyph ink is centered in exact boxes and may overlap when those boxes
are small; a diagnostic reports this. Word's undersized-box clipping is not
reproduced. First-line/hanging and signed left/right indentation retain their
requested geometry rather than being clamped to the body.

LF, CRLF and CR are deliberate soft-break inputs in native layout; their exact
source bytes and core boundaries are preserved. Shift+Enter creates LF through
a Rust transaction while Enter creates a paragraph. Tabs and other control
characters without a defined layout policy return `unsupported_layout` and
leave the source intact. Tab stops are not approximated as fixed-width glyphs.

## API boundary

Native callers create a reusable `LayoutEngine`, then call
`layout_document(&Document, &LayoutOptions)`. The result contains page/line
geometry, reusable glyph-outline definitions, text-position mappings and
diagnostics. A convenience free function supports one-shot use. Invalid options,
unsupported resource sizes or failed shaping return structured `LayoutError`
values and leave the document unchanged.

The browser uses a separate `layout_snapshot` method rather than enlarging every
ordinary editor view. Its envelope includes document ID, session ID and revision.
Requests for an old revision are errors. Layout and caches are outside the
document, native file and undo history. The frontend asks for layout when the
preview is needed, preserving the existing editing path.

## Compatibility and limits

Supported DOCX page and paragraph properties arrive through the adapter as
generic model values. Layout does not parse OOXML, store format-specific nodes or
modify the retained source. Unknown imported semantics follow the same read-only
preservation policy as before Step 6. Other file formats are not added.

This is an independent layout profile, not Microsoft Word pagination parity.
Font versions, available glyphs, line-breaking data, spacing policies and absent
features can change page boundaries compared with Word. Widow/orphan rules,
hyphenation, tables, floating objects, headers/footers, numbering and advanced
section behavior require future explicit support. Native save/reopen must retain
the same semantic input and therefore reproduce layout under the same profile.

Resource limits protect layout without narrowing document-model validation.
Values may be valid model data but exceed the current layout profile. Such cases
return an error rather than silently clamping, repairing or mutating the model.

| Layout resource | Profile 1 limit |
| --- | --- |
| Source text | 200,000 UTF-8 bytes and 200,000 UTF-16 units |
| Sections / paragraphs / runs | 2,000 / 2,000 / 20,000 |
| Styles / inheritance depth | 128 / 32 |
| Identifier / font-family label | 1,024 / 256 UTF-8 bytes |
| Absolute geometric value | 144,000 twips (100 inches) |
| Font size | 1–1,024 half-points (0.5–512 pt) |
| Line-spacing multiple | 1–4,800 units of 1/240 line |
| Output glyphs / lines / pages | 200,000 / 20,000 / 512 |
| Unique outlines / outline path bytes | 20,000 / 16 MiB |
| WASM options request / serialized layout response | 8 KiB / 32 MiB |

The preview may reject a valid document that exceeds this profile. Such a
failure does not prevent native saving, original recovery or continued editing.

See [research and review](RESEARCH.md), [decisions](DECISIONS.md), font provenance,
and the final milestone verification report for exact executed tests and remaining
limitations. Building an application is not deploying it or verifying access
restrictions at `writer.proairetos.com`.
