# Architecture

This document records long-term principles and the first document model. The
workspace currently implements the repository foundation, structured Writer
content, validation, schema-versioned JSON serialization, and Unicode-aware
positions and selections. A runtime session now provides atomic validated
transactions, Unicode-safe text and formatting operations, and bounded undo/redo.
Native Writer persistence now separates byte serialization from filesystem
storage. Step 4.5 adds a Rust/WebAssembly session boundary and a semantic browser
prototype. Professional layout, platform-independent rendering, and Office
format conversion remain future subsystems.

## Independent engine

The engine must remain independent of its interface. Core code is Rust and must
not depend on React, JavaScript, browser APIs, or a particular application shell.
An interface translates user intent into engine operations and presents engine
results.

Browser integration uses WebAssembly through `office-wasm`. Browser and
JavaScript bindings belong at that boundary rather than in `office-core`. Native
desktop applications should be possible without rewriting the core engine.

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
Validation rejects missing references and inheritance cycles. The core model
stores the style graph without resolving effective formatting. The browser
adapter adds a limited Rust display policy for inheritance and direct properties;
it does not alter the optional source values. Character styles, numbering styles,
theme fonts, and OOXML toggle semantics are not implemented.

### Validation and serialization

`Document::validate` checks global identifier uniqueness, valid identifiers,
style names and references, acyclic inheritance, and formatting values. It
returns the first structured `ValidationError`; paths identify local failures.
`Document::from_parts` and document deserialization run the same validation.
Individually assembled property records can contain invalid values until they
are accepted into a document.

The document payload representation is UTF-8 JSON, currently schema version
**1**. Its version is independent of the engine crate version and the native
file envelope version. A minimal valid model payload is:

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
`from_json` at the raw-model boundary; native `.writer` files use the persistence
API below, which delegates payload decoding to that same loader.

Unknown fields, enum variants, and block kinds are rejected to prevent silent
loss of unsupported information. No opaque imported-extension container exists
yet. Schema changes must be explicit and require a new version or a documented
compatibility decision with tests. Serialization preserves supported model
information, not source JSON spelling or an original Office file.

### Native Writer persistence (Step 4)

The `.writer` extension provisionally identifies the engine's internal native
format. Its specification may evolve before public release. It is an uncompressed
UTF-8 JSON envelope, without ZIP packaging or a custom binary layer:

```json
{
  "format": "office-writer",
  "format_version": 1,
  "document": {
    "schema_version": 1,
    "id": "document-1",
    "metadata": { "title": "" },
    "sections": [],
    "styles": []
  }
}
```

`WRITER_FORMAT_NAME`, `WRITER_FORMAT_VERSION`, and `WRITER_EXTENSION` expose
these native format values. `format_version` describes the file envelope;
`document.schema_version` describes the enclosed model. The engine's application
version is separate and is not written into the file. Only native format 1 and
model schema 1 are currently supported. Bare model JSON remains useful through
`Document::from_json` but is not accepted as an implicit `.writer` file.

`serialize_document(&Document)` validates the complete model and returns
`Vec<u8>`. The writer uses two-space pretty indentation, fixed envelope field
order (`format`, `format_version`, `document`), and a single trailing LF. Array
order and exact supported model values are preserved, including text runs,
explicit false versus unspecified properties, section properties, IDs, styles,
metadata, and Unicode strings. Serialization adds no timestamp or random value.
Repeated output for the same model is deterministic; it is not a cryptographic
JSON canonicalization scheme. JSON spelling, indentation on input, and object key
order are not preserved as document information.

`deserialize_document(&[u8])` first checks UTF-8, then the format identifier and
numeric native version before decoding document fields. A strict Serde envelope
keeps the original document object as borrowed `RawValue`, then calls the existing
`Document::from_json` loader. This avoids a competing model decoder and prevents
a `Value` intermediate from silently overwriting duplicate keys. Model validation
therefore remains identical to construction, editing commits, and raw-model
loading. Unknown envelope fields, unsupported kinds or properties, malformed
structures, duplicate keys, and unsupported versions are rejected without repair
or content loss. UTF-8 BOM input is rejected as invalid JSON; ordinary JSON
whitespace and equivalent escaped string spellings are accepted.

`PersistenceError` distinguishes invalid UTF-8, JSON/envelope failures, an unknown
format marker, unsupported native versions, and existing structured `DocumentError`
failures. Native format and model-schema version errors remain distinct. Future
interfaces can translate these error categories without parsing display strings.
See [the native fixture](../tests/fixtures/native/writer-v1.writer) for a realistic
multi-section document; the model-only fixture continues to test schema 1.

#### Serialization and native storage boundaries

Byte serialization has no filesystem or browser dependency. On targets other
than `wasm32`, `save_document(&Document, path)` and `load_document(path)` supply a
native filesystem adapter using `AsRef<Path>`. Native paths are retained as
`Path`/`PathBuf`, supporting Unicode filenames without a UTF-8 conversion step.
The adapter neither appends `.writer` nor creates missing parent directories.
The full document and file bytes are held in memory; streaming and resource
limits are not introduced in this milestone.

Saving follows this order:

1. Validate and serialize the complete document before filesystem changes.
2. Resolve and inspect the destination. Reject symlinks, non-regular files, and
   read-only existing files, even if the process could otherwise replace them.
3. On Unix, open the parent directory before replacement.
4. Exclusively create a uniquely named temporary file in the destination
   directory. Its short `.writer-` prefix does not depend on the target filename.
5. Write all bytes and flush the writer, then copy existing ordinary permissions
   when replacing a regular file. Synchronize the completed temporary data and
   metadata only after those permissions have been applied.
6. Persist the prepared file using the platform's replacement operation, without
   deleting the original destination first.
7. On Unix, synchronize the already-open parent directory after replacement.

The native-only `tempfile` dependency supplies exclusive unique-file creation,
RAII cleanup, and portable `persist` behavior. It avoids a home-grown naming or
replacement implementation. On failure before persistence, the adapter explicitly
attempts to close and remove the temporary file. A persistence failure similarly
attempts to clean up the returned temporary file. Cleanup failure is retained
alongside the primary I/O error and temporary path. The file may remain; an
explicit close failure does not guarantee an automatic retry. RAII cleanup covers
ordinary drop or unwinding paths, and is also best-effort. Cleanup cannot be
guaranteed after a process kill, changed permissions, or filesystem failure.

`StorageError` wraps format failures with the requested path, or an I/O failure
with a `StorageOperation`, path, original `io::Error`, `ReplacementState`, and
optional secondary cleanup error. `NotAttempted` means the replacement operation
was not invoked and the existing destination was untouched. `Uncertain` means
replacement returned an error: remote or unusual filesystems may not establish
whether it happened. `Completed` means replacement succeeded before a subsequent
directory synchronization failed; the new file is already installed. Callers
must not treat every save error as proof that the old file remains, or attempt an
automatic rollback after replacement.

Local replacement and synchronization semantics depend on the operating system,
filesystem, mount, and storage hardware. Windows has no equivalent parent-directory
synchronization step in this adapter. No universal atomic-replacement or power-loss
durability guarantee is made. Ordinary permissions do not include ownership, ACLs,
extended attributes, timestamps, or hard-link identity; these are not preserved.
There is no writer locking or conflict detection: concurrent saves are
last-writer-wins, and parent paths must remain stable and trusted during the
operation. Loading follows normal operating-system symlink behavior, checks for a
regular file before opening, and checks the opened handle again before reading.
It then reads the complete bytes and uses the same strict decoder. These checks
do not claim protection against adversarial concurrent pathname changes.

#### Compatibility and future adapters

Loading never changes the source file. Unsupported native or model versions fail
explicitly; no migration, downgrade, or fallback is implemented. When a second
version is needed, add an explicit supported source/target transformation with
fixtures and losslessness expectations, validate its result, and retain tests for
every supported version. An old version's data must not silently acquire new
meaning under a newer decoder. The provisional format may change before public
release only through an explicit versioning decision.

Saving borrows the immutable document and does not alter committed content,
revision tokens, pending drafts, editing endpoints, or undo/redo. Files contain
only document values; session identity, history, revisions, and endpoints are not
persisted. Loading returns a validated `Document`; creating a new session from it
starts a fresh revision lineage and empty history.

Step 4.5 transports these same bytes through IndexedDB and native file
import/export. Browser storage has its own completion, quota, eviction, and
conflict semantics; it does not inherit the native filesystem guarantees above.
DOCX import/export remains a separate future adapter to and from the typed model,
requiring its own researched compatibility policy and approval. Autosave and
cloud synchronization are not implemented.

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
mutating the committed state. Raw intermediate drafts may violate invariants;
commit checks the final candidate. The document ID and schema cannot be changed
through the draft. These staging records provide the transaction boundary used
by the editing operations introduced in 3C.

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
Each changed commit retains the preceding document as one undo snapshot when
history is enabled, and clears redo. No-op and failed commits preserve history.

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
also requiring work proportional to document size. No persistent tree, document
cache, or event stream is added without demonstrated need.

### Text and formatting operations (3C)

`Transaction::position` and `Transaction::selection` create `EditPosition` and
`EditSelection` values tied to the current draft by a private identity stamp.
Only endpoints issued by the same unchanged transaction can be used by an
operation. Raw 3A inspection values cannot be supplied directly. Successful
changed operations rotate the stamp, as does every call to `draft_mut`, even if
the caller changes no value. Stale and foreign endpoints return `WrongContext`.
Failed and no-op operations preserve existing editing endpoints.

Every operation builds and validates a complete candidate before replacing the
transaction's draft. Invalid input leaves earlier successful staging intact;
callers can drop the transaction to abort the entire batch. Committed state
remains untouched until commit succeeds. Endpoints and text coordinate snapshots
must be reacquired after a changed operation. Final commit
still checks the originating session revision, so a detached draft cannot
overwrite a later commit. These runtime types do not change schema version 1.
Creating editing endpoints or applying an operation requires the current draft
to be valid; repair invalid raw staging records before using those helpers.

Insertion, deletion, and replacement use extended-grapheme boundaries across
the complete paragraph text. Inserted UTF-8 is preserved exactly, including CR
and LF as literal text; no paragraph IDs or structural breaks are synthesized.
Inserted runs receive explicitly supplied direct `CharacterFormat`, avoiding
an implicit typing-affinity or style-resolution policy. New adjacency can merge
graphemes, such as an inserted combining mark joining preceding text. Core
operations return a changed flag without an implicit result cursor or automatic
rebasing. The browser adapter's explicit caret policy is separate.

A same-section multi-paragraph replacement preserves the start paragraph's ID,
style reference, and paragraph properties. It combines its retained prefix,
inserted text, and the retained end paragraph suffix, removing covered paragraphs
including the end paragraph. Other section properties and unaffected content
remain intact. Replacement across sections is rejected with a structured error
until section-joining semantics are defined. Reverse selections use their ordered
half-open range. Deleting all text leaves a valid empty paragraph.

Text replacement retains empty runs strictly before its start and at or after
its end, removing empty runs within the replaced interval. An empty run at an
insertion cut belongs to the suffix. Identical replacement text with matching
direct properties preserves the original run partition as a no-op.

`CharacterFormatPatch` and `ParagraphFormatPatch` use `PropertyChange::Keep`,
`Set(value)`, and `Clear` for each optional property. Clearing restores an
unspecified value, distinct from explicit `false` or underline `none`. Setting
indentation replaces its complete override record. Supplied property values are
validated even for an empty selection. Patches do not resolve style inheritance.

Character formatting applies only to selected text and may span sections. It
splits runs at selected boundaries only when properties change, preserves
unselected content and empty runs, and performs no run coalescing. An already
matching patch preserves the original run boundaries and produces no history.
Collapsed character formatting is a no-op, not a typing-state change. Paragraph
formatting and named-style assignment target paragraphs touched by the logical
range; a collapsed selection targets its paragraph. A noncollapsed range ending
at offset zero excludes that final paragraph. Style assignment validates the
reference, and clearing it leaves direct paragraph properties unchanged.

Operation setup currently validates and copies the candidate document; text
operations additionally scan affected paragraphs and formatting operations scan
the document structure. The implementation favors the shared validator and
explicit behavior over incremental indexes or structural sharing. No frontend,
visual bidi navigation, DOCX mapping, or layout dependency is introduced.

### Bounded undo and redo

History belongs to `DocumentSession`. Each changed commit creates one undo step,
including commits assembled from raw draft records and transactions containing
multiple operations. Stored snapshots restore exact document values: stable IDs,
run boundaries, text, metadata, styles, and direct formatting. There is no typing
coalescing or operation grouping beyond the transaction boundary.

The default `DEFAULT_HISTORY_LIMIT` is 100 retained undo snapshots.
`with_history_limit` sets another count; zero disables history. Pruning discards
the oldest retained undo snapshot. This bounds snapshot count, not memory bytes:
each snapshot is a complete document and may be large. Undo transfers the current
document to redo; redo transfers it back to undo. New changed commits clear redo.
The total number of retained undo and redo snapshots stays within the limit.
No-op commits and all rejected commits leave both stacks unchanged.

Undo and redo validate the target snapshot and check revision exhaustion before
changing document, stacks, or revision. Success publishes a fresh monotonically
increasing revision; it never restores an earlier token, even when an earlier
document value returns. Pending transactions and old position-origin tokens
therefore remain stale. An empty stack returns `NothingToUndo` or `NothingToRedo`
without changing state. `clear_history` discards both stacks without changing
committed content or its revision.

History, draft identity stamps, and session revisions are runtime state. They are
not serialized into documents or restored by loading. Undo/redo is in-memory
history rather than durable recovery or a collaboration protocol. The position,
transaction, and operation milestones retain separate acceptance checks.

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

## Browser Writer prototype (Step 4.5)

The prototype is implemented on `dev/writer-browser-prototype`; local verification
passes as recorded in [VERIFICATION.md](VERIFICATION.md). No merge or deployment
is implied. Step 4's native persistence
foundation is already merged at `52ac4e9` through PR #6. The engineering rules in
[AGENTS.md](../AGENTS.md), primary-source evidence and retrospective findings in
[RESEARCH.md](RESEARCH.md), and decisions 021 through 026 govern this addition.
The research record contains the full source links and alternatives rather than
duplicating them here.

### Ownership and transport

`office-wasm` owns a Rust `BrowserEngine`, which owns the live `DocumentSession`,
directed selection, typing properties, and cursor history associated with core
undo steps. `wasm-bindgen` exposes a small owning `WriterEngine` handle to the
browser. The binding crate also builds natively so command, Unicode, persistence,
and presentation policy can be tested outside a browser. `office-core` remains
independent of React, JavaScript, browser storage, and browser APIs.

`apps/writer` uses React, TypeScript, Vite, and CSS to render immutable presentation
snapshots, dispatch commands, and manage dialogs, dirty/save state, DOM selection,
composition, and browser storage. It maintains neither an editable JavaScript
document model nor an independent undo stack. JSON command/snapshot DTOs are
transport records, not another persisted document schema. Mutating a presentation
value cannot change Rust-owned content.

Commands return structured `{ok, view}` or `{ok, error}` responses. Errors carry
a machine-readable code and a human-readable message. Session/revision handles
cross the boundary as opaque strings; JavaScript never rounds a `u64` counter
through its numeric type. Commands carry an expected revision so delayed input
can be rejected. A cryptographic UUID namespace supplied by the host and a
checked Rust counter allocate new identifiers; imported IDs remain opaque. DOM
lookup uses identity maps rather than interpolating IDs into CSS selectors.

### Browser input and Unicode policy

The editing surface is a labeled multiline `contenteditable` textbox with
semantic paragraphs and text spans. Rust snapshots control its committed content.
Supported cancellable `beforeinput` commands become Rust transactions; unhandled
or uncancellable `input` changes are reconciled through a supported command or
restored from Rust with an error. Native browser navigation and directed
selection remain available. Reconciliation retains unchanged presentation nodes
where practical, and selection restoration is conditional rather than applied
after every navigation or toolbar focus change.

During an active IME composition, the browser DOM is deliberately a transient
buffer. Reconciliation of that buffer pauses while its original Rust revision
and selection are retained. Completion commits the final replacement once;
cancellation and duplicate final-event paths must not delete text or double-commit.
This exception concerns temporary browser interaction, not authority over the
committed document. Browser undo/redo commands dispatch to Rust history.

DOM text offsets are UTF-16 code units, while element offsets address child nodes.
The presentation adapter maps both to stable paragraph identities and UTF-16
positions. JavaScript string length is used only for DOM UTF-16 bookkeeping, not
for grapheme counts. Rust's pinned Unicode 17 rules remain authoritative across
all formatting runs, including combining sequences and multi-code-point emoji.

The strict core conversions still reject surrogate interiors and invalid cursor
boundaries. By default, a collapsed browser endpoint inside a grapheme moves to
its following boundary and a range expands outward, preserving direction.
Navigation requests explicitly choose leading/trailing affinity for the moving
focus, allowing backward movement and Shift-selection contraction across runs.
Input capture synchronizes pending native selection before applying an edit;
it does not depend on asynchronous `selectionchange` arriving first. Distinct
spellcheck target ranges retain outward normalization. A scalar-invalid UTF-16
surrogate offset is rejected. Post-edit caret
placement uses trailing Rust grapheme affinity because new adjacency can merge
clusters. This is not a general position-rebasing or visual bidi engine.

Enter and plain-text multiline paste use a targeted paragraph-break operation
through the existing transaction boundary. The retained prefix keeps its ID;
the suffix gets a new unique ID and inherits the start paragraph's style and
paragraph properties. Original prefix/suffix runs are cut independently before
new text is inserted, so a combining suffix cannot change the intended cut.
Browser paste normalizes CRLF and CR into paragraph breaks; the low-level core
text operations continue preserving literal CR/LF. One paste or completed
composition is one transaction. Cross-section structural replacement remains
explicitly unsupported. Opening a valid document with no paragraphs preserves
that structure; the first deliberate edit can materialize an initial paragraph.

Copy/cut obtain supported selected text from the Rust adapter's read-only
`selected_text` API, which uses core Unicode coordinates and one LF per logical
paragraph boundary. Native DOM `Selection.toString()` can add visual blank lines
between paragraphs and is not the document clipboard format. Cut publishes a
deletion only after clipboard writing succeeds. Clipboard input/output remains
plain text; source formatting and soft-versus-structural line-break distinctions
are not preserved by plain-text paste. Native Writer persistence remains exact.

### Presentation and supported controls

The Rust adapter overlays ancestor paragraph styles, the paragraph's direct
properties, and run properties to produce effective display values. Missing
values receive prototype defaults such as Arial 12 pt. Source optional values
remain unchanged, so explicit false, unspecified values, styles, and native
round trips retain their existing meaning. This policy is not complete Word
style/toggle/theme/default evaluation. Fixed and minimum line spacing share a
limited CSS presentation. Native negative indentation is preserved, but its
browser projection does not implement full outdent/layout behavior. Host font
fallback and metrics vary.

The prototype exposes bold, italic, underline, font family, font size, paragraph
alignment, title changes, word count, undo/redo, and document actions. Formatting
controls represent mixed selections. Collapsed character controls update Rust
typing properties for later insertion; they do not add an empty source run.
The interface uses semantic labeled controls, visible focus, dialogs, and status
messages. These choices support keyboard use without claiming accessibility
certification or completed assistive-technology testing.

### Explicit local storage and native files

IndexedDB records store exact core-serialized `.writer` bytes with document ID,
title, save etag, and timestamp metadata. No JSON reinterpretation in JavaScript
becomes the native format. Saving compares the expected etag and writes the next
record inside one read/write transaction; success is transaction completion.
A new record expects no existing etag. A stale tab or same-ID imported document
cannot silently overwrite another saved version; the user can reopen it or save
a separate copy with a new document identity.

Local Save captures document bytes and its Rust session/revision before awaiting
storage. Dirty state clears only if that revision is still current when saving
finishes. New, Open, Import, and Close guard unsaved replacement. Async open/import
also checks that the live session did not change during the read. Rust fully
validates candidate bytes before replacing the active session; opening a local
record additionally verifies its document ID. Corrupt input, storage failures,
and conflicts preserve live unsaved work. Export downloads exact native bytes
without replacing local save state. History remains session-local and is not
serialized.

IndexedDB availability, quota, eviction, blocked upgrades, closure, corrupt
records, and transaction failures are actionable application errors. A local
transaction's completion does not guarantee backup, power-loss durability, or
continued retention by the browser. Native filesystem synchronization/rename
guarantees do not apply to IndexedDB. There is no autosave, cloud storage, or
synchronization service.

Library listing returns readable summaries and explicit per-record problems;
one damaged record cannot hide healthy documents. Loading and overwriting remain
strict, and listing never repairs or deletes bytes. Save-and-continue errors are
announced inside the active modal rather than behind its inert backdrop.

### Resource bounds, verification, and delivery

The browser adapter limits accepted and produced native files to 5 MiB, total
paragraph text to 200,000 UTF-16 code units, and documents to 2,000 paragraphs,
2,000 sections, and 20,000 text runs. At most 128 named styles with inheritance
depth at most 32 bound worst-case presentation and selection work. Its session
retains at most 20 complete history snapshots, compared with the core's default
100. These application bounds do not change model or native-format
validity. Complete document copies, validation, JSON presentation snapshots,
serialization, and history remain known costs. The history bound counts entries,
not bytes; performance changes need evidence and a separate architectural review.

Stable Rust builds `office-wasm` for `wasm32-unknown-unknown`; the generated web
bindings require `wasm-bindgen-cli` 0.2.130 to match the locked crate. The frontend
production build performs the WASM build before TypeScript checking and Vite
bundling. `dist` is ready for preview/build inspection, not an authorization to
publish. Required validation includes the engine's four checks, TypeScript,
lint, unit tests, production build, and real-browser integration with production
WASM asset loading. The milestone report records actual results, screenshots,
and measured limitations separately from this design.
Eight direct Node/V8 ABI tests execute generated web bindings against actual
release WASM in addition to native Rust tests and production Chromium UI tests.
The [pre-merge review](PRE_MERGE_REVIEW.md) gives the current gate and distinguishes
clean process-restart evidence from untested crash/power-loss guarantees.

Synthetic composition and Chromium integration cannot certify every OS IME,
candidate window, dictation path, mobile browser, or assistive technology. Manual
OS IME and accessibility review remain necessary. Browser CSS presentation is
not professional page layout, pagination, or Word rendering compatibility.

`cloudflare.config.ts.example` is inert preparation for a separate assets-only
Writer Worker. It supplies no route or domain, disables workers.dev and preview
URLs, and changes no live deployment or access setting. The future address
`writer.proairetos.com` is not an implemented deployment. Turning this example
into active configuration, installing a deployment toolchain, changing DNS or
development-access restrictions, and releasing a site require explicit approval.
Step 5 also requires a separate researched DOCX scope and approval; completing
this prototype does not start it.

## Incremental workspace

`office-core` exposes the independent engine version, document model,
position/selection APIs, editing operations, and transaction
sessions with history. Serde, serde_json, and unicode-segmentation are declared in
`[workspace.dependencies]` and inherited by the crate for serialization and Unicode
segmentation. Serde JSON's `raw_value` feature reuses the model loader inside the
native envelope. `tempfile` is a native-only workspace dependency for safe saving;
it is excluded from `wasm32` builds along with the filesystem API. `office-wasm`
is the browser binding crate; `apps/writer` is its separate frontend project with
a committed npm lockfile. Generated WASM bindings and frontend output are build
artifacts. Additional dependencies are reviewed against the engineering rules;
their licenses do not decide the project's pending software license.
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
