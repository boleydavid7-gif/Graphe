# Phase 2, work area 3: searchable PDF investigation

Mobile selection and compatibility are completed first (`2bccd2c`, `2a6b2cf`).
This work remains on the dedicated Phase 2 development branch; no merge or
deployment is authorized.

## Decision: retain outlined production export

The investigation is complete; searchable/selectable PDF is **not implemented in
production**. Six bounded Type 3 variants were generated; the independently
tested variants failed rendering acceptance, and one remained untested.
Keep the existing outlined exporter and its explicit search/selection/accessibility
warning. Do not treat syntax validity or exact text mappings as visual fidelity.
No production API, layout DTO, core/native schema, dependency or font changed.

## Findings and implementation experiment

The current PDF paints reusable vector forms from Rust layout. Its output is
deterministic and visually faithful to those outlines, but contains no searchable
text. The pinned catalog is mostly variable TrueType: embedding its raw bytes
would lose the selected instance (notably Hebrew defaults to weight 100 while
Writer requests 400). Glyph IDs are also not Unicode characters. Existing layout
retains logical line text, font/glyph IDs and positions but discards the shaped
atom-to-source association needed for trustworthy text export.

A bounded Type 3 prototype used existing `pdf-writer` and resolved vector outlines.
It added transient generic shaped-atom provenance, with checked logical UTF-8
coverage and visual glyph membership. Each text code mapped to a complete shaped
atom, including ligature continuations. Existing glyph/caret/decoration/page
geometry and outlines stayed identical on the four-page reference after stripping
the additive metadata (original fingerprint `7741b146df0ad6128c653190af74ccad0daa8efad249200a82d7c0b9c03b0236`).
No text was inferred from glyph IDs or cursor rectangles; no second shaping pass
or fake invisible text layer was introduced. The experimental code was archived
outside the repository and restored out of the production tree after rejection.

Acceptance requires strict PDF parsing, native/WASM parity, independent Poppler
and browser PDF rendering/text checks, and old/new raster comparison. Check
combining marks, emoji, supplementary scalars, complex scripts, bidi, ligatures,
whitespace, soft wraps, explicit breaks, page continuations and more than 255
distinct atoms. Do not replace the current output merely because a prototype
emits valid PDF syntax. Reader disagreements or fidelity failures remain blockers.

## Alternatives and dependencies

- Raw Type 0/CID TrueType embedding needs exact instance materialization,
  subsetting/metrics and cluster provenance. It is a larger change, particularly
  for current variable fonts. Do not silently embed their default instances.
- A whole-line replacement code or guessed invisible text layer can make some
  extraction work but gives coarse/incorrect selection and Unicode associations.
  It is not an acceptable substitute for the intended text representation.
- Existing MIT/Apache-2.0 `pdf-writer` already supports Type 3 character procedures,
  ToUnicode CMaps and ActualText. No additional dependency is required for this
  experiment. Mature higher-level Rust exporters are considered rather than
  inventing a PDF parser/shaper, but do not justify replacing the layout engine.

## Primary sources and licensing

- [Adobe PDF 1.7 reference](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/pdfreference1.7old.pdf),
  Type 3 fonts, text rendering, ToUnicode and replacement text.
- [ISO 32000-1:2008 public copy](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf),
  §§9.6.5, 9.10 and 14.9.4: text codes, multi-scalar mappings and ActualText.
- [pdf-writer 0.15 font APIs](https://docs.rs/pdf-writer/0.15.0/pdf_writer/writers/struct.Type3Font.html)
  and [UnicodeCmap](https://docs.rs/pdf-writer/0.15.0/pdf_writer/types/struct.UnicodeCmap.html).
- [SIL OFL FAQ](https://openfontlicense.org/ofl-faq/), §§1.12–1.13: font embedding
  and produced documents. The pinned families use OFL 1.1; existing notices stay
  in source/application assets. All ten pinned fonts have OS/2 `fsType=0`.
  See `crates/office-layout/fonts/README.md` and its provenance manifest.

The third-party font licenses do not choose Writer's pending software license or
complete the planned commercial/security audit.

Direct Adobe specification downloads returned HTTP 403 in this environment;
the links remain the primary references, not a claim of a completed standards
conformance review. Available official OFL/PDF Association guidance and the
installed low-level library's documented APIs were inspected. Krilla 0.8.2 and
subsetter 0.2.6 (MIT OR Apache-2.0) were evaluated as future positioned-glyph and
variable-instance alternatives. Their published release dependency/target review
and an actual WASM build were not executed; no dependency was introduced.

## Accessibility and archival profiles

Searchable/selectable Unicode text alone does not establish accessibility.
Tagged PDF needs a logical structure tree, reading-order semantics, language,
roles and independently verified assistive behavior. PDF/A additionally needs
the selected profile's font, metadata, color and conformance requirements and an
independent validator. Investigate those separately; make no PDF/UA, WCAG or
PDF/A compliance claim for the current or experimental output.

## Executed experiments and results

Font verification passed: `python3 crates/office-layout/fonts/verify.py` checked
all ten files and retained notices. FontTools inspection found `OS/2.fsType=0`
for all ten. These are embedding/provenance checks, not security certification.

Prototype native checks passed: initial `cargo check --locked --offline -p
office-formats`; 66 layout tests including ten provenance regressions; five final
focused PDF tests; and the targeted library/provenance guards. They are rejected
experiment checks, not production features or extra shipping-suite passes.

Independent consumers: pypdf 6.10 strict parsing, PDFium through pypdfium2 5.14,
MuPDF through PyMuPDF 1.26.6, and Poppler (`pdfinfo`/`pdftoppm` 26.05,
`pdftotext` 25.03). Public fixtures covered the four-page layout reference,
project notes and a mixed-format Unicode/font-bank stress document.

| Variant | Confirmed result |
| --- | --- |
| Initial uncolored nested forms | Missing separator produced `d1q`; independently rejected. Corrected in later experiments. |
| Conventional font matrix/metrics | Fixed spurious interletter extraction spaces, but PDFium lost inherited text color. |
| Upward font-coordinate variant | Generated but not independently tested. No pass or failure claimed. |
| Colored nested forms | Poppler matched the baseline; MuPDF still changed materially. |
| Uncolored direct paths | Corrected PDFium color and nearly matched MuPDF; Poppler still changed materially. |
| Colored direct paths, final | Exact mapped Unicode and fine-grained selection; material rendering differences remained. Rejected. |

The final candidate preserved **all 771 source characters including whitespace**
in its decoded text codes and ToUnicode mappings. The stress candidate used 365
codes across two font banks; multi-scalar atoms, combining marks and emoji were
retained. PDFium supplied distinct character boxes for ordinary words and an
expected shared atom box for a combining sequence. These successes did not
override rendering failures.

At 144dpi, the final four-page candidate changed these pixel counts against the
outlined baseline in the same reader:

| Reader | Pages 1 / 2 / 3 / 4 changed pixels | Largest channel difference |
| --- | --- | --- |
| MuPDF | 23,654 / 25,976 / 1,998 / 2,116 | 148 / 255 |
| PDFium | 345 / 322 / 82 / 50 | 222 / 255 |
| Poppler | 23 / 88 / 17 / 2 | 79 / 255 |

Mixed-bidi extraction also varied: pypdf dropped/reordered chunks, while Poppler
inserted bidi wrappers. PDFium's supplementary-character search limitation was
separately reproduced with a correctly mapped embedded Type 0 control PDF; that
specific API failure is not unique to Type 3. None establishes general browser
PDF Find or accessibility compatibility.

Artifacts remain under `/tmp/writer-phase2-pdf-*`, including the old PDFs,
candidate variants and JSON acceptance reports. Final source is archived at
`/tmp/writer-phase2-pdf-prototype-source.tar.gz`; it is not active production code.
The final independent command was:

```sh
python3 /tmp/writer-phase2-pdf-acceptance.py --candidate /tmp/writer-phase2-pdf-prototype-v6-color-direct.pdf --baseline /tmp/writer-phase2-pdf-old-layout.pdf --writer tests/fixtures/layout/layout-v1.writer --report /tmp/writer-phase2-pdf-layout-prototype-v6-acceptance.json
```

It failed acceptance as described. No physical Acrobat, Preview, iOS/PWA or
assistive-technology test occurred.

After restoring the original production source, these commands passed:

```sh
cargo test --locked --offline -p office-formats --test pdf
cargo run --offline --locked --quiet -p office-formats --example export_pdf -- tests/fixtures/layout/layout-v1.writer /tmp/writer-phase2-pdf-restored.pdf
cargo run --offline --locked --quiet -p office-layout --example layout_snapshot -- tests/fixtures/layout/layout-v1.writer > /tmp/writer-phase2-pdf-restored-layout.json
python3 crates/office-formats/tests/check_pdf.py /tmp/writer-phase2-pdf-restored.pdf /tmp/writer-phase2-pdf-restored-layout.json
cmp /tmp/writer-phase2-pdf-old-layout.pdf /tmp/writer-phase2-pdf-restored.pdf
cmp /tmp/writer-phase2-pdf-old-layout.json /tmp/writer-phase2-pdf-restored-layout.json
```

All nine production PDF tests passed. The strict independent checker found four
pages, unchanged outlined behavior and maximum page-box error 0.000025pt.
Both PDF bytes and complete layout JSON equal their pre-experiment baselines.
The nine targeted cases overlap the 650-test shipping suite, not additional passes.

## Recommended follow-up

Review a pinned mature Type 0/CID exporter/subsetter with exact variable-font
instantiation and shaped UTF-8 provenance. Krilla/subsetter is a candidate, not
an accepted dependency: audit its published dependency graph, target support,
licenses and actual WASM output first. Preserve these independent acceptance
fixtures and require reader rendering, Unicode, search and selection evidence.
That font-pipeline change requires a concrete separate approval; this investigation
does not authorize it or make a PDF/A/PDF/UA claim. Other Phase 2 areas can proceed
with the reliable existing outlined behavior intact.
