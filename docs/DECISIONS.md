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
