# Graphe 8C.2 — Section boundaries and independent page setup

Development branch: `dev/graphe-step8c2-sections`. **Not merged or deployed.**

## Authorized scope

- Insert → **Section break** splits a *body* paragraph at a collapsed text caret, or moves the existing body paragraph unchanged when the caret is at offset zero with preceding body content. Following body blocks remain ordered and retain their IDs, text, formatting, tables, and images.
- Rust creates a new `Section` with a fresh ID and copies the preceding section's *exact* optional page properties. Existing page setup continues to edit just the active section, allowing landscape pages or different margins/paper sizes within one document.
- Layout → **Remove section break…** joins the selected section to its predecessor, retaining both sections' blocks in order and using the preceding section's page geometry. The UI requires confirmation. Rust separately rejects geometry-changing joins without an explicit acknowledgement.
- Every operation is one validated, revision-guarded core transaction and one undo/redo step. Section IDs, existing paragraph IDs and native file schemas stay stable; no new schema or format dependency is introduced.
- The existing Rust layout engine already starts each section on a fresh page. This step adds **next-page** section breaks only; continuous, even-page, and odd-page section-break types are not claimed.

## Deliberate limits

- Section insertion is rejected in tables, across noncollapsed selections, and at the very beginning of a section (to avoid creating a blank preceding page).
- Removal joins complete sections and does **not** combine text paragraphs or discard blocks. Content from the deleted section adopts the predecessor's page layout. DOCX/ODT may not match full Microsoft Word section semantics, and advanced compatibility needs separate verification.
- The selected section is determined by the Rust selection or image position. Browser rendering, text measurement, page pagination, and undo remain Rust-authoritative.
- Validation required before merging: `cargo fmt --all -- --check`, strict workspace Clippy, `cargo test --workspace`, `npm run build` / `typecheck` / `lint` / `test` / `test:wasm` / `test:build`, and focused browser tests, plus real-device checks.
- The GitHub connector can commit/review source but cannot itself run the full Rust/real-browser suite. Avoid claiming a green build or a deployed feature without actual checks.
