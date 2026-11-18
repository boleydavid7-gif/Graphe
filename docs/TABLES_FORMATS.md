# Table file compatibility verification

Verified locally on 2026-11-18 against the table development branch based on
`fe4e094544f08390c11be6624c325f9efa20ce12`. This report covers format adapters;
see the table implementation report for browser, engine and layout checks.

## Supported profile

- Native `.writer`: exact table/row/cell/paragraph identities and supported
  formatting are persisted by the shared schema-5 model and native envelope.
- DOCX: rectangular table profile implemented by `office-docx` (see its tests).
- ODT: rectangular tables, ordered cell paragraphs, existing supported run and
  paragraph formatting, and optional positive physical column widths. Fixed
  grid: 0.5pt `#808080`, 4pt padding, matching Rust layout. An absent/incomplete
  cell appearance uses that grid **with a meaningful normalization notice** and
  retains the exact original. Unknown cell appearance, merged/covered cells,
  formulas, repeated rows/columns, nesting, protection and unsupported content
  remain guarded; no partial editable projection is returned.
- PDF: grid rectangles and glyphs use Rust page geometry, with no second layout
  engine. Existing outlined-text limitations still apply: no searchable or
  selectable text, semantic tagging, PDF/UA or PDF/A claim.
- TXT: an explicit meaningful-loss warning precedes tab-separated cells and
  newline-separated rows. Cell paragraph breaks remain newlines. This is text
  conversion, not a reversible table encoding; borders/widths/structure are lost.
- RTF: table imports remain preserved read-only; table export and its assessment
  explicitly return `unsupported_rtf_export`. No table is silently omitted.

ODT output checks the same paragraph/style resource limits as its importer,
refusing an unreopenable oversized conversion before returning bytes. Adapter
limits are not a reason to discard content from the native document.

## Correctness correction

ODF underline style `none` must omit `style:text-underline-type` (ODF 1.3 Part 3
§20.391). The former exporter emitted `none` plus `single`; LibreOffice treated
ordinary text as underlined. The targeted correction preserves default, inherited
and direct clearing, single and double underline. Two regressions cover XML
semantics and native recovery/ODT regeneration. No other unfinished audit changes
were pulled into the table branch.

Source: <https://docs.oasis-open.org/office/OpenDocument/v1.3/os/part3-schema/OpenDocument-v1.3-os-part3-schema.html>

## Automated adapter regressions

```sh
cargo test -p office-formats --test tables --test odt_underline_export --offline
```

Latest execution: **8 passed, 0 failed** (6 table tests and 2 underline tests).
Coverage includes Unicode/combining marks/emoji/Arabic, mixed formatting,
multiple cell paragraphs, stable native recovery, import → transaction edit →
native save/reopen → external export/reimport for DOCX and ODT, exact immutable
originals, unknown/merged/nested/formula/repeated-content guards, normalization
notices for empty/partial named cell styles, deterministic PDF grid geometry,
TXT loss assessment and RTF loss refusal. The realistic committed fixture is
`crates/office-formats/tests/fixtures/project-review-table.writer` (36 rows,
72 cells, 75 paragraphs including introductory and closing paragraphs).

An earlier `cargo test -p office-formats --offline` invocation executed **108
unit/integration tests successfully**, then failed during doctest compilation:
Rustdoc reported missing `office_core`/`office_layout` artifacts in the concurrent
build directory. This invocation is **not** recorded as a complete suite pass.
Use the final consolidated verification report for the final workspace result;
focused reruns are overlapping coverage, not additional suite passes.

## Independent document consumer and PDF checks

Executed after the final canonical grid-color/underline output changes:

```sh
mkdir -p /tmp/graphe-table-consumers-final/from-docx /tmp/graphe-table-consumers-final/from-odt
cargo run -p office-formats --example export_format --offline -- crates/office-formats/tests/fixtures/project-review-table.writer odt /tmp/graphe-table-consumers-final/review.odt
cargo run -p office-formats --example export_format --offline -- crates/office-formats/tests/fixtures/project-review-table.writer docx /tmp/graphe-table-consumers-final/review.docx
cargo run -p office-formats --example export_format --offline -- crates/office-formats/tests/fixtures/project-review-table.writer pdf /tmp/graphe-table-consumers-final/review.pdf
soffice -env:UserInstallation=file:///tmp/graphe-table-consumers-final/lo-docx-profile --headless --convert-to odt --outdir /tmp/graphe-table-consumers-final/from-docx /tmp/graphe-table-consumers-final/review.docx
soffice -env:UserInstallation=file:///tmp/graphe-table-consumers-final/lo-odt-profile --headless --convert-to docx --outdir /tmp/graphe-table-consumers-final/from-odt /tmp/graphe-table-consumers-final/review.odt
python3 crates/office-formats/tests/verify-table-consumers.py /tmp/graphe-table-consumers-final
pdfinfo /tmp/graphe-table-consumers-final/review.pdf
pdftoppm -f 1 -singlefile -scale-to 1000 -png /tmp/graphe-table-consumers-final/review.pdf /tmp/graphe-table-consumers-final/review-page1
```

All commands exited successfully. Versions: LibreOfficeDev 26.8.0.0.alpha0,
revision `2c87e51eeaa2b413ff4ae097b2705eea1995d8e5`; Poppler 26.05.0.
The independent stdlib ZIP/XML checker imports no engine code. Original and
LibreOffice-converted artifacts retain one table, 36 rows, 72 cells and all
75 paragraphs in exact Unicode/text order. DOCX generated from ODT also confirms
ordinary body runs do not gain underlining. Generated byte sizes: ODT 3,775;
DOCX 2,890; PDF 287,485. PDF metadata reports three Letter pages, no JavaScript,
forms or encryption, and PDF 1.7. Page 1 was rasterized and visually inspected:
grid boundaries, mixed formatting, multi-paragraph cells, Arabic and emoji are
visible without clipping.

This is bounded interoperability evidence, not Microsoft Word certification.
LibreOffice resaves may introduce additional styles/metadata outside the strict
editable profile. Physical devices, Microsoft Word, accessibility conformance,
searchable PDF and general arbitrary-table compatibility were not verified here.
