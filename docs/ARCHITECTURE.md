# Architecture

This document records long-term principles and the first document model. The
workspace currently implements the repository foundation, structured Writer
content, validation, and schema-versioned JSON serialization. Editing, layout,
rendering, and format conversion remain future subsystems.

## Independent engine

The engine must remain independent of its interface. Core code is Rust and must
not depend on React, JavaScript, browser APIs, or a particular application shell.
An interface translates user intent into engine operations and presents engine
results.

Browser integration will use WebAssembly. Any browser or JavaScript bindings
belong at the integration boundary rather than in the core. Native desktop
applications should be possible without rewriting the engine.

## Separate document, editing, layout, and rendering concerns

The document model describes document content and semantics. Editing operations
change that model. Layout determines the placement of content, and rendering
draws the resulting layout. These responsibilities should have clear boundaries
so that display or editing choices do not define the document's meaning.

Document layout must eventually be platform-independent. Typography, font
handling, measurement, and pagination will require explicit policies and
verification so that host-specific behavior does not silently determine results.
The detailed interfaces and algorithms remain future decisions.

## Compatibility as a primary requirement

Microsoft Word DOCX compatibility is a primary requirement. Future model,
serialization, and layout decisions must account for faithful import, export,
and document behavior. Compatibility will be developed incrementally and checked
against documented expectations and fixtures as those capabilities are added.

The project will build an independent engine rather than embed an existing
complete Office or word-processing engine. Established low-level open-source
libraries may be used when appropriate, after evaluating their behavior,
portability, maintenance, and licensing against project needs.

## Document model

`Document` owns a stable ID, schema version, metadata including a title, ordered
sections, and named paragraph-style definitions. Each `Section` owns a stable ID,
ordered blocks, and optional reserved section properties. `Block` is a typed enum
with a `Paragraph` variant. Each paragraph owns a stable ID, ordered text runs,
an optional named style reference, and direct paragraph properties. A `TextRun`
stores a UTF-8 `String` and direct character properties.

`Document::new` creates an empty document. `Section::new`, `Paragraph::new`, and
`TextRun::new` assemble content; `Document::from_parts` validates the whole
candidate before returning a document. Read-only accessors expose order and
identity without mutable access to a validated document. These are construction
APIs, not insertion, deletion, or transaction operations.

The initial structural rules deliberately permit zero sections, empty sections,
empty paragraphs, and empty runs. A blank model does not imply a synthesized page
or paragraph. Validation preserves empty containers, run boundaries, text, and
ordering. It never repairs content, generates IDs, merges runs, or normalizes
Unicode.

IDs are opaque, caller-assigned, case-sensitive strings. They must be nonempty
and have no leading or trailing whitespace. IDs of the document, sections,
paragraphs, and style definitions share one document-wide uniqueness domain.
Applications choose an allocation scheme and preserve IDs through persistence
and future transactions. Text runs have no identity in schema 1; stable editing
anchors remain a later design decision.

### Formatting and styles

Character properties include bold, italic, underline, font family, font size,
and automatic or explicit RGB text color. Paragraph properties include alignment,
spacing before and after, line spacing, and indentation. Properties default to
unspecified (`None`). Explicit `false` and underline `none` remain distinct from
unspecified values so future style resolution can honor overrides. The model
does not choose a default platform font or compute layout defaults.

Integer units avoid floating-point ambiguity. `Twips` are signed 1/1440-inch
lengths. Paragraph spacing must be nonnegative; fixed and minimum line heights
must be positive. Negative indentation is valid, and negative `first_line`
represents a hanging indent. `HalfPoints` font sizes must be positive; 24 means
12 points. Multiple line spacing uses positive 1/240-line units; 240 is single
spacing. Color channels are bounded eight-bit values. Blank font-family names
are invalid. Integer representation bounds are enforced during deserialization;
the model imposes no additional speculative layout limits.

`NamedStyle` has a stable ID, nonblank unique case-sensitive display name,
optional `based_on` reference, paragraph properties, and character defaults.
Paragraph references and parent references resolve by ID, not display name.
Validation rejects missing references and inheritance cycles. The model stores
the style graph but does not compute effective formatting. A future resolver
will apply the nearest specified style properties and then direct formatting.
Character styles, numbering styles, theme fonts, and OOXML toggle semantics are
not implemented.

### Validation and serialization

`Document::validate` checks global identifier uniqueness, valid identifiers,
style names and references, acyclic inheritance, and formatting values. It
returns the first structured `ValidationError`; paths identify local failures.
`Document::from_parts` and document deserialization run the same validation.
Individually assembled property records can contain invalid values until they
are accepted into a document.

The persistent model format is UTF-8 JSON, currently schema version **1**. Its
version is independent of the engine crate version. A minimal valid document is:

```json
{
  "schema_version": 1,
  "id": "document-1",
  "metadata": { "title": "" },
  "sections": [],
  "styles": []
}
```

Blocks use `{"kind":"paragraph","content":{...}}`. Line-spacing and color
variants use a `kind` discriminator and snake_case names. Lengths and font sizes
serialize as integers. Optional properties serialize as `null` when unspecified;
missing optional formatting properties also mean unspecified. Structural
collections and required records are explicit. JSON object key order and
whitespace are not part of the semantic contract; array ordering is preserved.
See [the realistic sample](../tests/fixtures/writer-sample-v1.json) for supported
formatted content and style references.

`Document::to_json` validates before writing pretty-printed JSON.
`Document::from_json` checks the version before decoding model fields, then
checks shape and invariants. It returns `DocumentError` variants for unsupported
versions, JSON errors, or validation errors. Future versions are rejected even
if their payload shape differs; no migration or downgrade is implicit. The
Serde `Document` implementation also validates decoded content, but generic
Serde errors do not retain the typed `DocumentError` distinction, and an
incompatible future shape can fail before its version is inspected. Prefer
`from_json` at the persisted-document boundary.

Unknown fields, enum variants, and block kinds are rejected to prevent silent
loss of unsupported information. No opaque imported-extension container exists
yet. Schema changes must be explicit and require a new version or a documented
compatibility decision with tests. Serialization preserves supported model
information, not source JSON spelling or an original Office file.

### Unicode positions and future transactions

Text storage uses Rust UTF-8 strings. A byte offset, Unicode scalar index,
grapheme-cluster index, and UTF-16 code-unit offset are different coordinates.
No cursor or selection API exists in this milestone.

Future user-visible cursor movement and selections should use extended grapheme
cluster boundaries, accounting for combining marks, emoji sequences, and clusters
that may span adjacent formatting runs. Rust string slicing must always check
UTF-8 boundaries. External formats and APIs that use UTF-16 offsets will need
explicit, boundary-checked conversion, including handling surrogate pairs and
positions that land inside a grapheme cluster. Segmentation policy and offset
mapping are future responsibilities; this model performs no normalization,
segmentation, or offset conversion.

Future editing transactions should construct a candidate change, preserve
unaffected IDs, validate the result, and commit atomically. Invariants should
remain centralized rather than duplicated in interface code. Transaction
granularity, anchor behavior, insertion/deletion, and undo/redo are not designed
or implemented here.

### DOCX compatibility limits and extension points

Section boundaries and separate paragraph/run property sets anticipate OOXML's
organization. Integer twips, half-points, style IDs, and inheritance references
provide useful starting points without making the internal model identical to
OOXML. Section properties currently contain no page-layout fields; an absent
record and a present empty record remain distinct.

Schema 1 does not implement tables, images, headers or footers, lists or
numbering, page setup, tabs or explicit break elements, fields, revisions,
embedded objects, or unknown OOXML preservation. Plain strings have no typed
semantics for those elements. Underline and font/color properties are a small
subset of Word's formatting model. Future format work must decide how to map
units, defaults, themes, inherited toggles, unsupported elements, and source
identifiers without silently losing fidelity. Additional typed block variants
and property records should be added when their behavior is implemented and
covered by compatibility tests.

## Incremental workspace

`office-core` is the only crate in the Cargo workspace. It exposes the engine
version and document-model API. Serde and serde_json are declared in
`[workspace.dependencies]` and inherited by the crate for versioned serialization.
New crates should also inherit the workspace package settings and
lints. Additional crates will be introduced only when concrete responsibilities
require them; conceptual boundaries do not require empty crates or modules in
advance.

Writer is the first application. Word processing and DOCX compatibility establish
the initial requirements. Spreadsheets and presentations follow later without
forcing their models or application code into this foundation.

Correctness, performance, compatibility, maintainability, and architectural
stability guide development. Changes should keep responsibilities explicit and
be validated at the scope they introduce.
