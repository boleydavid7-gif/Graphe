# Decisions

These decisions establish the initial direction. They may be revisited when
implementation evidence justifies a change; changes should record the reason
and consequences here.

Entries retain their original milestone context. Decisions 031–033 supersede
the earlier schema-1/native-format-1-only and conversion-pending statements;
034–036 resume Step 6 after that prerequisite. Historical reasons remain recorded.

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
3B also passed its checks and was merged. Milestone 3C extends that foundation
under decisions 015 through 017.

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

Milestone 3C adds transaction-local editing wrappers under decision 015. Raw 3A
inspection positions retain the origin-token contract above; automatic rebasing
and typing affinity remain unimplemented.

## 015: Bind editing endpoints to a transaction's current draft

**Decision:** Introduce `EditPosition` and `EditSelection`, minted only through
the transaction from checked grapheme positions. A private draft identity stamp
is rotated after a changed operation and every mutable draft borrow. Editing
operators reject stale or foreign stamps before processing candidate content.

**Reason:** Stable element IDs and an originating committed revision do not
identify an intermediate draft state. A second operation must not accidentally
interpret offsets from before an earlier staged change. Transaction-local
provenance provides this boundary without changing the model or inventing a
document-wide position-rebasing system.

**Consequences:** Callers reacquire endpoints and coordinates after changes.
Failed and no-op operations preserve endpoints. Raw inspection positions cannot
be supplied to editing operators. Every operation validates a complete candidate
before updating the draft, while final commit still checks the session revision.
Operations return a changed flag rather than an implicit cursor because inserted
or rejoined text can form new grapheme clusters. No automatic rebasing, result
caret affinity, frontend integration, or position serialization is introduced.

## 016: Make text and formatting behavior explicit within the current model

**Decision:** Support insertion, deletion, and replacement across paragraphs in
one section. A join retains the start paragraph's identity, style, and properties
and appends the retained end suffix. Reject cross-section structural replacement.
Keep CR and LF as literal UTF-8 text. Require explicit character properties for
inserted runs. Use `Keep`, `Set`, and `Clear` property patches for formatting.

**Reason:** The existing model supplies paragraphs and sections but has no page
or section-joining policy. Explicit limits avoid inventing layout semantics or
implicit paragraph IDs. Three-state property changes preserve the distinction
between inherited values and direct overrides.

**Consequences:** Character and paragraph formatting may span sections without
changing structure. A collapsed character range does not create typing state;
paragraph formatting targets the containing paragraph. Half-open paragraph
ranges exclude a final paragraph at offset zero. Named-style assignment retains
direct formatting and validates references without resolving styles. Unaffected
run boundaries and properties remain intact; no-op patches and identical
replacement content preserve the original partition. No new schema or dependency
is required. Explicit paragraph splitting, cross-section joins, style resolution,
and visual editing behavior remain later work.

## 017: Keep bounded exact document snapshots for undo and redo

**Decision:** Record one complete prior document per changed commit in the
session's undo history, with a default limit of 100 snapshots and a configurable
count including zero. Undo and redo restore exact values under fresh increasing
session revisions. New changed commits clear redo; failures and no-ops preserve
both stacks.

**Reason:** Snapshot history makes transaction grouping and exact restoration
straightforward while the engine is small. It avoids introducing inverse-command
semantics before the operation set has stabilized. Fresh revisions preserve the
stale-origin guarantees even when content returns to an earlier value.

**Consequences:** Multiple staged operations form one undo step. IDs, formatting,
Unicode text, and run boundaries restore exactly. History is bounded by count,
not bytes, and full document snapshots can consume substantial memory.
Validation and revision-overflow checks precede history publication. Empty stacks
return structured errors; clearing history leaves document and revision intact.
History is session-local, is not serialized, and performs no typing coalescing or
grouping beyond a transaction. Structural sharing or more compact history should
follow measured requirements rather than precede them.

## 018: Wrap the existing document schema in a provisional native file format

**Decision:** Use `.writer` as the provisional internal extension for a UTF-8
JSON envelope containing `format: "office-writer"`, native `format_version: 1`,
and the existing schema-1 document. Keep native format version, model schema,
and engine application version separate. The specification may evolve before
public release.

**Reason:** A native file needs an explicit identity and compatibility boundary
without replacing the already validated document representation. A separate
envelope version permits future packaging decisions without conflating them with
the model or requiring ZIP, compression, or a custom binary format now.

**Consequences:** Serialization validates first and emits deterministic pretty
JSON with a trailing LF. Deserialization checks the envelope version, retains the
document as borrowed Serde JSON `RawValue`, and delegates to `Document::from_json`.
Enabling `raw_value` on the existing dependency preserves duplicate-key detection
and structured model errors without a competing decoder. Unknown fields, corrupt
data, and unsupported native/model versions are rejected. Bare model JSON is not
an implicit native file. Future migrations must be explicit, fixture-backed
source/target transformations validated under the resulting schema; no migration
or silent reinterpretation is introduced before a second version is needed.

## 019: Separate byte serialization from native filesystem storage

**Decision:** Expose `serialize_document` and `deserialize_document` independently
of storage. Add `save_document` and `load_document` only on targets other than
`wasm32`, using native paths. Keep the existing single core crate and unchanged
document model.

**Reason:** Browser storage and native files should share document bytes and
validation while retaining distinct I/O mechanisms. The core model must not gain
browser dependencies or require an application shell to save supported values.

**Consequences:** Saving borrows a document without changing its contents,
revision, pending drafts, or history. Loading returns a validated document;
reopening it in a session starts a new lineage with empty history. Revisions,
undo/redo, and editing endpoints are runtime state and are not persisted. File
processing currently holds the complete document and bytes in memory. Native
paths support Unicode without converting `Path` values to strings. No parent
creation or extension enforcement is implicit. Future browser storage and DOCX
adapters remain separate work; Step 4 implements neither and does not begin 4.5.

## 020: Prepare a synchronized temporary file before native replacement

**Decision:** Validate and serialize completely, then create a unique temporary
file in the destination directory, write all bytes, flush, apply retained ordinary
permissions, synchronize, and persist it over the destination without deleting
the old file first. Use the low-level native-only `tempfile` dependency for
exclusive unique creation,
portable persistence, and RAII cleanup. Explicitly close failed temporary files
and retain cleanup errors with the original failure. On Unix, pre-open and later
synchronize the parent directory.

**Reason:** Directly truncating an existing document risks losing valid data
when writing fails. Established low-level temporary-file handling avoids a
home-grown name-allocation and replacement protocol. Preparation before
replacement isolates ordinary pre-replacement failures from committed data.

**Consequences:** Storage errors retain operation, native path, original I/O
source, and replacement state. `NotAttempted` leaves the old destination intact;
`Uncertain` records a failed replacement call whose outcome may be ambiguous on
some filesystems; `Completed` reports that replacement succeeded before a later
directory-sync error. A failed explicit cleanup retains its error and path; the
temporary file may remain without an automatic retry. RAII cleanup applies to
ordinary drop and unwinding paths and remains best-effort. Cleanup cannot be
guaranteed after a process kill or filesystem/permission changes. Windows has no
equivalent parent-directory synchronization step here. Atomic replacement and crash durability
depend on operating system, filesystem, and hardware, so no universal guarantee
is made.

Saving rejects symlink, non-regular, and read-only destinations. Ordinary file
permissions are copied after writing and flushing, before synchronization;
ownership, ACLs, extended attributes, timestamps, and hard-link identity are not
preserved. Stable trusted parent paths are assumed,
and concurrent saves are last-writer-wins without locking or conflict detection.
Tests use temporary directories and deterministic private I/O failure injection
to verify preservation, cleanup, and post-replacement error behavior without
depending on privilege-sensitive permission failures.

## 021: Add an application binding boundary without moving engine authority

**Decision:** Step 4.5 adds `apps/writer` with React, TypeScript, Vite, and CSS,
plus a small Rust `wasm-bindgen` binding crate. `office-core` remains independent
of JavaScript, React, browser APIs, and filesystem storage adapters on WASM.
Commands reuse Rust sessions, operations, validation, history, and native bytes;
JSON snapshots initially transport the supported model and presentation data.

**Reason:** The user has authorized the first browser application. A concrete
binding crate isolates runtime concerns without duplicating the engine or
introducing a complete external editor. See `RESEARCH.md` for alternatives and
primary sources evaluated before implementation.

**Consequences:** TypeScript owns UI/input/storage state, not a competing
document model or undo stack. Boundaries return structured errors. Revisions
cross the JavaScript boundary as decimal strings because a Rust `u64` may exceed
JavaScript's exact numeric range. Imported IDs are opaque map keys, never CSS
selector fragments. The browser supplies a cryptographic UUID namespace; Rust
uses a checked counter and document validation for newly allocated IDs. Match
the `wasm-bindgen` CLI to the locked crate version. No WASM wrapper/storage
library is necessary merely to pass JSON snapshots or access IndexedDB.

## 022: Use semantic browser input with Rust-owned committed edits

**Decision:** Render the document into a labeled semantic `contenteditable`
surface. Intercept supported cancellable `beforeinput` commands and send Rust
transactions. Handle `input` as a recovery path for uncancellable or unexpected
browser changes. Let native pointer/keyboard navigation inform directed Rust
selections. Browser history commands call Rust undo/redo.

**Reason:** Native DOM editing offers established selection, bidi navigation,
IME, and accessibility behavior. Canvas with a hidden textarea would require
substantially more infrastructure. EditContext's limited availability is not a
suitable initial cross-browser foundation; `execCommand` is deprecated and
would introduce a competing browser history.

**Consequences:** DOM content is presentation, with a deliberate temporary
exception for an active composition buffer. Suspend reconciliation of that
buffer, retain its Rust origin, and commit the completed replacement once.
Composition cancellation and final event ordering require regression tests.
Plain-text paste is one Rust transaction; line breaks use explicit paragraph
splitting rather than teaching JavaScript to edit the document. Add targeted
transaction operations instead of rewriting the model. Existing raw text
operations still preserve literal CR/LF. Valid empty documents acquire initial
paragraph structure only through a deliberate edit. Unsupported cross-section
replacement produces an explicit error. Keyboard toolbar focus and native
navigation must not be overwritten by unconditional selection restoration.

## 023: Keep Unicode and effective presentation policy in Rust

**Decision:** Keep the engine's pinned Unicode 17 grapheme boundaries and strict
UTF-8/UTF-16 conversions authoritative. Map DOM UTF-16 text offsets and element
child offsets through stable paragraph IDs. Define post-edit caret affinity and
the prototype's inherited/direct effective formatting in Rust.

**Reason:** Formatting-run boundaries are not character boundaries. Inserting or
deleting text can merge adjacent graphemes, so adding JavaScript string length
to an old position is incorrect. Rendering inherited styles in JavaScript would
duplicate policy and could diverge from the engine.

**Consequences:** Keep exact UTF-8 and source formatting values unchanged. A
browser endpoint inside an extended grapheme requires an explicit adapter
affinity policy, not weaker core validation. Preserve anchor/focus direction;
restore selection only when necessary. Display inheritance is a documented
prototype policy, not full OOXML toggle/theme/default evaluation or layout.
Unicode 18, mapped positions, custom visual bidi movement, and advanced Word
style semantics require separate decisions. Add tests for combining marks and
emoji spanning run boundaries, edits, and DOM offset conversions.

## 024: Persist native bytes locally with explicit save conflict checks

**Decision:** Use IndexedDB to save the same validated `.writer` bytes as native
persistence. Keep opening, saving, importing, and exporting explicit. Compare a
record's expected save token and write its replacement within one read/write
transaction; success means transaction completion, not merely request success.

**Reason:** IndexedDB stores complete documents asynchronously without
localStorage's synchronous complete-document storage costs. Transactional
comparison prevents a stale tab from silently overwriting a newer local save.
Reusing native bytes preserves one format and validation path.

**Consequences:** Storage or import failures leave the live Rust document and
unsaved work intact. A completed asynchronous save clears dirty state only if
the in-memory revision still matches the saved snapshot. Opening/replacing work
must handle unsaved changes deliberately. Browser quota, eviction, private-mode,
and transaction failures are user-visible limitations; local save is not a
universal crash-durability or backup guarantee. History remains session-local
and is not serialized. No autosave, cloud storage, or synchronization is added.

## 025: Record research and prepare deployment without changing live services

**Decision:** Apply `AGENTS.md` engineering rules to Step 4.5 and future work.
Record primary-source research, alternatives, classifications, and regression
requirements in `RESEARCH.md` before adding subsystems. Prepare an inert
`cloudflare.config.ts.example` for a separate assets-only Writer Worker using
the current `cf/config` shape; disable workers.dev and preview URLs and attach no
domain. `writer.proairetos.com` remains the intended future address.

**Reason:** Research and retrospective review reduce compatibility and
maintenance risks without requiring speculative refactors. Deployment readiness
does not authorize changes to DNS, Cloudflare, access restrictions, or other
applications.

**Consequences:** No deployment or CLI installation is performed merely to add
the example. Real deployment configuration and release require approval. No
existing critical architectural defect was identified in the pre-implementation
review. New integration risks need regression tests. Snapshot/history redesign,
incremental validation, full Word style semantics, and unknown OOXML preservation
remain important future decisions requiring approval before major changes.
Dependency licenses are reviewed separately from the pending project license.
Synthetic IME and automated accessibility tests have documented limits.

## 026: Make browser navigation affinity and prototype limits explicit

**Decision:** Keep strict core conversions unchanged. For browser selection
updates, an explicit leading/trailing affinity snaps the moving focus in its
logical direction; collapsed endpoints follow the same affinity. Without an
explicit navigation affinity, ranges expand outward and collapsed endpoints use
trailing affinity. Invalid surrogate boundaries remain errors. Post-edit cursor
placement uses trailing affinity in Rust.

**Reason:** Real Chromium tests found that native movement can stop inside a
grapheme spanning formatting nodes. Always-forward snapping traps ArrowLeft;
always-outward range snapping traps reverse selection contraction. Directional
browser requests solve these cases without a second Unicode implementation.

**Consequences:** Browser limits are 5 MiB native bytes, 200,000 UTF-16 units,
2,000 paragraphs/sections, 20,000 runs, 128 styles, style depth 32, and 20 undo
snapshots. Check cheap projected text/paragraph counts before staging multiline
paste; validate complete candidates before publication and export. These are
prototype resource policies, not schema changes. Reject edits without a usable
selection when content exists; never replace existing content to create a cursor.
Forced DOM recovery reconstructs canonical presentation even for empty native
documents. Regressions cover the critical integration defects listed in
`RESEARCH.md`; substantial history/layout/style redesign remains deferred.

## 027: Read clipboard structure from Rust and isolate recoverable local failures

**Decision:** Copy/cut read the normalized selection through the Rust adapter,
joining paragraph boundaries with one LF. Core Unicode coordinates determine
the UTF-8 slices. Copying is read-only; cut deletes only after successful
clipboard writing. Native format/schema and editing transaction APIs are unchanged.

**Reason:** Actual browser testing found that DOM selection text adds visual
blank lines, creating unintended paragraphs when pasted through the explicit
plaintext policy. DOM presentation cannot define structural clipboard text.

**Consequences:** Plain-text clipboard does not preserve source styling or
distinguish literal soft line breaks from paragraph separators. Native `.writer`
files still preserve the complete supported model. Additional Node/V8 tests run
the actual compiled WASM ABI alongside native and production-browser suites.
Library listing reports damaged records individually without repairing/deleting
them; healthy records remain openable. Modal save errors appear inside the
active dialog. These are targeted pre-merge corrections, not a major refactor.
The exhaustive gate and manual testing limits are in `PRE_MERGE_REVIEW.md`.

## 028: Provision hosted build tools explicitly without changing local builds

**Decision:** Add `npm run build:cloudflare` as a build-only entry point. On the
Cloudflare Linux x86_64 image it installs stable Rust and the WASM target when
needed, obtains the pinned wasm-bindgen 0.2.130 prebuilt CLI with SHA-256
verification, then runs the existing build with the prepared environment.
Preserve caller Cargo/Rustup homes, isolate defaults in ignored cache paths,
and leave HOME and shell startup files unchanged.

**Reason:** The hosted build supplied by the user fails because the binding
generator is absent. Tool installation cannot be assumed from Node availability
or undocumented cache persistence, and an npm prebuild child cannot export PATH
to a subsequent command.

**Consequences:** No new runtime dependency, engine change, active Worker config,
or deployment command is introduced. Tests verify corrupt downloads are rejected
before execution and that the build inherits the toolchain environment; an
actual clean-toolchain production build verifies the complete path. Private
access must be established before publishing through the connected Git pipeline.
Remote permission failures and Access state are documented in
`CLOUDFLARE_BUILD.md`; they are not treated as successful deployment verification.

## 029: Keep file formats outside the shared semantic model

**Decision:** Step 5 introduces a format-specific `office-docx` adapter around
the existing Rust model. OPC/ZIP, WordprocessingML, DOCX relationships and
compatibility/source-preservation state belong to that adapter. `office-core`
must not gain DOCX-specific records, assumptions, parser dependencies or
format-specific import/export methods.

**Reason:** Writer must support additional formats over time, including ODT,
RTF, TXT, Markdown and HTML. They should use the shared document semantics
directly rather than conform to a DOCX-shaped model or DOCX intermediate.

**Consequences:** Core/native schema and format versions remain unchanged.
Step 5A provides bounded read-only package inspection and immutable `DocxSource`
retention. The user selected **preserve unsupported originals before editing**;
editable projection and export must wait for a reviewed preservation/edit
mapping that prevents silent loss. Original-byte retention alone is not a
post-edit fidelity guarantee. Each future adapter applies this policy where
appropriate using its own representation, not necessarily ZIP/XML. Add shared
semantic structures only for understood format-independent behavior. No other
format, generic registry, speculative plugin hierarchy, or core extension bag
is implemented in Step 5.

## 030: Gate semantic DOCX conversion behind bounded source inspection

**Decision:** Begin with a byte-oriented, read-only OPC/ZIP and UTF-8 XML 1.0
profile. Use maintained low-level ZIP/XML libraries, bounded structural
preflights and exact owned source bytes. Separate structured errors from
third-party parser types. Reject unsupported encodings/container forms and
resource excess explicitly rather than repairing or truncating input.

**Reason:** Package safety and preservation must be established before editable
import. The core cannot yet represent many ordinary Word constructs. ZIP index
allocation and XML attributes/namespaces need bounds before library processing;
parser defaults alone do not enforce the required profile.

**Consequences:** The initial profile is intentionally narrower than all valid
DOCX packages. Inspection is neither schema certification nor semantic import.
No editable projection, DOCX reconstruction, native migration or UI integration
is included in 5A. `DocxSource` preserves the exact original, but does not promise
post-edit round trips. Limits, dependency rationale and regression corrections
are specified in [DOCX.md](DOCX.md); extend the profile only with compatibility
evidence and tests.

## 031: Add generic page properties with strict schema migration

**Decision:** Schema 2 adds format-neutral page size/orientation, margins and
paragraph page-break-before. Preserve explicit dimensions without implicit
rotation. Require positive dimensions/body area and nonnegative page margins;
retain existing signed paragraph indentation. Decode schema 1 through its exact
historical property shapes, then migrate new values to unspecified.

**Reason:** DOCX conversion needs real shared page semantics before typography
can consume imported properties. New properties must not silently acquire
meaning inside legacy payloads or depend on XML/Word structures.

**Consequences:** Legacy IDs, text, optional-value distinctions and order survive.
Unknown newer fields and duplicate keys remain errors, including direct Serde
JSON loading. Future versions are rejected. Geometry storage does not implement
layout, headers/footers or professional pagination. Negative Word top/bottom
margin overlap remains outside the editable profile.

## 032: Retain originals in a versioned native file outside editing history

**Decision:** Native format 2/schema 2 writes a `WriterFile` with an optional
format-neutral `OriginalSource`. Its metadata and exact bytes are bounded; bytes
are immutable shared `Arc` data, encoded as strict padded base64 in JSON. Full-file
byte and filesystem APIs preserve both document and original. Format 1/schema 1
loads through the explicit legacy migration. Document-only loading rejects source
presence rather than dropping it.

**Reason:** Preserving originals before editing must survive save/reopen without
putting large source blobs into every document/undo snapshot or coupling the
model to DOCX. Existing document-only APIs remain safe for source-free files.

**Consequences:** Core file/source limits are 32/16 MiB; the browser keeps its
smaller complete 5 MiB budget. Source format validity belongs to the adapter,
not the generic record. Old version labels cannot carry newer shapes. Source
bytes can include text removed by later edits, so native export discloses this.
An exact original is a backup, not a post-edit fidelity guarantee.

## 033: Gate editable DOCX import on a complete profile and durable local save

**Decision:** `office-docx` returns either a complete supported editable model
plus original or a preserved read-only source with structured diagnostics. Map
supported styles/defaults, direct properties, sections and controls deliberately;
reject ambiguous text whitespace/control interpretations. DOCX export regenerates
current supported content, while original download returns unchanged bytes.
Browser import saves a detached candidate's complete native file through IndexedDB
before replacing the live editor session.

**Reason:** The owner chose original preservation before editing and paused
Step 6 to finish conversion. A partial projection or failed asynchronous save
must not replace live work or imply that unknown semantics survived editing.

**Consequences:** Unsupported sources remain recoverable in the read-only dialog
but are not silently saved in the local library. Storage failures, stale origins
and invalid imports preserve the prior session/history/dirty state. Native imports
still use explicit Save. DOCX IDs, exact run partitions and Word style-editing
intent are not universal round-trip guarantees; Microsoft Word interoperability
and pagination remain unverified. No other format or typography implementation
is added. The [profile](DOCX.md), [contract](DOCX_CONVERSION.md) and
[verification](DOCX_CONVERSION_VERIFICATION.md) define the practical limits.

## 034: Add a separate Rust typography and layout subsystem

**Decision:** `office-layout` consumes validated format-neutral documents and
produces immutable page, line, glyph and cursor geometry. Use pinned Parley,
HarfRust and Skrifa rather than implementing specialized shaping algorithms.
Core positions, editing transactions and validation remain authoritative.

**Reason:** Typography now has a necessary, distinct responsibility. It must run
on native and WebAssembly targets without depending on browser text metrics,
React or DOCX structures. Existing editor/layout APIs do not justify a rewrite.

**Consequences:** Derived geometry and caches stay outside documents, originals,
native serialization and undo history. Unicode/glyph/position units stay explicit.
The layout profile defines checked measurement limits, integer output rounding,
line/paragraph spacing and page-flow policy. Unsupported sizes fail without
clamping or changing valid model data. This is independent layout, not a claim
of Microsoft Word pagination compatibility.

## 035: Pin font bytes and make fallback observable

**Decision:** Register a versioned, licensed font catalog explicitly; disable
system/host font discovery. Use the same font files, variations and low-level
math features on native/WASM. Keep substitutions and missing coverage visible in
layout diagnostics while preserving requested model font families.

**Reason:** Device-installed fonts and silent fallback make document geometry
depend on the environment. A finite reviewed catalog provides reproducible
inputs without bundling proprietary fonts or claiming universal coverage.

**Consequences:** Catalog changes require provenance/hash, licensing and geometry
regression review. Ship complete OFL notices with browser assets embedding font
bytes; upstream font licenses do not choose this project's license. CJK, color
emoji and unsupported synthetic styles retain explicit limitations. Cross-target
agreement is tested with actual outputs, not inferred from successful compilation.

## 036: Verify Rust layout through a read-only outline preview first

**Decision:** Expose layout through a separate revision-bound WASM API. React
draws Rust-provided glyph outlines and page geometry without reshaping or
paginating text. Preserve the functioning semantic editor during Step 6.

**Reason:** Rendering fixed outlines exercises the native/browser architecture
without quietly introducing a second layout engine. A complete page-based
editing surface has distinct caret, bidi navigation, IME and accessibility work.

**Consequences:** Preview results are transient and stale revisions are rejected.
The current contenteditable/CSS view remains a prototype editing projection.
SVG raster edges can differ across browsers even when geometry matches. Step 7
may build editing on tested layout mappings; no such redesign, merge, deployment
or live access change is implied by this decision.

## 037: Use Rust geometry for paginated interaction and retain proven input handling

**Decision:** Extend the existing layout boundary with hit testing, caret and
selection geometry, visual navigation and explicit affinity. Render interactive
pages from Rust glyph geometry while retaining controlled semantic browser input
as the keyboard, clipboard and IME adapter. Committed text, selections, edits,
validation and history remain owned by Rust.

**Reason:** The read-only Step 6 preview established the shaping/rendering
boundary. Everyday page editing now requires position-aware interaction, but
does not require replacing the existing model or embedding another editor.
Reusing input recovery/composition handling keeps a tested path for browser
events while avoiding a competing CSS page-layout or JavaScript document engine.
EditContext remains unsuitable as a required cross-browser foundation.

**Consequences:** Page coordinates and core UTF-8/UTF-16/grapheme positions stay
distinct. Requests are revision/session-bound; navigation changes no document
content or undo history. Mandatory breaks, bidi affinity and vertical preferred-x
need regression coverage. The input projection remains focusable and semantic;
its native CSS navigation cannot replace Rust visual decisions. Composition is a
temporary buffer and commits once through the existing transactions. Clipboard
remains deliberate plain text, and cut follows successful clipboard writing.
Manual OS IME, mobile editing and assistive-technology limitations must remain
visible rather than being inferred away by passing Chromium tests.
Glyph memo comparisons may skip unchanged Rust output only when every rendered
path, transform, color, decoration and line property agrees; they do not derive
layout or change the document. The isolated 30-event before/after measurement in
RESEARCH shows improved tail time but similar medians, with approximately 0.1 s
full-layout latency on its 11.8k-character sample. Retain that important limit
for the planned audit instead of introducing an unmeasured layout architecture.

## 038: Serialize autosave and explicit Save through one local write lane

**Decision:** Add a small save controller around existing full native byte
serialization and `WriterStorage`. Debounced autosave and explicit Save use one
write lane and etag lineage. Each attempt captures immutable bytes and its
document/session/revision at execution time; document changes invalidate queued
status through an epoch. Pause on storage/conflict failures and during IME.

**Reason:** Independent background/manual writes could conflict with their own
previous write or incorrectly mark newer content saved. The existing IndexedDB
transaction/etag protocol already protects records and requires no new storage
format, database version, accounts or synchronization system.

**Consequences:** A completed earlier write advances only its still-current
binding's expected etag; dirty clears only for the exact saved revision. Old
callbacks cannot affect another session even when its document ID is reused.
Foreground autosave waits for 750 ms of inactivity, with a 5-second maximum
pending wait so continuous typing cannot postpone every save indefinitely;
composition suspends both timers. Browser timer throttling can extend those waits.
Failures retain editable in-memory content and an understandable persistent save
status, with retry/copy/export options. Untouched blank documents need not create
background library records. Visibility-hidden flushing and unload warnings are
best effort; neither promises completion during abrupt browser termination.
Autosave never labels an active, uncommitted composition buffer as saved.

## 039: Keep the Lamplight application small and backed by working commands

**Decision:** Use the approved Lamplight reference for compact warm brown and
charcoal chrome, restrained gold accents and white document pages. Present only
working editing/document controls, counts and zoom with simplified smaller-screen
adaptation. Preserve full native/DOCX original handling and existing compatibility
limits rather than filling the interface with speculative feature placeholders.
The approved "Paragraph style where available" selector applies already-present
named styles through the existing Rust operation; it does not add style creation,
editing, template management or a new style/inheritance model.

**Reason:** Step 7 is a dependable everyday Writer milestone, not an attempt to
recreate every Word feature or redesign the established engine. A restrained
interface leaves clear space for later capabilities without adding empty modules
or architectural layers now.

**Consequences:** Tables/images, headers/footers, advanced formatting/reviewing,
style creation/editing, templates, PDF, cloud integration and additional formats
remain future work. Responsive
layout does not imply polished mobile selection or full mobile IME support.
Actual application screenshots and executed tests establish the delivered state;
the reference alone does not. Development pauses after this milestone for the
owner's planned comprehensive technical and commercial licensing audit. Existing
font/dependency notices and the pending project-license decision are preserved.
Merging, deployment and live access changes still require separate approval.

## 040: Keep ordinary collapsed typing within its edited formatting run

**Decision:** When inserting nonempty text at a collapsed selection, extend only
the final nonempty retained left-prefix run whose direct `CharacterFormat`
exactly equals the supplied format. Otherwise create the inserted run as before.
Keep the complete right suffix, empty runs and pre-existing adjacent partitions;
do not add a document-wide run normalization pass. Noncollapsed replacement and
identical-content no-op rules remain unchanged.

**Reason:** A retrospective regression reproduced 1,000 separately committed
ordinary keystrokes creating 1,000 runs. The browser's 20,000-run safety budget
would therefore limit an ordinary unformatted writing session by keystroke count
and incur avoidable per-run layout work. Extending the edited prefix addresses
that foundation defect without rewriting the model or increasing safety limits.

**Consequences:** Direct-format equality includes optional-property presence;
`None` and `Some(false)` remain distinct even if currently rendered alike.
Comparison does not resolve styles or assume equivalent visual appearance.
Untouched imported run boundaries and empty suffix records survive; insertion
within an existing run may still retain its separately partitioned right part.
Unicode boundaries remain determined by core text coordinates. Transactions,
revision/undo semantics, public APIs and native schemas do not change. Regression
coverage must verify run growth, Unicode, exact overrides, retained partitions,
no-ops and undo/redo rather than merely checking final text.

## 041: Add format-specific adapters around the existing shared model

**Decision:** Add a small `office-formats` crate with concrete ODT, RTF, TXT and
PDF modules and dispatch to the existing DOCX adapter. Keep format syntax,
conversion limits and library dependencies outside `office-core`. Reuse the
current `WriterFile`/`OriginalSource` layer, model/native versions and existing
transaction, history and storage APIs. No schema, database or generic plugin
framework is introduced.

**Reason:** The owner explicitly authorized Step 7.5 after Step 7. Additional
file formats are now needed, while a separate semantic model per format would
split correctness, editing and pagination. The existing format-neutral source
record already supports exact originals without copying them into history.

**Consequences:** The complete editable projection is validated before replacing
any session. A supported external import is saved natively with its original
before browser editing activates. Unsupported rich semantics remain preserved
read-only under the existing policy; parsing is not a promise of fidelity. Native
reopen revalidates the retained source with its relevant adapter. A zero-byte TXT
has no nonempty original record, but its empty native document is still saved.
Native copying requires a complete `WriterFile` through `export_file`; the
document-only exporter rejects `Writer` instead of exposing a natural facade
round trip that silently omits a retained original. Foreign-format adapters
continue consuming the semantic document independently of that file layer.

## 042: Make Save As a reviewed conversion, separate from local Save

**Decision:** Expose readable formats through Open and all export formats through
Save As. Rust returns bounded format warnings with session/document/revision;
export checks the current origin and requires explicit acknowledgement when
warnings exist. Native Save/autosave continues through the existing IndexedDB
etag lane. Downloading a copy does not change that association or clean state.

**Reason:** External files cannot retain all native IDs, optional-property/run
distinctions and original bytes. TXT also loses formatting/metadata and richer
structural distinctions. A download is not proof of local persistence or an
atomic filesystem replacement. Users need the warning before publication, while
the full native in-memory/local copy remains intact.

**Consequences:** Choosing another format resets acknowledgement; stale reviews
and failed conversions emit no partial download and leave live work intact. The
browser controls destination and duplicate-name behavior. Retained-original
privacy disclosure still applies to source-bearing native exports. TXT uses
strict Unicode decoding and explicit BOM/line-ending rules rather than guesses.

## 043: Export PDF from Rust glyph geometry with explicit profile limits

**Decision:** Use pinned `pdf-writer` and `svgtypes` to serialize the existing
Rust pages/glyph outlines into bounded vector PDF, sharing repeated glyph forms.
Do not add another typesetter, font-selection policy or browser print layout.
PDF remains export only.

**Reason:** Existing deterministic page/glyph geometry already resolves shaping,
fonts, formatting and pagination. A low-level PDF writer can preserve its visual
result without competing with that engine or relying on installed browser fonts.
The selected libraries offer declared Apache-2.0/MIT alternatives and native/WASM
support; their new transitive graph and notices are reviewed separately.

**Consequences:** The first PDF profile draws outlines: text is not searchable or
selectable and is not tagged PDF/UA or PDF/A. Save As must disclose that, along
with layout/font diagnostics. Geometry errors fail explicitly. No cloud service,
full Office engine or proprietary font dependency is introduced. This targeted
dependency review does not select the project's license or replace the planned
comprehensive commercial licensing/security audit.

## 044: Classify DOCX auxiliary relationships without weakening semantic guards

**Decision:** Accept exact known package-root/main relationships only in their
correct roles, with matching content types, XML roots and an explicit inert
payload profile. Retain nonsemantic metadata and unused ancillary information
in the existing immutable original. Continue using the existing style parser;
do not resolve themes, discard active settings or infer support from filenames.

**Reason:** The old graph whitelist rejected ordinary Word/LibreOffice metadata,
theme, font-table, settings and web-settings parts before inspecting them. The
presence of these parts does not by itself imply content loss, whereas their
active properties and relationships may affect formatting or document behavior.

**Consequences:** An otherwise supported document can become editable without a
new model or schema. Unknown/external relationships, unresolved theme properties,
embedded fonts and unsupported active settings remain read-only; malformed or
ambiguous packages fail explicitly. All child relationship parts remain checked.
Bounded diagnostics identify the actual relationship Type, Target and Id.
Native saving preserves the exact original before editing. A regenerated DOCX
copy omits unmodeled auxiliary information and requires the existing Save As
acknowledgement; original download remains byte-identical. Real producer fixture
limits and evidence are recorded in [DOCX_RELATIONSHIPS.md](DOCX_RELATIONSHIPS.md).

## 045: Normalize known DOCX formatting while preserving content and originals

**Decision:** With the owner's explicit authorization, permit validated known
formatting approximations on editable import. Preserve all supported text/order,
exact originals and core validation. Keep content, revision, protection,
unknown-markup and malformed-structure safeguards. Report normalization notices
separately from blocking compatibility findings with independent bounds.

**Reason:** Ordinary Word and LibreOffice documents include unused style
libraries, theme/script defaults and layout preferences beyond Writer's model.
Requiring every formatting detail to match the native model made ordinary
content unavailable for editing without improving content integrity.

**Consequences:** Resolve supported used character styles into run properties;
retain unused libraries in the original and never prune active/default/inherited
numbering. Use deterministic documented font/color/layout fallbacks and display
an accessible adjustment notice after original-preserving local storage. Native
reopen derives runtime notices from the original without replacing edited state.
Existing import APIs remain available; a richer facade result carries notices
without a second parse. No native schema, dependency or layout-engine change is
needed. Regenerated exports disclose losses; full originals remain recoverable.
This is a bounded compatibility profile, not complete Word fidelity. See
[DOCX_NORMALIZATION.md](DOCX_NORMALIZATION.md) for evidence and safeguards.

## 046: Recover local saves and retain explicit browser export handoffs

**Decision:** Preserve the existing Rust session, IndexedDB etag lane and reviewed
conversion APIs. Allow transient database reconnection and a replacement epoch's
explicit Save to run after its predecessor's bounded drain. Retain one immutable
prepared export with native download/open links. Focus paged input synchronously
on completed touch gestures and reveal existing Rust geometry using VisualViewport.

**Reason:** Cached failed connections made retry ineffective; a new blank Save
could be swallowed by the old session's final write. Detached anchors and a
one-second Blob lifetime offered no dependable retry for browser file handoff.
Compatibility events and stale layout could swallow mobile focus. Browser APIs
cannot prove successful file delivery or a physical software keyboard's appearance.

**Consequences:** Failed saves keep unsaved memory and existing etags. Exports do
not mark local state saved; copy links disclose snapshot age and release memory
on replacement/dismissal/unmount. Modal shortcuts preserve conversion consent.
Keyboard presentation adjustments do not change document layout; inset padding
is bounded to prevent viewport feedback. No schema, adapter, dependency or undo
engine changes. Device verification remains required; see
[WRITER_RELIABILITY.md](WRITER_RELIABILITY.md).

## 047: Keep menu activation independent of pointer focus

**Decision:** Retain the native document disclosure and existing commands. Ignore
null-target blur; dismiss on outside pointer, Escape or known outside keyboard
focus. Before invoking a command, focus the visible summary so native dialogs
can return there. Close dialogs in layout cleanup before DOM removal; successful
session replacement closes its owned dialog stack before capturing return focus.
Constrain the existing scrollable panel to the visible viewport.

**Reason:** Safari can activate a button without focusing it. Closing on that
blur swallows its click; an unbounded panel also hides commands in short windows.
Changing every command or adding a custom menu library would obscure the shared
cause. There were no unfinished menu actions to replace.

**Consequences:** No delayed clicks, forced focus on every pointerdown, new menu
semantics, dependency or document-engine change. Existing consent, composition,
history, conflict and source-preservation guards remain. Browser/device testing
limits are explicit in [WRITER_MENU_RELIABILITY.md](WRITER_MENU_RELIABILITY.md).

## 048: Confirm meaningful copy losses with Save copy

**Decision:** Save As contains filename, format and Save copy. Remove ordinary-save
technical panels and acknowledgment checkboxes. Rust reports short warnings only
for meaningful conversion loss; clicking Save copy confirms that displayed
assessment. Keep backend confirmation and session/document/revision checks.

**Reason:** Blanket notices about IDs, run partitions and original archives made
every foreign save appear risky. They obscured real loss such as plain-text
formatting, outlined PDF text/accessibility or omitted source properties/bookmarks.

**Consequences:** Ordinary supported copies are warning-free. Native originals
receive a compact privacy hint rather than a loss warning. Mobile fields stack
at 16px and the dialog follows VisualViewport bounds. Copies never change the
live document, original, history, local association or etag. See [SAVE_COPY.md](SAVE_COPY.md).

## 049: Preserve validated DOCX source properties around regenerated content

**Decision:** Full-file export routes through `office-formats::export_file` to
the DOCX source-aware adapter. Regenerate current body/styles; carry validated
descriptive/custom metadata and safe auxiliary parts with fresh relationships
and content types. Require final package validation and equality with the
freshly regenerated semantic projection.

**Reason:** The document-only exporter could not see original metadata. Copying
the entire original package would risk stale/deleted text, dangling references
and unsupported content. A bounded source-aware adapter preserves useful data
without coupling the shared model to DOCX or pretending arbitrary OOXML is editable.

**Consequences:** Original bytes remain exact. Drop stale caches/audit counters,
previews and XML comments/PIs; omit unsafe opaque themes with a specific brief
warning. Normalize known Strict namespace declarations without changing property
values. Block malformed packages and unsupported body/relationship graphs.
An additive runtime `CompatibilityReport.has_formatting_normalizations` flag
tracks real approximations before bounded notices overflow; no native schema,
browser transport, dependency or editing-history format changes are needed.
Existing document-only APIs remain available; source preservation requires a full
file. This is a bounded compatibility profile, not full OOXML/XSD validation.

## 050: Project Rust selections onto touch handles without browser text reflow

**Decision:** Keep the SVG editor and native input/IME projection. Add Rust
word-selection and independent endpoint-drag actions to the existing layout
transport, and render their endpoint rectangles as touch handles. JavaScript
recognizes gestures and positions controls, without segmenting text or calculating
document offsets. Keep logical endpoint identities through reversals and bidi.

**Reason:** Native DOM handles cannot follow arbitrary Rust glyph paths. A
second visible contenteditable flow would compete with engine pagination. The
existing Rust geometry and pinned word/grapheme implementation already provide
the required boundaries; no new model, dependency or layout layer is needed.

**Consequences:** Selection does not change native bytes, revisions or history.
Gestures also guard exact selections, since selection-only changes do not advance
revisions. Native scrolling/pinch and IME own their respective input; cancellation
must not become a tap. Async plain-text clipboard cut/paste rechecks live Rust
identity, revision, selection and composition after browser I/O. Physical-device
behavior and OS-native handles/loupe are not claimed by browser automation.
See [PHASE2_MOBILE_SELECTION.md](PHASE2_MOBILE_SELECTION.md).

## 051: Classify external formatting separately from unsupported content

**Decision:** Extend the existing adapters with exact, validated producer-setting
recognition and disclosed known-formatting approximations. Keep independent
bounded notices and blockers, and track meaningful formatting before notice
truncation. Source-aware copy assessment reuses the import classification across
external destinations. Retain exact originals and existing native schemas.

**Reason:** LibreOffice resaves exposed harmless defaults and unsupported run
formatting being treated as content blockers. Ignoring arbitrary settings or
post-filtering a bounded report could instead publish an incomplete document.
RTL direction and associated script formatting are not inert metadata.

**Consequences:** Safe producer files become editable without relaxing protection,
content, resource or malformed-data checks. Approximate appearance is disclosed;
an unsupported feature still prevents a partial editable projection. ODT/RTF
regenerated copies retain supported semantics rather than arbitrary source XML
or control words. Full native files retain exact recovery sources. No dependency,
model, storage migration or additional formatting engine is introduced. See
[PHASE2_COMPATIBILITY.md](PHASE2_COMPATIBILITY.md).

## 052: Retain outlined PDF after cross-reader text-font experiments fail

**Decision:** Keep current outlined PDF and its limitations warning. Do not ship
the experimental Type 3 text-font path or its extra layout metadata. Archive the
experiment and evaluate a pinned mature CID/subsetting solution separately.

**Reason:** Exact Unicode mappings, unchanged Rust geometry and valid font syntax
did not preserve rendering across PDFium, Poppler and MuPDF. Bounded experiments
exposed color, metrics, caching and reader-order differences. No tested variant
met the existing visual-fidelity requirement.

**Consequences:** Production APIs, native state, font catalog and dependencies
stay unchanged. Searchable/selectable PDF remains unfinished. Exact variable-font
instances and shaped-source provenance are requirements for a follow-up, with
independent reader tests before acceptance. Tagged PDF/PDF-A are separate work
and no accessibility/conformance claim is made. See [PHASE2_PDF.md](PHASE2_PDF.md).

## 053: Stream validated library summaries without copying document bodies

**Decision:** Use a read-only cursor and existing record validation for metadata
listing. Preserve byte copies in load/save, all corruption warnings, sorting and
transaction completion. Keep the existing database and save/revision/etag policy.

**Reason:** Actual interleaved Chromium measurements show reduced binary-buffer
retention and listing latency. Cursor iteration alone with the old byte copies
was worse; both parts of the change are justified by measurement. A metadata
database migration is unnecessary for this correction.

**Consequences:** Large libraries avoid eager retention of every full result and
redundant owned copies. Each value is still read; native caching/GC can retain
multiple buffers, and small libraries may be slightly slower. Separate large-
document tests expose full-layout/transport/SVG costs; do not replace the engine
or add speculative caches as part of the storage fix. See
[PHASE2_PERFORMANCE.md](PHASE2_PERFORMANCE.md).


## 2026-11-15 — First-level semantic lists

Decision: Represent bullets and decimal numbered lists as optional `ListKind`
on each paragraph in the Rust model, not as inserted marker characters,
CSS `::before`, or an independent browser editing model. Consecutive numbered
paragraphs are one numbered group; normal text or bullet paragraphs reset the
sequence. Rust retains control of editing/undo, page shaping and glyph geometry.

Reason: The current engine's foundational block is Paragraph, and replacing
it with a nested-list block tree would require broad rewrites of Unicode
positions, selection, pagination, persistence and format adapters. Native
schema / Writer envelope advance to v3. Schema 1 and 2 use strict migrations;
old version numbers never implicitly accept list properties.

Word export uses `word/numbering.xml`, separate numbering definitions and
paragraph `numPr` references. Unsupported DOCX variants remain read-only.
TXT, ODT and RTF disclose list loss; native Writer and supported DOCX preserve
list structure. First-level only; nested lists, custom counters, split list
group identities, style-defined numbering and bidirectional list-marker
layout are deferred with clear compatibility restrictions.

Consequence: A native schema/version migration and list-aware DOCX validation
are required. Full build, Rust, file round-trip and real-device verification
must succeed before the feature is production-ready.

## 2026-11-15 — Inline semantic hyperlinks in Graphe

Chosen: Rust-owned, validated HTTP(S) link metadata on a text run's character
properties, with native v4 migration and transactional insertion/change/removal.
Do not insert raw HTML, run URL fetches, or use browser-only range maps.
Link formatting is non-inclusive in intent: editing commands can change an
existing linked run, but subsequent ordinary typed text should be unlinked.
Links cannot live in named styles. Rendering uses subdued blue/underline
unless explicitly overridden by the document.

OOXML links use w:hyperlink plus package external hyperlink relationships;
do not copy stale links from source DOCX documents. Unsupported Word link
semantics must remain preserved read-only. Other external formats must disclose
loss of click targets. This foundation can be extended with mailto and anchors
only after separate validation and format adapter work.


## 2026-11-16 — Transactional paragraph geometry controls

Expose existing `ParagraphFormat` via `set_paragraph_metrics`, with
`ParagraphMetricsPatch` that touches only supplied fields, merges individual
indentation components per paragraph, and preserves style, list, page-break,
alignment and text. Rust validates unit ranges, converts to canonical twips,
and commits one undoable transaction. The UI displays Rust's readonly
mixed-selection state and sends only changed fields on Apply; dialog input
cannot become an independent document model.

Do not change native schema version or regenerate document content to achieve
paragraph spacing. Native pages and DOCX already support these properties.
This release deliberately excludes page margins, style editing and tab stops.

## 2026-11-16 — Four new locally pinned font families

Add Carlito, Caladea, Source Sans 3 and JetBrains Mono as *real fonts* in both
the deterministic Rust shaping catalog and the UI's demand-loaded previews.
Keep the existing sans, serif and international fallback order. Native document
formats and requested font names do not change. Explicit substitutions:
Calibri→Carlito, Cambria→Caladea, Source Sans Pro→Source Sans 3; these are
compatibility aids, not Word pagination guarantees.

Maintain one complete OFL license per new family, combined NOTICE and hash-/
blob-verified manifest; never rely on network fonts or commercial system fonts.
Increase the font-catalog identity to v2. Keep a native grouped font selector
and small inspector preview, with unknown imported names preserved and labeled
as substituted. Defer more large families until WebAssembly bundle growth and
real-device performance are measured.

## 2026-11-16 — Four more pinned Graphe font families

Choose Lora, Montserrat, DM Sans and Bitter as opt-in families with real
upstream original TTF files, each including separate upright and italic
variable faces with actual weight axes. Add them to the Rust collection,
the native font menu and the lazy browser previews without changing the
fallback order or rewriting preserved imported font-family names. Increase
the pinned catalog marker to `writer-fonts-v4`; no document schema change.

Retain their complete unmodified SIL OFL 1.1 licenses alongside each source
asset and in the existing combined NOTICE; update provenance, immutable blob
identity and SHA-256 checksums. Expect 3.11 MB extra uncompressed font data
in the Rust/WASM build. Require native, WASM, browser and real-device checks
before claiming performance or rendering parity across platforms.

## 2026-11-16 — Opt-in Graphe font catalog v5

Add Work Sans, Crimson Pro, Fraunces, Roboto Mono and Oswald as five further
document-oriented font choices. Bundle nine exact official SIL Open Font
License files at a fixed upstream revision. Register fonts explicitly in
Rust layout and browser previews, with distinct regular/italic faces where
upstream provides them; Oswald remains upright only. Preserve the existing
fallback chain, DOCX source names, document schema and undo semantics.

Pin complete OFL notices in individual license files and existing combined
NOTICE, record SHA-256 and Git blob identity for every original binary, and
update the root proprietary LICENSE without re-licensing third-party fonts.
Change the layout catalog ID to `writer-fonts-v5`. Additional 2.53 MB of
uncompressed font bytes and Fraunces' unusual variation axes require native,
WASM, browser and real-device verification before deployment.

## 2026-11-16 — Clearly labeled common font compatibility choices

Add the familiar Times New Roman, Arial, Calibri, and Cambria names to
Graphe's font selector, each visibly marked as rendered using Tinos, Arimo,
Carlito, and Caladea respectively. Preserve the name selected by the author
in native documents and DOCX exports; preserve imported family metadata.
The Rust layout engine continues to draw from the already pinned open-source
faces, including an explicit Arial-to-Arimo mapping. No Microsoft binaries,
font renaming, file-format changes or network font downloads are allowed.
Substitution is disclosed in the inspector preview and the menu. Add native
font-outline and browser export regression coverage. Treat the remaining
metric/layout differences from Microsoft fonts as expected, not exact parity.

## 2026-11-18 — Tables reuse paragraphs and Rust layout

Implement bounded rectangular, unmerged, non-nested tables as a typed block
with stable table/row/cell IDs and existing paragraph content. This extends the
current engine rather than embedding a JavaScript editor. Keep text positions
paragraph-relative, distinguish cell/body containers for destructive edits, and
use explicit atomic structural transactions with existing exact-snapshot undo.
Selection spanning cells may inspect/copy/format text but cannot replace/join
cell containers implicitly. Column operations preserve valid geometry.

Advance native envelope/model to version 5 with strict legacy decoding. Rust
owns equal/explicit column widths, padding, shaping, intact-row pagination,
borders and hit testing. React projects only that geometry; its semantic table
is an input/assistive projection. An over-page row reports a layout limitation
instead of silently clipping content. Unsupported imports/exports keep explicit
loss/error/original safeguards. No dependencies were introduced. Primary sources,
alternatives and unresolved compatibility limits are in `TABLES_RESEARCH.md`.

## 2026-11-18 — Step 8A uses bounded, immutable raster blocks

Add format-neutral standalone `Block::Image` with stable IDs, original PNG/JPEG
bytes, twip dimensions, alt text and alignment. Reuse transactions, bounded undo
and Rust layout instead of adding an object editor or a browser document model.
A selected image is Rust state; structural image barriers protect text edits.

Use pinned low-level `png`, `zune-jpeg`/`zune-core` and `kamadak-exif` rather than
host/browser codecs or a general image-processing framework. Bound compressed
bytes, metadata, dimensions and aggregate pixels before decoding. Share immutable
derived oriented RGBA/PNG across clones, apply all eight EXIF orientations, and
disclose unsupported color-management approximations. Preserve complete upstream
licenses in the repository and production About notices. Native preservation may
retain private EXIF metadata; it is not a sanitization operation.

Advance native model/envelope to 6 with strict 1–5 migrations. Keep DOCX and ODT
pictures in adapters and reject incomplete supported projections. Regenerate
media relationships from current content; never graft deleted original images.
PDF paints the same normalized pixels/geometry with alpha. TXT requires meaningful
loss disclosure and RTF images are refused until represented safely. Ordinary
PNG/JPEG bytes can be retained externally; orientation/color normalization uses
canonical PNG and reports its limits. See IMAGES.md and IMAGES_RESEARCH.md.

## 2026-11-18 — External pinned font bytes for browser WASM

Keep the canonical licensed catalog and native embedding; exclude font binaries
from WASM with target configuration. Compile trusted manifest descriptors and
preserve the existing registration order rather than manifest entry order.
Use RustCrypto sha2 0.11.1 for independent Rust verification and WebCrypto for
browser cache integrity. Only a complete catalog can activate layout/PDF.
Serve original fonts through existing static assets with manifest-digest paths,
reuse verified bytes for CSS FontFace previews and retain versioned partial caches.
No lazy partial fallback, host font discovery, R2, service worker, document schema
change or deployment-setting change is needed. See EXTERNAL_FONTS_RESEARCH.md.

## 2026-11-18 — Step 8C.2 section boundaries use existing sections

Keep `Section` and `SectionProperties` as the sole format-neutral source for
next-page breaks and independent page geometry. Structural editing is one
detached Rust transaction: split a body paragraph only when necessary, retain
existing block and paragraph identities, inherit exact section properties, and
give the new section a new ID. The pinned layout engine already starts each
section on a new page; React cannot decide pagination or construct section
records. Joining sections preserves all blocks but applies the preceding
section's geometry; require an explicit confirmation when geometry differs
and show that consequence before every UI removal. Existing native model and
file-format versions remain unchanged. Continuous/odd/even section break types,
table-cell break insertion and full Word section semantics are deferred.
See SECTION_BREAKS_8C2.md for tests and limitations.

## 2026-11-18 — Step 8C.3 paragraph pagination rules

Add optional, style-inheritable `keep_together` and `keep_with_next` to the
shared `ParagraphFormat` with exact-snapshot undo, atomic Rust format patches,
and a strict native schema/envelope version 7. A frozen schema-6 reader covers
body and table-cell paragraphs, named styles, table geometry, and images; older
schema readers reuse that exact former formatting shape to reject pagination
flags under historical version labels. Absent flags are omitted from saved JSON,
preserving historical payload shapes until an explicit choice is made.

Rust lays out kept paragraphs before placing any lines. If a paragraph is too
large for a page, it falls back to normal line splitting with an explicit
diagnostic. `keep_with_next` keeps a body paragraph with the following body's
first line when that pair fits a page. A one-entry shaped-line cache avoids
repeated font shaping/glyph budgets. This is a bounded first profile, not
Word-compatible grouping across long heading chains or tables, and not
widow/orphan control. DOCX/ODT/RTF import/export use the corresponding known
paragraph properties; TXT export declares pagination loss, PDF renders the
already authoritative Rust layout. No new dependencies or browser text
measurement. See PARAGRAPH_PAGINATION_8C3.md.

## 2026-11-18 — Step 8C.4 widow/orphan control defaults

Enforce a bounded two-line widow/orphan minimum in the authoritative Rust body paragraph pagination, without changing the native schema or introducing frontend text measurement. Two- and three-line paragraphs move whole when they fit a blank page; longer paragraphs reserve their first two lines and keep the final pair on the same page if at least two lines of the paragraph are already placed there. A forced Keep with next also reserves the following two lines when possible. Unavoidable tall-line and small-page cases fall back to the established pagination rather than silently dropping content or looping. Table-row pagination and explicit source-level widow-control toggles are deferred.

## 2026-11-18 — Direct image manipulation, in-flow editing

Image manipulation is native Rust document editing, not CSS document placement.
The selected image paints eight handles. Corner resizing preserves aspect ratio;
side handles can change one dimension. On touch, two-pointer pinch changes image
dimensions and a single-finger drag reorders the in-flow image relative to
neighboring body blocks. Desktop drag uses the same structural operation.
Drag placement snaps horizontally to left, center or right, and vertically to
an insertion boundary between block IDs in the same section. It is one
transaction and one undo step. No new schema, new packages, free-floating
coordinates, paragraph wrap-around or cross-section moves. The browser may
render a transient gesture preview but never persists viewport coordinates.
Image bytes, native IDs, precise Image Properties and PDF page layout continue
to be Rust-owned. See IMAGE_DIRECT_MANIPULATION.md for UX and limitations.

## 2026-11-18 — Step 8 audit corrections on main

The owner explicitly authorized direct-main commits without branches/PRs.
Preserve the current rectangular table schema while adding validated column-width
transfers that keep total width and stable IDs. DOCX must export supported keep
flags rather than silently omit them. Custom pinch suppression is scoped to image
touches; cancellation never commits an unfinished edit. Advanced merged cells,
cell appearance and split-row layout remain unimplemented, not silently flattened.
See STEP8_AUDIT.md for corrections, actual results and remaining work.
