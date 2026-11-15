# Compact Save As and DOCX copy preservation

Branch: `fix/writer-simple-save-copy`, based on main `b3e879e`.
The owner authorized merging this focused task after verification; deployment
commands and new document features are outside its scope.

## User workflow

Save As shows Filename, File format, Cancel and **Save copy**. Ordinary supported
copies have no technical warning panel or checkbox. A brief inline warning appears
only for meaningful conversion loss; Save copy confirms that currently displayed
assessment. Native copies retaining an original show a short privacy hint, because
the original may include removed text. Neither warning nor hint changes the file.

Mobile fields stack and use 16px controls. The existing native dialog follows
VisualViewport height/pan so the save button remains reachable with a keyboard.
Enter submits the form locally; native dialog cleanup and return focus remain.
Export Word may download directly when no meaningful warning is needed.

All assessment/conversion logic remains Rust-owned and revision-bound. Copies
do not rename/edit the live document, replace source bytes, alter undo/redo,
mark unsaved work saved or update the local record/etag. File delivery still uses
the prepared-copy native links and the browser's destination/duplicate-name rules.

## Meaningful-loss policy

| Format or source feature | Warning policy |
| --- | --- |
| Writer, ordinary DOCX/ODT/RTF | No warning for IDs, run partitions, unused property distinctions or authoring caches. |
| TXT | Warn only for effective used formatting, geometry, sections or soft breaks that will be lost. Explicit default values and unused styles do not cause warnings. |
| PDF | Briefly disclose nonsearchable/nonselectable/untagged outlined text and actual missing-glyph/font/layout risks. Repeated diagnostics are grouped. |
| Imported DOCX formatting/bookmarks | Warn about proven original formatting approximations or named bookmarks that cannot be retained. Exact `_GoBack` cursor caches are excluded. |
| DOCX properties converted to another format | Warn when proven descriptive/custom properties will not be included; no blanket source-metadata warning. |
| Unsafe-to-copy theme | DOCX copy omits that optional part with a specific brief warning. Other safe format exports still work. |

Backend loss confirmation remains required when actual warnings exist. Clicking
Save copy supplies it for the displayed current assessment. A stale session,
document or revision still fails without producing bytes. The public native and
document-only low-level conversion APIs retain their documented distinctions.

## Why DOCX metadata was omitted

The browser previously passed only the semantic `Document` into conversion. That
exporter regenerated main content, styles, title-only core properties and package
control parts. It had no original package from which to preserve auxiliary data.
The full immutable `WriterFile` now reaches `assess_file` / `export_file`, which
dispatch source-aware DOCX behavior inside `office-docx`; the shared model remains
independent of OOXML and future format adapters.

The DOCX copy profile:

- Regenerates current body/styles and title from the supported model. Never copies
  old body/style XML or restores removed paragraphs, revisions, fields or objects.
- Preserves recognized descriptive/core properties including creator/created,
  typed custom properties and static extended properties. Replaces the title with
  the current title; drops stale modified/revision/last-printed/last-editor fields.
- Carries safe theme/font descriptors and settings/web settings only within the
  validated inert profile. Normalized active settings are not reattached.
- Excludes thumbnails, heading/title/count/hyperlink caches, comments and processing
  instructions. Exact original recovery remains available in native files.
- Rebuilds relationship IDs, canonical targets and content types rather than
  trusting original names or grafting opaque relationship graphs.
- Normalizes exact known Strict namespace declarations to the existing Transitional
  export profile; literal user property values containing namespace URIs are unchanged.
- Omits unknown/relationship-bearing theme payload with a meaningful warning.
  Malformed packages/XML and unsupported body/relationship graphs remain errors.
- Enforces existing ZIP/XML/output budgets, reinspects the package and requires the
  resulting supported document to equal the regenerated baseline projection.

`export_docx_with_source` and `assess_docx_copy` are adapter APIs. A runtime
`CompatibilityReport.has_formatting_normalizations` flag is recorded before
bounded notice truncation, preserving correct risk classification when galleries
are large. No core/native schema, database, undo format or dependency changed.
Existing document-only `export_docx` / `export_document` cannot preserve source
information they are not given; callers needing preservation must use full-file APIs.

## Verification

Production assets are exercised in actual Chromium desktop/touch contexts. Tests
inspect exported ZIP metadata/relationships using independent Node ZIP decoding,
and check unchanged native bytes, originals, record etags, selection and history.
Native adapter cases include upstream Microsoft Word-producer fixtures, the genuine
LibreOffice fixture and the documented synthetic Word-web normalization profile.
Neither a current Microsoft Word nor LibreOffice application is executed here;
fixture provenance remains in `tests/fixtures/docx/relationships/README.md`.

The previously supplied DOCX also passed actual UI import/edit/save/reload/reopen,
all six exports and reopening all five importable formats. Native output retained
the exact original. Independent Python ZIP/XML checks compared its retained core
property values and the copied current title without printing private data.
Private source/generated files remain in `/tmp`, outside Git.

New regressions cover ordinary one-click copies across all editable formats,
brief real-loss warnings, no checkbox, mobile bounds/16px fields, Enter submission,
keyboard pan/resize, original/source immutability, Strict metadata mapping,
malformed/unsupported sources, unsafe themes, cache/comment exclusion, semantic
projection equality and formatting notices after style-gallery budget overflow.

| Directory | Exact command | Result |
| --- | --- | --- |
| Repository root | `cargo fmt --all -- --check` | Passed |
| Repository root | `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| Repository root | `cargo test --workspace` | Passed, 608 tests including doctests |
| Repository root | `cargo build --workspace` | Passed |
| Repository root | `cargo check --locked --workspace --target wasm32-unknown-unknown` | Passed |
| `apps/writer` | `npm run typecheck` | Passed |
| `apps/writer` | `npm run lint` | Passed |
| `apps/writer` | `npm test` | Passed, 44 tests |
| `apps/writer` | `npm run test:build` | Passed, 2 tests |
| `apps/writer` | `npm run test:wasm` | Passed, 53 compiled WASM tests |
| `apps/writer` | `npm run build` | Passed, locked Rust/WASM, TypeScript and Vite production build |
| `apps/writer` | `npm run test:browser -- --project=chromium --output=/tmp/writer-save-copy-browser-results --reporter=list` | Passed, 136 tests including 8 new compact Save As cases |
| Repository root | `node /tmp/writer-save-copy-private.mjs` | Passed, supplied-document UI workflows described above |

Actual screenshots: [desktop DOCX](images/writer-save-as-docx-desktop.png),
[mobile DOCX](images/writer-save-as-docx-mobile.png),
[desktop native](images/writer-save-as-desktop.png) and
[mobile native](images/writer-save-as-mobile.png). Mobile captures are Chromium
touch contexts, not physical iOS photographs. An existing menu-geometry test was
also corrected to wait for its queued native `toggle` event before sampling;
the same strict viewport bounds remain asserted.

## Limits

- No real iPhone/iPad, iOS Safari or installed Home Screen environment is available.
  Touch/focus and controlled viewport tests do not verify a physical keyboard,
  native selection handles or Files delivery.
- `PLAYWRIGHT_BROWSERS_PATH=/workspace/.writer-test-browsers npx playwright install webkit`
  failed with CDN HTTP 403 `Domain forbidden`. WebKit/Safari tests could not run;
  no Safari pass is claimed. Compatibility projects include the new cases.
- Hosted CI is reported separately; prior GitHub Actions runs failed before
  allocating runners. Local passes do not establish hosted/macOS/Windows results.
- Preservation is bounded to supported source roles and validated payloads, not
  arbitrary OOXML or full XSD validation. Some formatting and bookmarks still
  require a brief warning; unrepresented tables/images/fields remain read-only.
- The retained original is immutable, so original-formatting notices can remain
  conservative after substantial editing. Native includes complete source bytes;
  external DOCX includes selected properties, not the entire original archive.
- PDF outline/accessibility, font availability and local-storage/lifecycle limits
  remain as documented. No deployment or live-origin verification is implied.
