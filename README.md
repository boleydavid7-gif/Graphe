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

There is no editing, DOCX parsing, layout, rendering, or user interface yet.
Style inheritance is stored and validated; effective formatting is not resolved.

The project does not use an existing complete Office or word-processing engine.
Serde and serde_json support model serialization. Established low-level libraries
may be introduced when their responsibilities become necessary.

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
for the schema contract, validation rules, Unicode position plan, and compatibility
limitations. A [sample Writer document](tests/fixtures/writer-sample-v1.json)
demonstrates schema version 1.

## Repository

- `crates/office-core/` contains the sole engine library crate.
- `docs/ARCHITECTURE.md` describes long-term boundaries and principles.
- `docs/ROADMAP.md` records the preliminary development sequence.
- `docs/DECISIONS.md` explains the initial technology and scope decisions.
- `tests/fixtures/` contains the versioned sample document and will hold future
  compatibility fixtures.
- `.github/workflows/ci.yml` defines the required validation workflow.

Additional crates will be introduced only as their responsibilities become
necessary.

## Licensing

The licensing decision is pending. The project owner has not yet chosen whether
the engine will be open-source, source-available, or proprietary. No software
license has been selected, and the `LICENSE` file is intentionally absent until
that decision is made.
