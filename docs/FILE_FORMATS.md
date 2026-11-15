# Writer file formats — Step 7.5

Step 7.5 reuses the Rust model, transactions, layout, native files and existing
DOCX adapter. Additional conversion lives in `office-formats`; browser bindings
remain in `office-wasm`. No external format structures enter `office-core`.
No browser document/layout engine, storage service or server conversion is used.

| Format | Open/edit | Save As | Preservation contract |
| --- | --- | --- | --- |
| `.writer` | Full supported native model | Full native file | Stable identifiers, exact model distinctions, metadata/styles and optional original |
| DOCX | Declared content profile with known formatting normalization | Existing Rust DOCX exporter | Exact originals retained; formatting notices and content safeguards in [DOCX_NORMALIZATION.md](DOCX_NORMALIZATION.md) |
| ODT | Declared paragraph/span/style profile | ODF text package | Unsupported rich content remains preserved read-only |
| RTF | Declared text/run/paragraph/style profile | Unicode RTF | Unsupported rich content remains preserved read-only |
| TXT | Strict Unicode plain text | UTF-8, LF line endings | Original encoding/newlines retained separately, formatting is not portable |
| PDF | Export only | Rust-laid-out vector pages | Fixed appearance, no editable model or retained original |

These are explicit supported profiles, not claims of complete Word/ODF/RTF
compatibility. The presence of a format in Open does not make every construct
of that format editable. Unsupported originals are never silently replaced by
partial semantic documents. Data-loss warnings do not imply hidden support for
tables, images, fields, numbering, headers/footers or other absent model features.

## Open, Save and Save As

Open retains the local document library and adds a file picker for `.writer`,
DOCX, ODT, RTF and TXT. PDF is identified as export only and refused on import.
Extensions/MIME select an adapter; the Rust parser still validates actual bytes.
Malformed input preserves the active document and editing history.

External import builds a detached Rust session. Its complete native file,
including the exact nonempty original, must commit to IndexedDB before the new
document becomes editable. Storage failure leaves the current work intact.
Unsupported rich input has diagnostics and an original-download path, without
an editable partial projection. Empty TXT is editable without inventing retained
bytes because a native original record is intentionally nonempty.

Save/Ctrl+S and autosave continue saving native Writer state locally through the
existing serialized etag lane. Save As/Ctrl+Shift+S selects a format and filename,
requests a Rust assessment tied to session/document/revision and presents only
brief meaningful-loss warnings. Save copy confirms that displayed assessment;
ordinary saves have no acknowledgment checkbox or technical panel. A changed document
invalidates that assessment. Bytes are fully produced before a download starts.
Downloads are copies; they do not mark a newer local revision saved, replace its
etag or imply that an original external file was overwritten.

Keep a `.writer` copy for stable IDs, exact direct/inherited-property distinctions,
native styles and original recovery. Foreign-format identifiers/run partitions
are not exact native guarantees. Editing history is not persisted in any current
format. Native files retaining originals may include text or metadata removed
from the current document; the existing disclosure applies to every source type.

Source-aware DOCX copies preserve validated source properties and auxiliary parts
where safe, while regenerating current body/styles. Use `assess_file` / `export_file`
when a complete `WriterFile` is available; a document-only exporter cannot recover
information it was not given. Safe preservation and meaningful-loss policy are
specified in [SAVE_COPY.md](SAVE_COPY.md).

## TXT policy

- UTF-8 is decoded strictly, with an optional leading UTF-8 BOM.
- UTF-16LE/BE requires its BOM; odd lengths and unpaired surrogates fail.
- UTF-32, guessed legacy encodings, invalid UTF-8 and NUL-containing text fail.
- CRLF, CR and LF become paragraph boundaries. Trailing newlines create trailing
  empty paragraphs. Interior U+FEFF, combining marks, emoji and script text retain
  their spelling, without Unicode normalization.
- Export concatenates text runs in order, normalizes CRLF/CR even across run
  boundaries and separates paragraphs with LF. A semantic first U+FEFF is
  protected with an additional transport BOM so reopening cannot consume it.
  NUL-containing native text is refused before export, rather than creating a
  TXT file that this declared profile cannot reopen.
  Rich formatting, styles, title metadata, section geometry and distinctions
  between paragraph boundaries and literal soft line breaks do not survive TXT.
  The warning is explicit even when a particular document is currently simple.

TXT has a 16 MiB decoded UTF-8 / 10,000-paragraph adapter limit. Browser limits
remain lower: 5 MiB native files, 2,000 paragraphs and existing text/run budgets.

## ODT and RTF profiles

ODT uses the existing low-level ZIP/XML libraries. Packages are read in memory,
never extracted; encrypted/linked/duplicate/overlapping or excessive members,
unsafe names, DTD/entity hazards and malformed XML are rejected. The ODF MIME
record, manifest and semantic parts are validated. Supported paragraph/text
properties map into the shared model rather than being stored as ODF nodes.
Physical decimal lengths convert deterministically to the nearest native twip
(halfway values away from zero); the exact source spelling remains in the
original. This is an explicit unit-conversion rule, not Unicode normalization.

RTF uses the low-level borrowed `rclip-rtf` tokenizer, with a strict Writer
semantic adapter. It checks group nesting, parameter ranges, binary bounds,
Unicode fallback, surrogate pairs and destinations. The clipboard library's NUL
skipping/numeric saturation policies cannot silently reinterpret native files.
Supported code-page decoding is explicit; unknown/undefined text bytes are not
replaced with plausible text. Unknown rich semantics prevent editable import.

The exact implemented profile and independent-consumer findings belong in the
verification report. Ordinary third-party documents can include unsupported
properties even when their visible text appears simple. Preserve originals and
report those limitations rather than inferring fidelity from self-roundtrips.

## PDF output

The existing Rust layout engine provides physical pages, positioned glyphs,
outlines, colors and underlines. `pdf-writer` writes the PDF object structure;
`svgtypes` parses existing paths/transforms. The adapter does not shape text or
paginate a second time, discover host fonts or ask the browser to print CSS.
Each reusable outline becomes a vector form. Page coordinates are converted
from fixed 1/64-point top-left coordinates into PDF bottom-left points.

This initial PDF profile is intentionally **outlined**: text is not searchable
or selectable, and the file is not PDF/UA tagged or a PDF/A conformance claim.
Save As warns before export and includes existing layout diagnostics, including
font substitution or missing glyphs. Tabs and other unsupported layout input
remain structured failures. No source bytes or native IDs are embedded in PDF.

## APIs and bounded failures

`office-formats` exposes concrete `DocumentFormat`, `FormatError`,
`FormatDiagnostic`, `ImportOutcome` and `ExportReport` values, with:

```text
import_document(format, bytes, document_id, filename)
assess_export(format, document)
assess_file(format, writer_file)
export_document(format, document)
export_file(format, writer_file)
```

The format-neutral full native file remains `WriterFile`. Document-only
`export_document(Writer, ...)` explicitly fails because it cannot know whether
an original exists. `export_file(Writer, ...)` preserves the complete native
file and source; browser-native export also uses the full-file serializer.
Model-only assessment does not know about source retention, so interfaces must
add the existing original-content privacy disclosure when sharing native files.
Generic WASM
`import_format`, `export_assessment` and `export_format` preserve existing DOCX
and Writer APIs, validate source records through their own adapter, guard stale
origins and require confirmation of meaningful warnings through Save copy.
The legacy WASM `export_docx` remains a model-only regeneration primitive;
user-facing copies use the source-aware assessed `export_format` route.
Source state stays
outside undo snapshots. No native schema or IndexedDB migration is required.

Adapters cap input at 16 MiB (native files retain their existing 32 MiB limit),
generic output at 32 MiB, PDF output at 16 MiB, and imported document-ID labels
at 1,024 UTF-8 bytes. ODT additionally bounds expanded bytes, members and XML;
RTF bounds tokens, nesting, style tables and paragraphs. Diagnostics and browser
transport are bounded. Limits fail explicitly without publishing partial output.
Editable external output is also revalidated against its adapter's import profile
before bytes are returned. An unusually large font/style label, numeric value or
generated structure cannot produce a copy that Writer itself cannot reopen.
This extra validation is limited to explicit file conversion, not typing. It
establishes reopenability within the profile, not full semantic or Office fidelity.

## Licensing and verification

The project license remains undecided; no project `LICENSE` was added. Existing
low-level libraries are reused where suitable. New dependency choices and
licenses are recorded in [DEPENDENCIES.md](DEPENDENCIES.md); source/distribution
notices for the new graph are in `crates/office-formats/THIRD_PARTY_NOTICES.txt`.
This targeted review is not the planned comprehensive commercial-licensing audit.
LibreOffice and PDF inspection/rendering utilities are independent test tools,
not linked, embedded or shipped as Writer's conversion engine.

Run the existing Rust, compiled-WASM, frontend and production-browser gates.
Format tests must import, perform actual Rust edits, export and reopen, preserve
original bytes, exercise invalid/unsupported input and verify warnings/staleness.
Use independent consumers for exported ODT/RTF/PDF and record their limitations.
No deployment or additional format is authorized by this milestone.
