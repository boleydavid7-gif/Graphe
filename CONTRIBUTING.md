# Contributing to Graphe

Thank you for helping improve Graphe, its independent Rust document engine, and its browser editor.

> This repository is in **private open-source preparation**. Contributions will be accepted publicly only after a reviewed release. Do not publish this snapshot or its earlier Git history.

## Before you start

Read [the README](README.md), [engineering rules](AGENTS.md), [current release gates](docs/OPEN_SOURCE_RELEASE.md), [license](LICENSE), [trademark guidance](TRADEMARKS.md), and [code of conduct](CODE_OF_CONDUCT.md).

For a significant feature or document-model change, discuss the proposed approach in an issue before implementation. Small bug fixes and documentation corrections can go straight to a focused pull request.

## Building and testing

You need Rust stable (edition 2024), Node 24, npm, the `wasm32-unknown-unknown` target and a compatible `wasm-bindgen-cli` as described in the README.

At the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

In `apps/writer`:

```sh
npm ci
npm run build:wasm
npm run typecheck
npm run lint
npm test
npm run test:wasm
npm run test:build
npm run build
```

When user interaction changes, add relevant browser tests (`npm run test:browser`) and identify physical-device tests not run. Do not report an unexecuted check as passing.

## Pull requests

- Keep each PR focused; explain the user-visible change and link an issue when applicable.
- Include tests for document preservation, errors, format imports/exports, Unicode and undo/redo as applicable.
- Preserve the Rust engine as the document/layout authority and respect current backwards-compatibility rules.
- Include a screenshot of your own synthetic sample for visible UI changes; **never** attach private user documents, tokens, logs containing personal data or customer data.
- Declare new dependencies, asset origins, their exact licenses, and any third-party excerpts in the PR. Update notices and lockfiles as required.
- Do not copy code from Microsoft Office, LibreOffice, ONLYOFFICE or other editors unless there is clear, compatible permission and full attribution.
- Confirm that you own the contribution or have permission to offer it under MPL 2.0, and that your submission does not conflict with obligations to an employer or another licensor.

**Contribution terms:** New original source contributions are offered under the Mozilla Public License 2.0 unless the maintainers explicitly arrange otherwise in writing. Do not submit third-party source that cannot legally be distributed under the intended project terms. Third-party assets keep their own licenses; contributors must document them.

Maintainers decide whether, when and how contributions are merged. Participation does not convey rights to Graphe or ProairetOS branding.

## Security and privacy

See [SECURITY.md](SECURITY.md). Do not post vulnerabilities, exploit chains, documents from real people or credentials in public issues.
