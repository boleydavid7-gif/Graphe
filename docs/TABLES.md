# Basic tables

Implementation branch: `dev/graphe-tables`, based on main `fe4e094` (18 November
2026 inspection). Graphe remains in `apps/writer`. The unfinished release audit
in the separate `/workspace/stuff` checkout is preserved; this branch does not
merge that work, change unrelated applications, or deploy.

## Working profile

- Insert a 1–20 row/column rectangular table after the current body paragraph.
  The browser command limit is deliberately smaller than the bounded native
  model (1,000 rows, 32 columns, 4,096 cells).
- Type, delete, select, paste plain text, format and create paragraphs inside
  cells using the existing Rust text engine. Tab/Shift+Tab moves between cells;
  the last cell exits to the following body paragraph when available.
- Insert/remove rows and columns, and delete a table. Each structural command is
  one validated, undoable Rust transaction. Final row/column deletion is disabled;
  Delete table is explicit. Retained cells keep their identities.
- Column insertion splits a donor width; removal returns that width to a
  neighbor. Explicit total width is preserved, including imported full-page tables.
- Rust shapes and wraps text, computes fixed 4pt padding and 0.5pt neutral grid,
  maps paragraph-relative Unicode positions and paginates whole rows. React
  renders cell rectangles and existing glyph paths. A semantic HTML table is
  the controlled native-input projection, with surviving DOM nodes retained.
- Native format/schema 5 stores table/row/cell/paragraph IDs and full supported
  content. Versions 1–4 migrate strictly; older labels cannot introduce tables.
  Local save/autosave reuse the existing serialized IndexedDB write lane.

Cross-cell selections can copy/format existing text; destructive replacements
crossing cells or a table in body flow are rejected unchanged. Plain clipboard
text uses paragraph newlines rather than a rich table representation. Selecting
an entire document containing a table and typing therefore does not flatten it.

## Formats and preservation

| Destination | Table behavior |
| --- | --- |
| `.writer` | Exact supported model/IDs and retained original source bytes. |
| DOCX | Rectangular grid, cell paragraphs, supported run/paragraph formatting and widths; regenerated simple table appearance. |
| ODT | Rectangular table, cell paragraphs, supported formatting and widths; appearance normalization disclosed. |
| PDF | Export only; same Rust pages, glyph outlines and table grid. Existing search/selection/accessibility limitations remain. |
| TXT | Explicit meaningful-loss notice, then deliberate tabs between cells and newlines between rows/paragraphs. |
| RTF | Table export returns a structured unsupported error; source RTF tables remain preserved read-only. |

Merged/nested tables, formulas, protection, revisions, vertical text and unknown
content-bearing extensions retain strict import guards. An unsupported document
does not become an editable partial table. Exact original download remains
separate from regenerated current-content export. See [format verification](TABLES_FORMATS.md).

## Limits

Rows do not split across pages. A row taller than the usable page, an internal
cell page break, an over-wide grid or padding leaving no text width produces an
explicit layout error; source content remains intact and text fallback remains
available. Very narrow unbreakable spans retain the existing overflow diagnostic.
No merged cells, nested tables, repeating headers, row resizing, column drag
resizing, border/shading controls, cell-range selection or rich table clipboard
is claimed. DOCX section boundaries ending in an unrepresentable table position
are rejected explicitly. This is not arbitrary Word/LibreOffice table fidelity.

No new dependencies, fonts, database migration, browser storage implementation,
JavaScript document model or alternate undo history were added. Native schema
evolution is the necessary persistent change. Old application builds correctly
reject version-5 files rather than interpreting them incorrectly.

## Verification

The [research review](TABLES_RESEARCH.md) records primary sources and the initial
paragraph-only assumptions corrected in core, layout, adapters and WASM. The
format report records a 36-row/72-cell/75-paragraph realistic fixture and actual
LibreOffice cross-conversion and Poppler PDF rendering.

Commands run from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --workspace
```

Commands run from `apps/writer`:

```sh
npm ci
npm run build
npm run typecheck
npm run lint
npm test
npm run test:wasm
npm run test:build
npm run test:browser -- --project=chromium --workers=2
```

The final complete browser run follows correction of stale baseline tests: the
previous complete run executed 208 tests (195 passed, 13 failed). Failures
expected retired Open shortcuts, the former startup label/font ordering, native
version 2, a now-ambiguous font control, or an obsolete embedded Save As workflow.
Tests now exercise current menus, strict version-5 migration and the real trusted
parent Files protocol without weakening content/identity/etag assertions. The
two About dialog focus regressions failed before its opener was routed through
the existing visible-summary focus helper. No unrelated application was changed.

Final verification:

| Check | Actual result |
| --- | --- |
| Rust formatting, strict Clippy and workspace build | Passed. |
| `cargo test --workspace --locked` | 730 passed, 0 failed/ignored, including doctests. |
| `npm ci`, production build, TypeScript and ESLint | Passed; 182 packages installed; no dependency/lock changes. |
| Frontend unit suite | 64 passed, 0 failed. |
| Actual compiled-WASM suite | 72 passed, 0 failed/skipped. Includes native/WASM layout and PDF parity. |
| Build/bootstrap/routing suite | 4 passed, 0 failed/skipped. |
| Complete Chromium run, 2 workers | 207 passed, 1 failed. All 11 table cases passed. |
| Corrected final reload/save regression | 1 passed after replacing the transient-label assertion with exact save/close/reopen verification. |

The final `npm run build` reproduced all 59 browser-tested production assets
byte for byte. WASM SHA-256:
`f4457069ea2bfd67dde9dc103d795c8d45eb1fd2184f5eaa4c0a41b3b853e08d`.

The final complete Chromium failure expected an `Unsaved` label after exports
and a real beforeunload prompt had outlived the 750ms autosave delay. Exact
in-memory content already matched; autosave had truthfully committed. The test
now additionally saves, closes, reopens and verifies the exact native file.
That corrected test passed in a focused rerun. The complete 208-case suite was
**not rerun after this test-only correction**; do not describe it as a single
208-pass suite. Focused reruns overlap full-suite coverage and are not additional
suite passes. Browser tests
use actual Chromium and the production WASM bundle; touch and composition events
are simulated, not physical iOS/Android or OS IME verification. Firefox and WebKit
install attempts failed with HTTP 403 `Domain forbidden` from their official
download hosts. Hosted CI and Windows/macOS have not been executed for this
unpublished branch. No Microsoft Word runtime or accessibility certification.

Screenshots from the running application:
[desktop](images/graphe-tables-desktop.png), [mobile](images/graphe-tables-mobile.png).
