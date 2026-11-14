# Step 5 — DOCX compatibility

Step 5 began on 2026-11-14 with explicit user authorization. Work is on
`dev/docx-package-foundation`; no merge or deployment is authorized by this step.
This plan records the researched scope before adding format behavior.

## Independently testable sequence

1. **5A: package foundation, implemented on the development branch.** Inspect bounded OPC/ZIP packages,
   content types, relationships, and namespace-aware XML. Locate the main
   document through its relationship. Return a read-only inventory or structured
   error. Retain the exact original bytes in an immutable `DocxSource`; never
   extract files, fetch external targets, execute embedded content, or replace
   a live document. This is not document-model import/export.
2. **5B: declared import profile.** Map a specifically approved paragraph/run and
   direct-formatting subset into a complete validated core document. Define how
   edits and persistence retain unsupported originals before enabling editable
   foreign documents, following the user's selected preservation policy.
3. **5C: export/interoperability.** Emit supported WordprocessingML/OPC, test
   semantic mappings and independent application fixtures, then consider browser
   file integration separately. Do not claim Word fidelity from self-roundtrips.

The current model and native schema/format remain unchanged. `office-docx` is a
separate byte-oriented format adapter; `office-core` gains no ZIP/XML/browser
dependencies. No editing, layout, frontend or deployment change belongs to 5A.

This boundary is deliberate for future ODT, RTF, TXT, Markdown, HTML and other
formats: each future adapter maps directly to shared semantic types and owns
its own source-preservation/compatibility policy. DOCX is not an intermediate
format for them. No other format or generic registry is implemented in Step 5.

## Why the compatibility policy matters

Sections exist, but schema-1 section properties have no page size/margins/header
fields. Character styles, document defaults, theme/script-specific fonts, typed
breaks, tables/images/lists, fields, revisions, hyperlinks, and unknown semantic
extensions are not stored. Word style bold/italic toggles also differ from the
browser's nearest-specified display policy. Native strings may contain XML 1.0
illegal controls or CR, and arbitrary native IDs/empty containers do not have a
universal lossless DOCX representation.

No confirmed critical existing-engine defect blocks 5A. Those are adapter
constraints, not reasons to change valid native documents. The user selected
**preserve unsupported originals before editing**. Step 5A therefore offers
original-byte retention and inspection only; no editable projection, partial
reconstruction or `.writer` conversion is enabled. Retaining originals through edits,
new model fields/native migrations, and broad Word-default semantics require a
separate approved design. Keeping an original blob alone does not establish
correct semantic round trips after edits. 5A never claims a semantically
supported editable import.

## Initial 5A profile

Accept classic single-disk ZIP with stored/deflated members. Reject ZIP64,
encryption, self-extracting prefixes, appended data, ambiguous end-record
signatures inside archive comments, inconsistent metadata,
duplicates/case aliases, symlinks, overlaps, malformed names and integrity errors.
Preflight EOCD/central-directory bounds and counts before constructing the ZIP
library's allocated index. Read every accepted member to EOF for CRC checking;
enforce advertised and observed sizes without allocating from untrusted lengths.
Validate classic signed/unsigned streamed data descriptors against central CRC
and size fields, including descriptor bytes in overlap checks. The optional
signature is not confused with an unsigned CRC of the same value. These bounded
checks supplement the ZIP library's central/local header and member-read checks.

The first naming profile uses canonical ASCII part paths (letters, digits,
underscore, hyphen, period and slash), plus the special `[Content_Types].xml`
item. Percent escapes and other URI spellings are explicit unsupported features,
not silently decoded or repaired. Internal relationship targets may resolve dot
segments relative to their source, or use a canonical package-absolute path,
but cannot escape the package. Network-path targets (`//`) are unsupported. Case aliases
are rejected. Validate all relationship parts and targets; root
`officeDocument` must identify exactly one internal, present, correctly typed
WordprocessingML part. Never assume `word/document.xml` is its location.

Support identifying both Strict and Transitional main-document namespace
families, matched by expanded namespace names rather than prefixes. Identification
is not a claim to implement their complete semantics. Main XML must contain a
body. Inventory unknown document features without converting them. Detect
macro-enabled main content types/known VBA parts as unsupported. External
relationships may be inventoried but are never fetched; a future importer must
decide whether their semantics can be represented.

Fixed prototype limits: 16 MiB compressed input, 256 ZIP entries, 8 MiB expanded
per part, 32 MiB aggregate expanded bytes. XML is UTF-8/XML 1.0 only, at most
2 MiB per XML part, depth 64, 32,768 nodes, 64 attributes per element, 65,536 total
attributes and 128 namespace declarations per part. Reject DTDs and unsupported
declarations. Limits reject; they do not truncate, normalize or repair.
MIME type and subtype names are limited to 127 ASCII bytes each (255 including
the slash), following RFC 6838. This is checked before copying a reused content
type into the part inventory, preventing metadata allocation amplification.
Raw `<` and `=` marker counts also bound the DOM parser's initial capacity
estimates before allocation; unusually marker-dense valid text can therefore
exceed this explicit prototype profile even when its semantic node count is low.

## Low-level dependencies and validation strategy

- `zip =9.5.1`, MIT, minimal stored/deflate backend; default compressors and
  encryption disabled. `zlib-rs` backend is Zlib-licensed.
- `quick-xml =0.42.0`, MIT, resource-only token preflight before DOM allocation.
- `roxmltree =0.21.2`, MIT OR Apache-2.0, strict namespace-aware well-formedness
  and metadata parsing after the budgets have been checked.

Two XML libraries have distinct necessary roles: roxmltree validates names,
namespaces and expanded duplicate attributes, but its DOM node limit excludes
attributes and cannot pre-bound their comparison cost. A bounded quick-xml
token pass caps attributes/namespaces/depth before that work. No specialized XML
grammar or decompressor is reimplemented. The ZIP library has no public entry
budget before constructing its index, so a small classic-header preflight is
needed. Neither parser's declaration defaults are assumed to enforce the entire
profile; version/encoding/DTD rules have explicit tests.

Source review and adversarial tests also found that roxmltree 0.21.2 substitutes
U+FFFD for invalid numeric scalar references and permits some invalid namespace
declarations. The bounded preflight explicitly rejects those inputs before DOM
construction, including surrogate/out-of-range references, reserved `xmlns`
bindings, duplicate default/`xml` bindings and prefix undeclaration. Nothing is
silently normalized into a valid imported document.

## Public API and preservation contract

`inspect_docx(&[u8])` returns deterministic part metadata, the discovered main
part and namespace family, relationship counts and observed expanded size.
`DocxSource::open(&[u8])` performs the same checks and owns an immutable copy of
the complete original archive. `original_bytes()` and `into_original_bytes()`
return that exact source, including unsupported parts, ZIP metadata and comments.
They are source accessors, not DOCX reconstruction/export. Errors distinguish
resource limits, unsupported profile features, invalid archives, invalid XML and
invalid OPC metadata; callers can match error variants without exposing parser
types. Diagnostic strings are for humans, not a stable machine-readable schema.

The API has no filesystem, network, frontend or core-model dependency. The
adapter compiles for WebAssembly, but no new browser bindings or file controls
are enabled. Original bytes are retained only in memory; their association with
an editable projection, native persistence and edit-aware fidelity is future
work. Do not save a partial DOCX projection as though it preserved its source.

The handcrafted fixtures under `tests/fixtures/docx/` cover unsupported semantic
content and a Strict document with a relocated main part. They are package and
preservation references, not independent Office-application interoperability
evidence.

Registry versions, licenses, MSRV and the temporary 35-package graph were
reviewed against official RustSec database commit
`4ef2df8e806a65e2f1ecc14f56f51249062b23d6`. Present versions satisfy applicable
patched ranges. This was a manual advisory review, not `cargo audit`. OSV lookup
was blocked with HTTP 403. A temporary native ZIP/XML probe and a
`wasm32-unknown-unknown` compile passed. ZIP enables a WASM getrandom backend;
the adapter is kept separate and does not add bindings to the existing core.

## Primary sources

- [ECMA-376 parts](https://ecma-international.org/publications-and-standards/standards/ecma-376/) and [Microsoft package creation](https://learn.microsoft.com/en-us/office/open-xml/general/how-to-create-a-package).
- [WordprocessingML structure](https://learn.microsoft.com/en-us/office/open-xml/word/structure-of-a-wordprocessingml-document) and [Strict format](https://www.loc.gov/preservation/digital/formats/fdd/fdd000400.shtml).
- [Markup compatibility](https://learn.microsoft.com/en-us/office/open-xml/general/introduction-to-markup-compatibility).
- [Bold toggles](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.bold?view=openxml-3.0.1), [sections](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.sectionproperties?view=openxml-3.0.1), [run fonts](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.runfonts?view=openxml-3.0.1).
- [XML 1.0](https://www.w3.org/TR/xml/), [Namespaces in XML 1.0](https://www.w3.org/TR/xml-names/), [whitespace](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.texttype.space?view=openxml-3.0.1), [line-spacing tokens](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.linespacingrulevalues?view=openxml-3.0.1).
- [MIME restricted-name bounds](https://www.rfc-editor.org/rfc/rfc6838#section-4.2) and [Strict root relationship URI](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/32be961f-d71a-4812-913f-b675c79aa88a).
- [zip source/features](https://docs.rs/crate/zip/9.5.1/features), [quick-xml](https://docs.rs/quick-xml/0.42.0/quick_xml/), [roxmltree options](https://docs.rs/roxmltree/0.21.2/roxmltree/struct.ParsingOptions.html).
- [Classic data-descriptor layout](https://docs.rs/crate/zip/9.5.1/source/src/format/data_descriptor.rs) and [ZIP descriptor API limitations](https://docs.rs/zip/9.5.1/zip/read/struct.ZipArchive.html#method.by_index_with_data_descriptor).
- [RustSec database](https://github.com/RustSec/advisory-db), [quick-xml attribute DoS](https://rustsec.org/advisories/RUSTSEC-2026-0194.html), [namespace DoS](https://rustsec.org/advisories/RUSTSEC-2026-0195.html), [ZIP allocation DoS](https://rustsec.org/advisories/RUSTSEC-2026-0429.html).

Direct normative archive downloads returned HTTP 403; research used the official
ECMA index and Microsoft standards/API sources. Full normative PDFs and actual
Word/LibreOffice interoperability have not been inspected/tested in 5A.

Executed commands, results and testing limits are recorded in
[DOCX_VERIFICATION.md](DOCX_VERIFICATION.md).
