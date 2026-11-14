# Office Engine

Office Engine is an independent Rust engine intended to power a professional
productivity suite, with correctness, performance, compatibility, maintainability,
and long-term architectural stability as priorities. The goal is to compete with
Microsoft Office, LibreOffice, and ONLYOFFICE through incremental development.

Writer is the first application. Word processing and Microsoft Word DOCX
compatibility come first; spreadsheets and presentations will follow later.

## Current status

Steps 1 and 2 establish the Cargo workspace and a structured document model in
the sole `office-core` crate. Documents contain sections, paragraph blocks, UTF-8
text runs, formatting, metadata, and named paragraph styles. Construction and
versioned JSON loading validate identifiers, references, and formatting.

Milestone 3A adds explicit UTF-8, UTF-16, and grapheme offsets, checked document
positions, and directed selections. Cursor boundaries use Unicode extended
grapheme clusters across complete paragraphs, including formatting-run boundaries.
The document model and its schema remain unchanged.

Milestone 3B adds detached transaction drafts, atomic validated commits, and
session-scoped revision tokens. Committed documents remain immutable to callers;
draft changes become visible together only after a successful commit.

There are no text-editing operations, DOCX parsing, layout, rendering, or user
interface yet.
Style inheritance is stored and validated; effective formatting is not resolved.
Milestone 3C (text editing, formatting operations, and undo/redo) has not started.

The project does not use an existing complete Office or word-processing engine.
Serde and serde_json support model serialization. The low-level
unicode-segmentation library supplies Unicode grapheme boundaries. Additional
libraries may be introduced when their responsibilities become necessary.

## Technology

- **Rust stable, edition 2024** provides the independent core engine.
- **Cargo** manages the workspace and shared dependency versions through
  `[workspace.dependencies]`.
- **WebAssembly** is a future compilation target for browser integration.
- **React and TypeScript** are planned for the future interface; neither is
  included in this milestone.
- **GitHub Actions** runs formatting, lint, test, and build checks on pushes and
  pull requests.

The core engine is independent of React, JavaScript, and browser APIs. Native
desktop applications should be able to use the same engine.

## Development

Install [Rust through rustup](https://rustup.rs/), then run these commands from
the repository root. `rust-toolchain.toml` selects stable Rust and the required
formatting and linting components. To install them explicitly:

```sh
rustup toolchain install stable --profile minimal --component rustfmt --component clippy
```

Run the same checks as continuous integration:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

To apply formatting locally, run `cargo fmt --all`. Compiler and Clippy warnings
are treated as errors. `Cargo.lock` is committed to preserve dependency
resolution; pass `--locked` to Cargo build, test, and Clippy commands when a
lockfile update must be rejected. The stable toolchain tracks stable releases,
so the lockfile does not pin the compiler version.

## Model API

IDs are supplied by the caller and must remain stable and unique within a
document. Assemble complete parts and validate them; no text-editing API is
included:

```rust
use office_core::{Block, Document, DocumentMetadata, Id, Paragraph, Section, TextRun};

fn example() -> Result<(), Box<dyn std::error::Error>> {
    let paragraph = Paragraph::new(Id::new("paragraph-1")?, vec![TextRun::new("Hello, 世界!")]);
    let section = Section::new(Id::new("section-1")?, vec![Block::Paragraph(paragraph)]);
    let document = Document::from_parts(
        Id::new("document-1")?,
        DocumentMetadata { title: "Example".into() },
        vec![section],
        vec![],
    )?;

    document.validate()?;
    let json = document.to_json()?;
    let restored = Document::from_json(&json)?;
    assert_eq!(restored, document);
    Ok(())
}
```

Use `Document::new(id)` for an empty document. Inspection methods return shared
references and ordered slices. See [the architecture document](docs/ARCHITECTURE.md)
for the schema contract, validation rules, Unicode position conventions, and compatibility
limitations. A [sample Writer document](tests/fixtures/writer-sample-v1.json)
demonstrates schema version 1.

## Positions and selections

`Utf8Offset`, `Utf16Offset`, and `GraphemeOffset` express different units. Their
numeric constructors do not validate text; `TextCoordinates` performs strict
conversions without snapping, clamping, or Unicode normalization. Raw UTF-8/UTF-16
conversions require scalar boundaries. Cursor positions additionally require
extended grapheme boundaries.

```rust
use office_core::{Document, GraphemeOffset, Id, Utf16Offset};

fn select_to_utf16_boundary(
    document: &Document,
    paragraph_id: &Id,
    end_offset: Utf16Offset,
) -> Result<(), Box<dyn std::error::Error>> {
    let coordinates = document.text_coordinates(paragraph_id)?;
    let start = document.position(paragraph_id, GraphemeOffset::new(0))?;
    let end = document.position(paragraph_id, coordinates.utf16_to_grapheme(end_offset)?)?;
    let selection = document.selection(start, end)?;
    let range = selection.ordered_range(document)?;
    assert_eq!(range.start.paragraph_id(), paragraph_id);
    Ok(())
}
```

The supplied UTF-16 endpoint must be a grapheme boundary. For example, offset 1
is valid in a paragraph containing only ASCII `A`; an offset inside a combining
sequence or surrogate pair returns a structured `PositionError`. For a direct
grapheme index, use `document.position(paragraph_id, GraphemeOffset::new(1))`.

Positions use stable document and paragraph IDs rather than run or array indices.
Selections preserve anchor/focus direction and expose a logical ordered range.
Positions and coordinate snapshots are runtime values; they do not track future
edits or change document serialization. See [architecture](docs/ARCHITECTURE.md)
for empty-paragraph behavior, cross-paragraph ranges, and snapshot limitations.

## Atomic transactions

`DocumentSession` owns a validated document and its current `Revision` token.
`begin_transaction` clones candidate metadata, sections, and styles into a
detached `DocumentDraft`. Drafts may be temporarily invalid; `commit` consumes a
transaction and validates the final candidate before publishing anything.
Dropping a draft or rejecting a commit leaves the document and revision unchanged.

```rust
use office_core::{Document, DocumentSession, Id};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = DocumentSession::new(Document::new(Id::new("document-1")?)?);
    let origin = session.revision();
    let mut transaction = session.begin_transaction();
    transaction.draft_mut().metadata.title = "Research report".into();
    assert_eq!(session.document().metadata().title, "");

    let outcome = session.commit(transaction)?;
    assert!(outcome.changed());
    assert_eq!(session.document().metadata().title, "Research report");
    assert!(session.validate_revision(&origin).is_err());
    session.validate_revision(outcome.revision())?;
    Ok(())
}
```

Every changed commit advances the session revision once. An identical candidate
is a no-op and preserves the revision. Foreign-session and stale transactions
are rejected, including stale candidates that happen to equal current content.
Start a new transaction after a conflict; the engine does not merge or retry it.

Capture the originating revision alongside positions or selections. Before
reusing them, call `session.validate_revision(&origin)?`, then the existing
document position or selection validation. Bare 3A positions do not carry a
revision: pairing an old position with a fresh token does not make it current.
Changed commits invalidate all earlier tokens, even for unchanged paragraphs;
no-op commits preserve them. There is no automatic rebasing or affinity policy.

Session revisions are runtime state, separate from document schema version 1.
The token includes opaque session identity, so equal counter numbers in different
sessions do not identify the same revision. Transactions provide in-memory
publication through an exclusive Rust borrow; they do not provide durable
storage, collaboration, text operations, or history. No dependencies were added
for 3B.

## Repository

- `crates/office-core/` contains the sole engine library crate.
- `docs/ARCHITECTURE.md` describes long-term boundaries and principles.
- `docs/ROADMAP.md` records the preliminary development sequence.
- `docs/DECISIONS.md` explains the initial technology and scope decisions.
- `tests/fixtures/` contains the versioned sample document and official Unicode
  grapheme-break conformance data.
- `.github/workflows/ci.yml` defines the required validation workflow.

Additional crates will be introduced only as their responsibilities become
necessary.

## Licensing

The licensing decision is pending. The project owner has not yet chosen whether
the engine will be open-source, source-available, or proprietary. No software
license has been selected, and the `LICENSE` file is intentionally absent until
that decision is made.
