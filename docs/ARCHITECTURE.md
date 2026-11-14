# Architecture

This document records long-term principles and the first document model. The
workspace currently implements the repository foundation, structured Writer
content, validation, schema-versioned JSON serialization, and Unicode-aware
positions and selections. A runtime session now provides atomic validated
transactions. Text-editing operations, layout, rendering, and format conversion
remain future subsystems.

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
and future transactions. Text runs have no identity in schema 1. Positions use
document and paragraph IDs with a logical grapheme offset; rebasable editing
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

### Unicode positions and selections (3A)

Text storage uses Rust UTF-8 strings. A byte offset, Unicode scalar index,
grapheme-cluster index, and UTF-16 code-unit offset are different coordinates.
Milestone 3A introduces explicit `Utf8Offset`, `Utf16Offset`, and `GraphemeOffset`
types. Their constructors wrap unsigned numeric requests, not proofs of valid
boundaries; conversions check them against actual paragraph text.

`TextCoordinates::new` takes an owned snapshot of the exact logical paragraph
text by concatenating runs without separators. Segmentation uses extended
grapheme clusters from unicode-segmentation, resolved to version 1.13.3 in
`Cargo.lock`, with Unicode **17.0.0** data. `GRAPHEME_UNICODE_VERSION` exposes the
algorithm's Unicode version. Formatting changes, empty runs, and run boundaries
do not split combining sequences, emoji sequences, or other clusters. Original
text and run boundaries are preserved in the document; no normalization occurs.
Unicode dependency updates require review with matching conformance fixtures,
because boundary changes can affect the meaning of a grapheme offset.

Raw UTF-8/UTF-16 conversion accepts any Unicode scalar boundary. It rejects byte
offsets inside a UTF-8 scalar, code-unit offsets inside a surrogate pair, and
out-of-range values. A scalar boundary inside a grapheme cluster remains valid
for these raw conversions. Conversion to `GraphemeOffset` adds a strict extended
grapheme boundary check; an interior scalar boundary is an error. No conversion
snaps, clamps, or silently repairs an offset. Offsets are paragraph-relative,
zero-based, and include the end boundary: N clusters have N+1 valid cursor
positions, numbered 0 through N.

`Document::position` checks a grapheme offset against the identified paragraph.
The returned `DocumentPosition` stores stable document and paragraph IDs and a
grapheme offset, not a section index, run index, or imported-format coordinate.
`Document::validate_position` checks document identity, paragraph membership, and
bounds. Empty paragraphs have boundary zero. Empty documents and sections have
no paragraph cursor position; no default content is synthesized.

`Document::selection` checks both endpoints and preserves anchor/focus direction.
Equal endpoints form a collapsed selection. `Selection::ordered_range` returns
their logical half-open range in section, block, and grapheme order. IDs are never
sorted lexically. The end of a paragraph and the start of the next paragraph are
distinct positions; the API does not invent newline offsets, extract selected
text, or decide how future deletion treats structural boundaries. Logical order
also differs from visual bidirectional cursor movement; visual movement and
layout affinity are outside 3A.

Positions, selections, and coordinate snapshots are runtime values without Serde
implementations. The document model and schema version 1 are unchanged. They
operate independently of a frontend or future DOCX mapping. Positions are not
revision-aware or automatically rebased: revalidation detects missing elements
and out-of-range offsets but cannot detect changed text meaning under the same
IDs. Session revision tokens in 3B provide a conservative origin check before
reuse; callers retain the token associated with each raw position or selection.
Editing affinity and rebasing remain future work.

Building a coordinate snapshot takes O(paragraph text length) time and stores the
text plus grapheme boundaries. Grapheme-to-byte lookup is O(1), byte-to-grapheme
lookup is O(log number of clusters), and UTF-16 conversions scan text in O(text
length). Document paragraph lookup scans sections and blocks. No document index
or coordinate cache is added until performance evidence calls for one.

Tests include exact offset cases, combining marks and emoji across run
boundaries, invalid scalar/surrogate boundaries, RTL logical ordering, and the
official Unicode 17 grapheme-break conformance fixture. These checks establish
logical boundary behavior rather than visual editing behavior.

### Atomic validated transactions (3B)

`DocumentSession` owns one validated committed `Document` and a session-scoped
`Revision`. Committed access is immutable. The document model, construction
invariants, and schema version 1 remain unchanged; sessions and transactions are
runtime values without serialization. Reopening a saved document creates a new
session lineage at revision zero.

`begin_transaction` returns a detached `Transaction` with the originating token,
fixed document ID, and a `DocumentDraft`. The draft contains cloned metadata,
sections, and named styles. Callers can assemble candidate records without
mutating the committed state. Intermediate drafts may violate invariants; only
the final candidate is checked. The document ID and schema cannot be changed
through the draft. These staging records provide a transaction boundary for
future operations; 3B adds no insertion, deletion, or formatting-operation API.

Commit consumes the transaction on success or failure and proceeds in this order:

1. Reject a token issued by another session.
2. Reject an originating revision that differs from the current revision.
3. Build and validate the complete candidate with `Document::from_parts`, using
   the same model validation as construction and loading.
4. Compare the candidate with the committed document. Structural equality is a
   no-op: it succeeds without replacing content or advancing the revision.
5. For a changed candidate, check that incrementing the revision cannot overflow,
   then publish the document and new revision under one exclusive Rust borrow.

`CommitOutcome` reports the resulting token and whether content changed.
Different run boundaries or property representations count as changes even if
the displayed text would look identical. Validation never repairs, normalizes,
or merges candidate content. Stale candidates are rejected before equality is
considered, so a stale draft cannot succeed merely because its content matches.

`TransactionError` distinguishes foreign sessions, stale revisions, invalid
documents, and revision exhaustion. Every rejection leaves both committed
document and revision untouched. Dropping a transaction aborts it; a panic in
caller-side staging also cannot mutate the session. No user callbacks run during
publication. Atomicity here means in-memory visibility through `&mut self`, not
durable storage, a lock-free protocol, or distributed concurrency. Start a fresh
transaction after a conflict; no automatic retry or merge occurs.

Revision tokens contain a private `Arc` identity and a checked `u64` counter.
Equality checks both session identity and counter. Two independent sessions with
the same document ID and revision number still have different tokens. The number
returned by `Revision::number` is diagnostic, not a globally comparable identity.
Sessions and transactions cannot be cloned; tokens and detached construction
records can. Changed commits advance the counter once; no-op commits preserve
it. Counter exhaustion rejects changed commits without wrapping, while a no-op
can still succeed.

Any changed commit invalidates all previous revision tokens conservatively,
including tokens associated with unchanged paragraphs. When computing raw 3A
positions or selections, capture `session.revision()` alongside them. Before
reuse, call `session.validate_revision(&origin)` and then the existing structural
position or selection validation against `session.document()`. Never restamp an
old position with a new token. The token check cannot prove the association with
a separately supplied raw position; callers retain that provenance. Positions
do not become revision-aware or automatically rebased in 3B. This policy rejects
old origins even after content changes from A to B and back to A.

Beginning a transaction copies candidate content in O(document size) time and
memory. Commit validates the complete document and compares structural equality,
also requiring work proportional to document size. No persistent tree, operation
framework, cache, event stream, or history is added without demonstrated need.

### Later editing milestone

Milestone 3C will add text editing operations, formatting operations, and undo/redo
on the transaction foundation. It remains unstarted. The position, transaction,
and operation milestones retain separate acceptance checks.

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
version, document model, position/selection APIs, and transaction sessions. Serde,
serde_json, and unicode-segmentation are declared in `[workspace.dependencies]`
and inherited by the crate for serialization and Unicode segmentation.
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
