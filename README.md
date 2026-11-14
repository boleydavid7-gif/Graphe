# Office Engine

Office Engine is an independent Rust engine intended to power a professional
productivity suite, with correctness, performance, compatibility, maintainability,
and long-term architectural stability as priorities. The goal is to compete with
Microsoft Office, LibreOffice, and ONLYOFFICE through incremental development.

Writer is the first application. Word processing and Microsoft Word DOCX
compatibility come first; spreadsheets and presentations will follow later.

## Current status

Step 1 establishes the repository foundation: a Cargo workspace containing only
`office-core`, a public engine-version function, a unit test, documentation, and
GitHub Actions checks. There is no document model, editing, serialization, DOCX
parsing, layout, rendering, or user interface yet.

The project does not use an existing complete Office or word-processing engine.
It currently has no external Rust dependencies. Established low-level libraries
may be introduced when their responsibilities become necessary.

## Technology

- **Rust stable, edition 2024** provides the independent core engine.
- **Cargo** manages the workspace and dependencies. Shared dependency versions
  will use Cargo workspace dependencies when needed.
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

## Repository

- `crates/office-core/` contains the sole engine library crate.
- `docs/ARCHITECTURE.md` describes long-term boundaries and principles.
- `docs/ROADMAP.md` records the preliminary development sequence.
- `docs/DECISIONS.md` explains the initial technology and scope decisions.
- `tests/fixtures/` is reserved for future compatibility fixtures.
- `.github/workflows/ci.yml` defines the required validation workflow.

Additional crates will be introduced only as their responsibilities become
necessary.

## Licensing

The licensing decision is pending. The project owner has not yet chosen whether
the engine will be open-source, source-available, or proprietary. No software
license has been selected, and the `LICENSE` file is intentionally absent until
that decision is made.
