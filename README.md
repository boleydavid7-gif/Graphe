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

Milestone 3C adds Unicode-safe text insertion, deletion, and replacement,
character and paragraph formatting, style assignment, and bounded undo/redo.
Operations stage validated candidates inside transactions; each changed commit
is one undo step.

DOCX parsing, layout, rendering, and a user interface remain future work.
Style inheritance is stored and validated; effective formatting is not resolved.

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
document. Construction APIs assemble complete parts and validate them:

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
storage or collaboration. Text operations and in-memory history build on this
boundary without changing the document schema or adding dependencies.

## Editing and undo/redo

Use `Transaction::position` and `Transaction::selection` to create editing
endpoints for the current draft. Raw inspection positions and selections cannot
be passed to operations. A changed operation or any `draft_mut()` borrow
invalidates existing editing endpoints; reacquire them before another operation.
Failed and no-op operations preserve them. Every operation returns whether its
candidate changed, while publication still requires a successful commit.

```rust
use office_core::{
    Block, CharacterFormat, CharacterFormatPatch, Document, DocumentMetadata,
    DocumentSession, GraphemeOffset, Id, Paragraph, PropertyChange, Section, TextRun,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paragraph_id = Id::new("paragraph-1")?;
    let original = Document::from_parts(
        Id::new("document-1")?,
        DocumentMetadata::default(),
        vec![Section::new(
            Id::new("section-1")?,
            vec![Block::Paragraph(Paragraph::new(
                paragraph_id.clone(), vec![TextRun::new("Hi")],
            ))],
        )],
        vec![],
    )?;
    let mut session = DocumentSession::new(original.clone());
    let mut transaction = session.begin_transaction();
    let at = transaction.position(&paragraph_id, GraphemeOffset::new(2))?;
    assert!(transaction.insert_text(&at, ", 世界!", CharacterFormat::default())?);

    // Insertion changed the draft, so acquire fresh formatting endpoints.
    let start = transaction.position(&paragraph_id, GraphemeOffset::new(0))?;
    let end_offset = transaction.text_coordinates(&paragraph_id)?.grapheme_len();
    let end = transaction.position(&paragraph_id, end_offset)?;
    let selection = transaction.selection(start, end)?;
    transaction.apply_character_format(&selection, &CharacterFormatPatch {
        bold: PropertyChange::Set(true),
        ..Default::default()
    })?;
    session.commit(transaction)?;
    assert_eq!(session.document().text_coordinates(&paragraph_id)?.text(), "Hi, 世界!");
    let edited = session.document().clone();

    session.undo()?; // Both operations were committed as one undo step.
    assert_eq!(session.document(), &original);
    session.redo()?;
    assert_eq!(session.document(), &edited);
    Ok(())
}
```

Inserted text uses explicitly supplied `CharacterFormat`. Formatting patches
distinguish `Keep`, `Set`, and `Clear`; clearing restores an unspecified property
eligible for inheritance. Character formatting can span sections. Paragraph
formatting and style assignment apply to touched paragraphs; a collapsed range
targets its paragraph, and a range ending at offset zero excludes that final
paragraph. Collapsed character formatting does not create typing state.

Text replacement can span paragraphs in one section. It preserves the start
paragraph's ID, style, and properties, retains the end suffix, and removes covered
paragraphs. Cross-section text replacement is rejected. CR and LF remain literal
text rather than creating paragraphs. New text adjacency can merge graphemes, so
operations return no implicit result cursor; inspect the new draft coordinates.

`DocumentSession::new` retains up to `DEFAULT_HISTORY_LIMIT` (100) complete undo
snapshots. `with_history_limit` chooses another count, including zero to disable
history. New changed commits clear redo; no-ops and rejected commits preserve it.
Undo and redo restore exact model values under fresh increasing revisions, so
old revision tokens remain stale. `clear_history` preserves document and revision.
History is bounded by snapshot count rather than bytes and is not serialized;
there is no automatic typing coalescing or grouping beyond a transaction.

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
