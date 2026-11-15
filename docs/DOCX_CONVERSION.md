# DOCX semantic conversion completion

The later owner-approved [formatting normalization](DOCX_NORMALIZATION.md)
supersedes this milestone's exact-formatting-only read-only criteria. Content,
original preservation, structural validation and explicit export-loss disclosure
remain required.

The owner paused Step 6 on 2026-11-14 to complete missing DOCX conversion first.
Base commit: `a504e2f`; development branch: `dev/docx-conversion`. Step 5A remains
the package-validation and original-byte preservation foundation. No typography,
layout engine, merge or deployment belongs to this work.

## Accepted implementation contract

- Keep all OPC/WordprocessingML parsing, mappings and compatibility diagnostics
  in `office-docx`. The shared model remains format-neutral.
- Import either a complete supported editable document with its immutable
  original, or a preserved read-only source with explicit unsupported-content
  diagnostics. Never publish a lossy editable projection by default.
- Support paragraphs, runs, Unicode, paragraph-style references/inheritance,
  supported character/paragraph properties, and section page dimensions/margins.
  Inspect unknown elements, attributes, relationships and parts deliberately;
  an unknown formatting property must not disappear silently.
- Translate OOXML style bold/italic toggles separately from direct run values.
  Preserve current supported appearance and document the differences between
  native absolute properties and future edits to Word toggle-based styles.
- Add only necessary generic model values: page size/orientation, page margins
  and paragraph page-break-before. Use existing twips/half-point units. Do not
  add XML, relationship IDs, DOCX extension bags or browser APIs to the model.
- Evolve the model schema explicitly, with strict legacy-schema decoding and
  a tested schema-1 migration. Reject future versions. Legacy unknown fields
  and duplicate keys remain errors; migration does not reinterpret new fields
  inside an older schema.
- A generic `WriterFile` native envelope retains a validated document and an
  optional bounded original-source record (format/MIME/name/exact bytes).
  Source state is outside `DocumentSession` and undo snapshots. Native version 2
  has strict JSON/base64 handling; the full-file API reads legacy version 1.
  Document-only loaders must reject source-bearing files rather than dropping
  their original. Keep old APIs where safe.
- Browser import uses a detached Rust candidate. Serialize and save its complete
  source-bearing file through the existing IndexedDB transaction before
  activating editing. Storage/import/stale-origin failures preserve the old
  live document, history and dirty state. Unsupported sources do not replace it.
- Export regenerates only the declared supported model in Rust, validates XML
  characters/measurements, escapes values correctly and completes the package
  before returning bytes. It is distinct from downloading the exact original.
  Retained originals remain recoverable after native save/reopen and edits.
- Clearly label native files that include the original: that backup can contain
  text removed from the edited document. DOCX export represents current supported
  content; original download returns the unchanged source.

## Compatibility and test gates

The first conversion profile does not implement tables, images, fields, links,
revisions, numbering, headers/footers, themes, arbitrary character styles or
markup-compatibility processing. Such valid packages remain preserved read-only.
Known metadata/defaults require explicit handling. An importer must not assume
that every inspected package is editable.

The subsequent [relationship correction](DOCX_RELATIONSHIPS.md) accepts inspected
inert metadata, unused themes, font descriptors, cached package thumbnails and
view/proofing/session preferences. It does not resolve theme formatting, ignore
active settings, prune unused styles or implement general markup compatibility.
Original bytes survive native save/reopen; regenerated DOCX exports explicitly
warn about unmodeled auxiliary parts and metadata omitted from that copy.

Nonfinal section properties belong to the last paragraph of that section;
final properties belong at body end. Automatic line spacing uses 1/240-line
units; exact/minimum spacing uses twips. First-line/hanging indentation maps to
the existing signed first-line value. Partial line/lineRule pairs remain read-only
because application inheritance behavior differed; the exporter emits complete
pairs. Script-specific fonts and unsupported
measurements need diagnostics, not an approximation. Independent Office-application
probes showed that literal CR/LF/TAB inside `w:t`, and boundary whitespace without
effective `xml:space="preserve"`, can change visible content. Those inputs remain
preserved read-only; no trimming or normalization is performed. Explicit `w:br`,
`w:cr` and `w:tab` are mapped deliberately. Native literal CR remains exact in
Writer files; DOCX export rejects it explicitly because XML character-reference
round trips did not establish Office semantic fidelity. XML-illegal controls are
also export errors. Ordinary attribute escaping uses quick-xml's tested writer.

Native IDs and full native model equality remain `.writer` guarantees. DOCX
semantic tests compare the declared supported properties; arbitrary native IDs,
empty containers and exact run partitioning have no universal DOCX mapping.
Reject unrepresentable structures or document a deliberate normalization.

Run legacy engine checks, semantic/native-source tests, actual WASM tests,
frontend checks, production build and real-browser import/save/reopen/export
tests. Add independent interoperability checks where tools are available; do
not equate self-roundtrips with Microsoft Word compatibility. Create a review
PR; do not merge or deploy without approval.

## Sources and retained Step 6 research

- [Sections](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.sectionproperties?view=openxml-3.0.1)
- [Paragraph spacing](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.spacingbetweenlines?view=openxml-3.0.1)
- [Style toggles](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.bold?view=openxml-3.0.1)
- [Run font mappings](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.runfonts?view=openxml-3.0.1)
- [XML 1.0](https://www.w3.org/TR/xml/)

Preparatory Step 6 research identified Parley 0.13/HarfRust/Skrifa as a promising
low-level typography stack, with explicitly registered pinned fonts and no host
font discovery. Four isolated native probes and a WASM compile passed under
`/tmp/writer-layout-probe`; this is research, not an implemented layout engine or
WASM runtime parity test. Rustybuzz/ttf-parser were rejected as unmaintained per
RustSec. Primary references: [Parley](https://github.com/linebender/parley/tree/v0.13.0),
[HarfRust](https://github.com/harfbuzz/harfrust/blob/main/HARFBUZZ.md),
[RUSTSEC-2026-0206](https://rustsec.org/advisories/RUSTSEC-2026-0206.html),
[RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html).
No typography dependencies or fonts have been added to the repository.
