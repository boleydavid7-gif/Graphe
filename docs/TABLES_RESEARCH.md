# First table support: research and integration review

Reviewed 18 November 2026 against Graphe main `fe4e094`. Graphe remains in
`apps/writer`; this work does not rename storage formats or existing APIs.

## Questions and primary sources

- [Microsoft: WordprocessingML tables](https://learn.microsoft.com/en-us/office/open-xml/word/working-with-wordprocessingml-tables)
  describes a block-level table, a shared column grid, rows, cells and paragraph
  content within cells. This supports a format-neutral structured table rather
  than tabs, a drawing, or an independent browser table model.
- [OASIS OpenDocument 1.3, part 3](https://docs.oasis-open.org/office/OpenDocument/v1.3/os/part3-schema/OpenDocument-v1.3-os-part3-schema.html),
  sections 9.1 and 19.679–19.682, distinguish rows, cells, covered cells and
  repetition. A span cannot be flattened without changing table structure.
  Repeated rows/cells also require expansion budgets, not unchecked allocation.
- [Microsoft: `cantSplit`](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.cantsplit?view=openxml-3.0.1)
  establishes that keeping a row together is a specific pagination policy;
  it is not Word's universal default. An initial keep-row-together profile must
  disclose its limits and return an error if a row cannot fit on a page.
- [ProseMirror table module](https://github.com/ProseMirror/prosemirror-tables)
  provides a mature comparison: rectangular, non-overlapping table invariants,
  explicit cell selections and table commands. Its implementation is a reference
  only, not a dependency or replacement editing engine. Its repository declares
  MIT licensing; no code is copied into Graphe by this review.
- [W3C table pattern](https://www.w3.org/WAI/ARIA/apg/patterns/table/) and
  [grid pattern](https://www.w3.org/WAI/ARIA/apg/patterns/grid/) distinguish static
  structure from composite interactive keyboard behavior. Use meaningful cell
  structure in the semantic input projection; do not claim a complete accessible
  grid merely because an SVG looks like a table.
- [Microsoft's RTF specification index](https://learn.microsoft.com/en-us/previous-versions/office/developer/office2000/aa140277(v=office.10))
  was available, but the current official archived 1.9.1 PDF could not be fetched
  through the research tool. This review therefore does not certify an RTF table
  implementation. Existing strict unsupported-table handling is preferable to
  inferred parsing or silent flattening.

## Recommended bounded implementation

Use a typed `Block::Table` with stable table, row and cell identities. Cells hold
existing paragraphs, so Unicode positions, direct/named formatting, hyperlinks,
lists and text transactions remain in Rust. Start with non-nested, unmerged,
rectangular tables. Require at least one row, one column, equal cell counts and
at least one paragraph per cell; check identifiers across every entity kind.
Bound dimensions, total cells and content before allocation.

Initial commands should insert a table, add/remove rows and columns, remove a
table and move between cells. All modifications use existing validated atomic
transactions and undo/redo. Deleting a row/column intentionally deletes its
content; the UI must identify these actions accurately. Cell navigation must
not advance the document revision or create history entries. Ordinary typing,
paragraph breaks and formatting reuse the existing engine.

Keep paragraph-relative grapheme coordinates. Logical traversal is row-major;
paragraph IDs distinguish paragraphs even when they occupy the same top-level
block. A text range within one cell may join its paragraphs. Destructive text
operations crossing cell boundaries or a table/body boundary must fail without
mutation until deliberate table-aware range semantics exist. Formatting may
traverse a checked range while retaining the surrounding table. Whole-row/cell
rectangular selection is a separate future capability, not something to infer
from ordinary text endpoints.

Rust shapes each cell's paragraphs at its available width, computes row height
from the tallest cell, paginates rows and emits cell bounds/borders alongside
the existing lines, glyphs and carets. React renders those coordinates. Browser
CSS wrapping or text measurement would introduce a competing layout engine.
Use explicit padding/border and rounding policies. Never clip or omit over-tall
rows silently; reject unsupported geometry with an actionable error.

Tables require an explicitly newer native document schema. Keep strict legacy
decoders: a table labeled as a previous schema is not a valid legacy document.
Saving/reopening must preserve IDs, cell/paragraph order, formatting and source
bytes. Import/export adapters own external table syntax. Only enable an external
table profile after validating its structure and supported properties. Unknown,
merged, nested, protected or content-bearing extensions remain exact preserved
originals with diagnostics; do not publish partial editable documents.

For formats whose exporters do not yet support table structure, report an
explicit unsupported export or a clearly assessed, intentionally selected text
conversion. Never silently skip tables. PDF must draw borders and the same Rust
geometry as the editor; existing outlined-text accessibility limits still apply.

## Existing code reviewed and findings

| Severity | Finding | Required response |
| --- | --- | --- |
| Critical | `office-core/position.rs` locates paragraphs by section/block only. Every cell paragraph would otherwise compare as the same location. | Extend structural locations and preserve stable paragraph identity/order. |
| Critical | `office-core/edit_ops.rs` replaces a range by splicing top-level section blocks. Merely adding flattened cell traversal could delete a table during ordinary typing. | Rebuild the correct paragraph container and reject unsafe cross-container replacement. |
| Critical | Several adapters and WASM views assume every `Block` is a paragraph. | Audit exhaustive matches, recursive/structured traversal, word count, find, formatting summaries and export assessment. Never add a wildcard that silently drops a table. |
| Critical | Schema 4 was introduced for hyperlinks and currently decodes through the current block type. | Preserve the exact old schema in migration before adding a new block type. |
| Important | Layout contains page lines but no cell geometry. | Reuse the Rust shaper and add bounded table geometry; preserve hit-testing/caret mappings across columns/pages. |
| Important | Body text selection is not rectangular cell selection. | Document and test the initial range policy; keep deletion/merge commands explicit. |
| Important | DOCX/ODT/RTF currently reject tables and have rich preservation policies. | Extend only deliberate profiles and retain all original-source safeguards. |
| Optional | Spans, nesting, resizable columns, table styles, repeated headers, split rows and rich rectangular clipboard slices. | Defer rather than implement placeholder controls or speculative abstractions. |

The existing Parley/skrifa shaping, pinned fonts, Unicode segmentation, Serde,
ZIP/XML and PDF code already supply the necessary low-level facilities. No new
third-party library is recommended for this bounded table feature. This review
does not certify tests, external consumer fidelity or device accessibility.

## Verification needed

Test invalid shapes/IDs and legacy schema rejection; cell-local Unicode edits
and formatting; cross-boundary failure atomicity; row/column operations and exact
undo/redo; native round trips; pagination and over-tall rows; keyboard and touch
hit-testing, selection, composition and clipboard safety; IndexedDB save/reload;
external-format round trips and explicit unsupported cases. Use independent
document consumers where available and distinguish desktop/mobile emulation
from actual iOS/Android testing.
