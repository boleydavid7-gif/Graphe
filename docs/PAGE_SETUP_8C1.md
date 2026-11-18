# Graphe 8C.1 — Page setup and manual breaks (development)

Branch: `dev/graphe-step8c1-page-setup`. **Not merged or deployed.**

## Scope

- Layout → **Page setup…** edits the focused section's real Rust page size, orientation and four body margins. Paper presets are US Letter, A4, and US Legal; custom 3–22-inch dimensions are permitted. Margins are 0–4 inches.
- Existing header, footer, and gutter offsets are preserved, even when the document originated in DOCX or ODT. Applying changes is a single revision-guarded, validated, undoable Rust transaction. Invalid body geometry is rejected.
- Insert → **Page break** splits the text paragraph at the collapsed cursor and flags the new paragraph to start on a new page. Layout → **Start on new page / Remove page break** toggles the paragraph property without changing text.
- There is no second browser-owned document model and no document schema bump. The existing layout, native, DOCX and ODT format paths consume the existing section and paragraph properties.

## Limitations and verification

- Page setup applies to **one selected section**, not the whole document. Creating/removing section boundaries is reserved for 8C.2.
- Inserting a page break inside a table cell or a selection spanning text is not supported.
- Editing page setup intentionally enforces bounded paper and margin ranges; unusual imported settings are retained until explicitly changed.
- For review before merge: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm run build`, `npm run lint`, `npm test`, `npm run test:wasm`, `npm run test:build`, and focused Chromium tests.
- Changes have not yet been run against a compiled Rust/WASM build or physical device in this environment. Do not claim green CI, merge or deployment.
