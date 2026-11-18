# Graphe

An independent word processor built around a Rust document engine, with a browser interface powered by React and WebAssembly.

Graphe is intended to be free to use and open source under the Mozilla Public License 2.0. Its source code includes the Graphe browser application and the independent Office Engine. It is developed separately from the broader **ProairetOS** product.

> **Maintainer note:** This file is a **draft of the README for the new public repository**, not proof that the source has been released. Replace the top-level README with a reviewed version of this file only after the [publication gate](OPEN_SOURCE_RELEASE.md) is cleared.

## What it does

- Write, edit and format text, with paragraphs, headings, lists and tables within declared support limits.
- Insert embedded PNG/JPEG images, with supported in-flow resizing and positioning.
- Open/save native `.writer` documents. Import/export supported portions of DOCX, ODT, RTF and plain text.
- Export PDF pages generated from the Rust layout engine.
- Edit in desktop and mobile browsers, with local persistence and explicit document exports.

**Limitations:** Graphe does not claim full Microsoft Word, LibreOffice or ONLYOFFICE compatibility. Unsupported rich content is retained for recovery or opened read-only where required; it must not be silently discarded. Certain advanced table behavior, floating pictures, PDF text searchability, browser/device edge cases and independent Word interoperability remain incomplete. See [file formats](FILE_FORMATS.md), [image scope](IMAGES.md), [table scope](TABLES.md) and [verification reports](STEP8_AUDIT.md).

## Architecture

- `crates/office-core` — structured document model, editing, validation, transactions, undo/redo and native persistence
- `crates/office-docx` — bounded WordprocessingML/OPC parsing and conversion
- `crates/office-layout` — deterministic typography, page geometry and bundled font catalog
- `crates/office-formats` — ODT/RTF/TXT adapters and PDF generation
- `crates/office-wasm` — browser-facing engine API
- `apps/writer` — React/TypeScript editor, file handling and user interface

Rust is the authority for document content, layout and history; JavaScript presents the interface.

## Build locally

Requirements: Rust stable with Cargo, Node.js 24.x, npm and the WebAssembly target.

```sh
rustup target add wasm32-unknown-unknown
# Install wasm-bindgen-cli version 0.2.130 using the exact toolchain instructions in the repository README.
cargo fmt --all -- --check
cargo test --workspace
cd apps/writer
npm ci
npm run build:wasm
npm run dev
```

The initial `build:wasm` command needs a compatible `wasm-bindgen-cli 0.2.130`. For full production validation, run Rust Clippy/build, frontend lint/typecheck, compiled-WASM tests, build tests and browser tests as described in the repository documentation.

## Contributing and security

See [CONTRIBUTING](../CONTRIBUTING.md), [community standards](../CODE_OF_CONDUCT.md), [governance](../GOVERNANCE.md) and [security reporting](../SECURITY.md). Please provide synthetic minimal examples rather than real people's confidential documents.

## Licensing and brand

Original Graphe/Office Engine source is intended to be available under [MPL 2.0](../LICENSE). Bundled fonts, third-party packages, test materials and other components keep their own licenses; see [NOTICE](../NOTICE.md) and [the licensing audit](COMMERCIAL_LICENSING_AUDIT.md). Software licensing does not grant rights to impersonate the official Graphe or ProairetOS product; see [trademark guidance](../TRADEMARKS.md).

Graphe is an independent implementation and is not affiliated with, certified by, or endorsed by Microsoft, LibreOffice or ONLYOFFICE.
