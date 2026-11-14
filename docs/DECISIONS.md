# Decisions

These decisions establish the initial direction. They may be revisited when
implementation evidence justifies a change; changes should record the reason
and consequences here.

## 001: Use Rust and Cargo for the engine

**Decision:** Use stable Rust with edition 2024 and Cargo workspace management.

**Reason:** An Office engine needs predictable performance, memory safety, and
maintainable code across a long development life. Rust's ownership model and type
system help enforce invariants without requiring a garbage collector. Cargo
provides a standard build, dependency, and test workflow. Rust also supports
native compilation and WebAssembly, which aligns with browser and desktop goals.

**Tradeoffs:** Ownership and API design require care, and font, layout, and file
format behavior still need substantial engineering and testing. Rust does not
by itself provide Word compatibility or identical layout across platforms.
The stable toolchain tracks stable releases; committing `Cargo.lock` preserves
dependency resolution without freezing the compiler.

## 002: Keep the engine separate from the user interface

**Decision:** Keep core code independent of React, JavaScript, and browser APIs.
Future browser integration will use WebAssembly; React and TypeScript will be
interface technologies only.

**Reason:** Document correctness and compatibility must not depend on a particular
interface or runtime. This separation allows engine tests without a browser and
supports native desktop applications without rewriting the engine.

**Consequences:** Bindings and platform integration must have explicit boundaries.
The document model, editing operations, layout, and rendering also need distinct
responsibilities. No interface or binding layer is implemented in Step 1.

## 003: Begin with Writer

**Decision:** Start with word processing and Microsoft Word DOCX compatibility.
Spreadsheets and presentations follow later.

**Reason:** Writer provides a focused first application with concrete document
and compatibility requirements. Attempting all Office applications at once would
spread implementation and validation across different models before any one
application has a sound foundation.

**Consequences:** The initial design will be informed by Writer. Shared abstractions
will be introduced when actual cross-application requirements justify them.

## 004: Introduce structure and dependencies only when needed

**Decision:** Start with one dependency-free `office-core` crate. Do not use an
existing complete Office or word-processing engine.

**Reason:** A small foundation is easier to understand and validate. Concrete
responsibilities should determine crate boundaries and dependency choices.
Established low-level libraries remain an option as requirements arise.

**Consequences:** Future dependencies will use Cargo workspace dependency
management and require evaluation for correctness, performance, portability,
maintenance, and licensing.

Step 2 adds Serde and serde_json for versioned model serialization, inherited
through workspace dependencies. The workspace still contains only `office-core`.

## 005: Leave licensing pending

**Status:** Pending project-owner decision.

No software license has been selected. The project owner will decide whether
the engine is open-source, source-available, or proprietary. The `LICENSE` file
and package license declarations remain absent until that decision is made.

## 006: Use a typed, ordered Writer model with stable caller-assigned IDs

**Decision:** Represent documents, sections, paragraphs, and runs explicitly.
Use a typed `Block` enum, starting with paragraphs. Assign opaque IDs to the
document, sections, paragraphs, and named styles; enforce uniqueness across
these entity kinds within each document.

**Reason:** Types and order make content inspectable and invariants testable.
Stable identity supports future editing transactions without tying identity to
array positions, rendered pages, or an imported format's identifier scheme.
Caller assignment avoids a random-number or platform dependency at this stage.

**Consequences:** Applications must allocate and retain IDs. Runs have no stable
ID yet; future editing anchors need a separate design. Empty documents,
sections, paragraphs, and runs are valid. No default content is synthesized.
Construction validates complete documents and exposes immutable inspection;
future transactions should validate candidates before committing atomically.

## 007: Preserve unspecified properties and use explicit integer units

**Decision:** Store optional character and paragraph property values rather
than resolving them immediately. Use integer twips, half-points, and
1/240-line spacing units. Named paragraph styles can reference a parent, with
missing references and cycles rejected.

**Reason:** Unspecified properties differ from explicit overrides such as
`bold: false`. This distinction is necessary for inheritance and future OOXML
mapping. Integer units avoid nonfinite values and floating-point round-trip
ambiguity. Separate paragraph/run properties resemble relevant OOXML concerns
without adopting its full representation.

**Consequences:** Validation rejects nonpositive font sizes and line heights,
negative paragraph spacing, and blank font-family names. Signed indentation
supports hanging and outdented content. Style resolution, theme behavior,
character styles, Word toggle semantics, and layout defaults remain future work.

## 008: Version JSON and reject unsupported information explicitly

**Decision:** Use Serde and serde_json for schema-versioned JSON persistence.
Schema 1 is independent of the engine version. `Document::from_json` preflights
the version and returns structured schema, JSON, or validation errors. Unknown
fields and enum kinds are rejected rather than silently ignored.

**Reason:** A stable model needs a round-trip contract and an explicit boundary
against future or unsupported structures. Serde provides established low-level
serialization support without introducing an Office engine.

**Consequences:** Step 2 includes basic serialization ahead of the original
milestone sequence. There is no migration, automatic repair, or opaque imported
data preservation. Unknown OOXML elements will require a deliberate policy
before faithful DOCX import/export is possible. JSON syntax details are not
preserved; supported model values and array order are preserved.

## 009: Keep Unicode storage and position coordinates separate

**Decision:** Store exact UTF-8 strings without normalization. Defer cursor,
selection, and offset-conversion APIs until editing requirements are defined.

**Reason:** UTF-8 bytes, Unicode scalars, extended grapheme clusters, and UTF-16
code units are not interchangeable. Combining marks and multi-code-point emoji
make scalar or byte counts unsuitable for user-visible cursor movement.

**Consequences:** Future editing must account for grapheme boundaries, including
clusters across run boundaries, and explicit conversion to external UTF-16
offsets. No grapheme segmentation library or mapping implementation is introduced
until needed. Unicode fixture tests check exact preservation rather than
claiming editing correctness.

The deferral in this decision described Step 2. Milestone 3A now implements the
logical boundary and conversion APIs under decisions 010 and 011; text storage
and normalization behavior remain unchanged.

## 010: Use paragraph-wide grapheme positions and explicit coordinate units

**Decision:** Introduce `Utf8Offset`, `Utf16Offset`, and `GraphemeOffset` as
distinct types. Build an owned `TextCoordinates` snapshot from a paragraph's
concatenated runs and segment extended grapheme clusters with the low-level
unicode-segmentation library. The lockfile currently resolves version 1.13.3,
using Unicode 17.0.0 data.

**Reason:** Formatting-run boundaries are not Unicode character boundaries.
Combining marks, emoji ZWJ sequences, and other graphemes may span multiple runs.
Explicit units prevent byte offsets from being mistaken for code units or cursor
positions. Established Unicode segmentation avoids maintaining a partial custom
implementation.

**Consequences:** Raw UTF-8/UTF-16 conversion accepts scalar boundaries, including
those inside a grapheme. Cursor conversion adds an extended grapheme check.
Invalid byte boundaries, surrogate interiors, grapheme interiors, and out-of-range
values return structured errors without snapping or normalization. The complete
official Unicode grapheme-break fixture verifies the matching Unicode version.
Dependency upgrades must review boundary changes and update matching fixtures.
Snapshots use linear text construction, a grapheme-boundary vector, and simple
lookups rather than a premature document-wide index or cache.

## 011: Anchor runtime positions to stable IDs and keep selections logical

**Decision:** A `DocumentPosition` holds a stable document ID, stable paragraph
ID, and checked grapheme offset. Directed selections retain anchor and focus;
their ordered half-open ranges follow logical document order. These types are
runtime values and do not alter document schema 1.

**Reason:** Document order and formatting-run organization can differ from stable
identity. Paragraph-relative positions avoid coupling the engine to frontend
indices, visual layout, or DOCX coordinates. Logical selection behavior is useful
before a renderer or editing system exists.

**Consequences:** Empty paragraphs have one boundary; empty sections/documents
have no paragraph positions. Adjacent paragraphs' end/start positions stay
distinct without invented separator offsets. There is no selected-text extraction
or visual bidi navigation. Positions are not revision-aware and do not rebase;
revalidation detects missing IDs and bounds failures, not semantic changes under
the same IDs. Future transactions must define stale-position and affinity policy.
No position or selection serialization is introduced.

## 012: Gate transaction and editing work behind the position milestone

**Decision:** Split Step 3 into 3A (positions/selections), 3B (atomic validated
editing transactions), and 3C (text editing, formatting operations, and undo/redo).
Implement 3A first and require its formatting, Clippy, test, and build checks to
pass before beginning 3B or 3C.

**Reason:** Unicode boundary correctness must be established before operations
can mutate text, and atomic validation must precede operations with history.

**Consequences:** 3A changes neither the document model nor editing state. It
implements no transactions, insertion/deletion, formatting operations, or history.
3B and 3C remain separate future milestones.

Milestone 3A subsequently passed its checks and was merged into `main`. Milestone
3B implements the transaction foundation under decisions 013 and 014. Milestone
3C remains unstarted.

## 013: Stage detached candidates and validate before atomic publication

**Decision:** Introduce `DocumentSession` as the runtime owner of a validated
committed document. A detached `Transaction` clones metadata, sections, and styles
into a mutable `DocumentDraft`, retaining the document ID. Commit consumes the
transaction, validates the final candidate through `Document::from_parts`, and
publishes only after every fallible check succeeds.

**Reason:** Compound changes need one validation and visibility boundary. Invalid
intermediate construction records must not leak into committed state. Reusing
the existing validator keeps model invariants centralized and preserves document
schema 1 without exposing mutable committed access.

**Consequences:** Failed commits, dropped transactions, and caller-side staging
panics leave committed content and revision unchanged. No callbacks execute
during publication. Atomicity uses an exclusive Rust borrow and is in memory;
it provides no durable persistence or collaboration protocol. Beginning a draft
copies document content, and commit performs full validation and structural
comparison. These initial linear costs are explicit; structural sharing or
incremental validation should follow measured requirements. There are no text
operations, formatting operations, undo/redo, events, or new dependencies in 3B.

## 014: Scope revisions to live sessions and reject stale origins

**Decision:** A `Revision` token privately combines an opaque `Arc` session
identity with a checked `u64` counter. Commit rejects foreign sessions and stale
tokens before candidate validation. Changed commits advance the counter once;
structurally identical candidates are no-ops that retain the current revision.

**Reason:** Document ID and counter alone cannot distinguish independent editing
sessions or reopened documents. Explicit origin checking prevents stale drafts
from overwriting later commits. No-ops should not invalidate positions or other
pending drafts when no model value changed.

**Consequences:** Revision numbers are session-local diagnostics, not global
identities. Sessions and transactions cannot be cloned, and none of their runtime
state is serialized into the document. Counter overflow rejects changed commits
without wrapping. A stale draft is rejected even if its candidate matches current
content, and an A-to-B-to-A sequence does not revive old tokens. Conflicts require
a fresh transaction; automatic merging and retries are absent.

Changed commits conservatively invalidate all earlier tokens, including origins
for unchanged paragraphs; no-op commits preserve them. Raw 3A positions and
selections still contain no revision. Callers capture the originating token with
those values, validate it before reuse, then apply existing structural checks.
Pairing an old value with a fresh token does not establish correct provenance.
Revision-aware position wrappers, rebasing, and editing affinity remain later
decisions before position-consuming text operations are introduced.
