# Office Engine

Office Engine is an independent Rust engine intended to power a professional
productivity suite, with correctness, performance, compatibility, maintainability,
and long-term architectural stability as priorities. The goal is to compete with
Microsoft Office, LibreOffice, and ONLYOFFICE through incremental development.

Writer is the first application. Word processing and Microsoft Word DOCX
compatibility come first; spreadsheets and presentations will follow later.

## Current status

Phase 2's [mobile text selection](docs/PHASE2_MOBILE_SELECTION.md) adds Rust word
selection, draggable endpoints, scrolling/zoom handling and guarded clipboard
controls on a separate development branch. Browser automation passes; physical
iOS/Android/PWA validation remains outstanding. This is not a release approval.

Phase 2's [external compatibility corrections](docs/PHASE2_COMPATIBILITY.md)
classify validated DOCX/ODT/RTF formatting separately from unsupported content.
Exact originals and native identity remain preserved; known approximations are
disclosed on import and external copies. This is a bounded profile improvement,
not a claim that all documents from those applications are editable.

PR #17's [stability and file-compatibility audit](docs/WRITER_STABILITY_AUDIT.md)
records executed format/save workflows, corrective regressions and outstanding
mobile, external-producer and hosted-CI limits. It is not a general release or
physical-device certification; the audit branch remains unmerged.

Steps 1 and 2 establish the Cargo workspace and a structured document model in
`office-core`. Documents contain sections, paragraph blocks, UTF-8
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

Step 4 adds the provisional `.writer` native file format, filesystem-independent
byte serialization, and safe native filesystem saving and loading. It preserves
the complete supported document model across sessions. Step 4 was merged into
`main` at `52ac4e9` through [PR #6](https://github.com/boleydavid7-gif/stuff/pull/6).

Step 4.5 implements a browser Writer prototype:
React and TypeScript present a Rust/WebAssembly session, with explicit local
save/open and native `.writer` import/export. It was merged through
[PR #7](https://github.com/boleydavid7-gif/stuff/pull/7) at `0a5d806`; local and
post-merge GitHub Actions checks passed. Cloudflare deployment is not verified.
Rust remains the document, editing,
validation, and undo authority. The interface holds presentation and browser
interaction state rather than a second document engine.

See the [verification report](docs/VERIFICATION.md) for exact results, limitations,
and [desktop](docs/images/writer-desktop.png) / [mobile](docs/images/writer-mobile.png)
screenshots of the running application.
The [pre-merge review](docs/PRE_MERGE_REVIEW.md) supersedes the initial counts
and lists every completion criterion, corrections, and untested scope.

Step 5 semantic conversion was merged through
[PR #10](https://github.com/boleydavid7-gif/stuff/pull/10) at `8ab784a`. It extends
the 5A package foundation with a declared editable DOCX profile, Rust export and
browser file integration. Supported imports retain their exact original in native Writer
format 2 before editing is activated. Unsupported content remains read-only with
diagnostics; no partial editable projection is published. The shared model stays
format-neutral. Step 5 itself did not add other formats.
See [the DOCX profile](docs/DOCX.md), [conversion contract](docs/DOCX_CONVERSION.md)
and [conversion verification](docs/DOCX_CONVERSION_VERIFICATION.md).

Step 6 was merged through [PR #11](https://github.com/boleydavid7-gif/stuff/pull/11)
at `86f2ad8`, with post-merge CI passing. The independent
`office-layout` crate computes typography and page layout in Rust using a pinned
font catalog. A read-only browser page preview draws Rust-generated glyph paths
and geometry. The existing semantic editor remains available; its CSS wrapping
does not define engine pagination. See [the layout contract](docs/LAYOUT.md),
[verification](docs/LAYOUT_VERIFICATION.md) and
[actual page preview](docs/images/writer-layout-desktop.png).

Step 7 was merged through [PR #12](https://github.com/boleydavid7-gif/stuff/pull/12)
at `dab4db7`. It adds Rust-driven paged editing, the approved
Lamplight desktop/mobile interface, shared manual/autosave coordination, Find
and assignment of existing paragraph styles. The core document schema, native
format, storage database and existing engines are reused. See
[the Step 7 contract](docs/WRITER_STEP7.md),
[verification](docs/WRITER_STEP7_VERIFICATION.md) and
[approved references](docs/design/README.md).

Step 7.5 was merged through PR #13 at `3f447c4` with the owner's approval:
DOCX/ODT/RTF/TXT open, edit and save within declared profiles; PDF export only;
and full native `.writer` retention. Open and Save As reuse the Rust adapters,
preserve imported originals and show brief warnings for meaningful conversion loss.
Unsupported rich content remains read-only rather than silently becoming a
partial editable document. See [file formats](docs/FILE_FORMATS.md),
[verification](docs/FILE_FORMATS_VERIFICATION.md) and
[dependency licensing](docs/DEPENDENCIES.md). The planned comprehensive technical,
security, DOCX and commercial-licensing audit remains pending; Step 8 is not begun.

The save/export/mobile reliability correction is documented in
[the workflow verification report](docs/WRITER_RELIABILITY.md). It repairs local
storage recovery, replacement-session Save, modal shortcuts and mobile input
focus. Prepared exports provide explicit download/open links while preserving
the local save status. Physical iOS keyboard and installed-app behavior still
require device verification; mobile Chromium checks do not establish that.

The focused [document-menu correction](docs/WRITER_MENU_RELIABILITY.md) traces
every existing three-dot command and fixes swallowed activation, clipped options
and dialog return focus. It preserves the same Rust, format and local-save APIs.

[Compact Save As](docs/SAVE_COPY.md) uses filename, file format and Save copy,
with no ordinary-save checkbox. DOCX copies now preserve validated source
properties and safe auxiliary parts around regenerated current content. Originals,
native saves and editing history remain unchanged by exporting a copy.

The project does not use an existing complete Office or word-processing engine.
Serde and serde_json support model serialization. The low-level
unicode-segmentation library supplies Unicode grapheme boundaries. Native storage
uses the low-level tempfile library for unique temporary files, cleanup, and
portable replacement. Additional libraries may be introduced when needed.
Typography uses the low-level Parley/HarfRust/Skrifa stack. Bundled fonts have
their own OFL notices and provenance; they do not decide the project's license.

## Technology

- **Rust stable, edition 2024** provides the independent core engine.
- **Cargo** manages the workspace and shared dependency versions through
  `[workspace.dependencies]`.
- **WebAssembly** hosts the browser session through a separate `office-wasm`
  crate; browser bindings remain outside `office-core`.
- **React, TypeScript, and Vite** provide the prototype in `apps/writer`.
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

## Browser prototype development

Use Node.js 24.x, npm, stable Rust, the WebAssembly target, and a `wasm-bindgen`
CLI matching the locked binding dependency:

```sh
rustup target add wasm32-unknown-unknown --toolchain stable
cargo install wasm-bindgen-cli --version 0.2.130 --locked
cd apps/writer
npm ci
npm run build:wasm
npm run dev
```

`build:wasm` builds `office-wasm` for `wasm32-unknown-unknown` in release mode and
generates its web bindings. It rejects a mismatched CLI version. The Vite client
loads the generated WASM asset explicitly; `dev` assumes the bindings have
already been built. Use a secure browser context, such as localhost or HTTPS,
for the UUID namespaces used by new documents and safe local saves.

Run the frontend checks from `apps/writer`:

```sh
npm run typecheck
npm run lint
npm test
npm run test:wasm
npm run test:build
npm run build
npx playwright install --with-deps chromium firefox webkit
npm run test:browser
```

The production `build` includes the WASM build, TypeScript checking, and Vite
bundling. After `build:wasm` or `build`, `test:wasm` executes the real generated
bindings and release WASM in Node/V8 without an engine mock. `test:browser`
starts the built application through Vite preview. Chromium runs the full suite;
Firefox and WebKit run the portable editing/persistence cases and the new
file-format Open/Save As suite.
For Chromium-only development use `npm run test:browser -- --project=chromium`.
Chromium can use `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH` or an installed
`/usr/bin/chromium`; otherwise install Playwright's Chromium. To
inspect production output manually, run:

```sh
npm run preview -- --port 4173 --strictPort
```

After a release build, `node scripts/measure-layout.mjs` measures actual WASM
layout/serialization and cache-copy costs on bounded generated documents. It is
a development measurement, not a hardware-dependent CI speed threshold or a
typing/paint latency test. Layout output limits are documented in
[LAYOUT.md](docs/LAYOUT.md).

The build output is `apps/writer/dist`. Producing or previewing it does not publish
a site or establish private-hosting access controls. CI defines engine checks on
Linux, Windows, and macOS plus frontend checks and production-browser integration.
The milestone report must distinguish configured checks from actual results.

## Using Writer

The prototype supports typing and deletion at Rust grapheme boundaries, directed
selections, Enter, plain-text multiline paste, title changes, bold, italic,
underline, font family, font size, paragraph alignment, and Rust undo/redo. The
formatting controls reflect mixed selections. Collapsed character formatting
sets Rust-owned typing properties rather than inserting an empty formatting run.
The default editor renders Rust-generated glyphs, caret/selection rectangles and
continuous pages. Rust also owns pointer hit testing and visual navigation;
browser text measurement does not decide pagination. Review offers a read-only
page preview and literal, case-sensitive Find. Existing named styles can be
assigned without introducing a style editor. Ctrl/Command shortcuts support
clipboard, select all, bold/italic/underline, undo/redo, Save, Save As, Open and Find.
Enter creates a paragraph; Shift+Enter creates a soft break. Zoom changes view
scale rather than document geometry.

The finite font/layout profile can reject tabs or show missing-glyph notices.
An explicit **Text view** reuses the semantic editor when required and reports
pages unavailable; it does not substitute browser pagination. Font fallback and
other layout limits are documented in [LAYOUT.md](docs/LAYOUT.md) and Step 7.

New, Open, Save, Close, and native import/export are explicit actions. Autosave
uses 750 ms inactivity with a five-second maximum wait during foreground typing.
Untouched new blank documents are not automatically stored. Manual Save and
autosave share one serialized lane; errors pause retries until explicit Save and
retain unsaved in-memory work. Active composition pauses saving and is never
labeled saved. Save writes
the exact `.writer` bytes produced by Rust to IndexedDB on this browser/device.
Each record has an etag: comparison with the expected token and replacement share
one read/write transaction, preventing a stale tab from silently overwriting a
newer local save. Completion means the transaction completed, not just that a put
request succeeded. An asynchronous save clears dirty state only if its captured
document revision is still current.

Opening or importing is guarded against unsaved replacement and against edits
made while the asynchronous read is pending. Rust validates the native bytes
before replacing the active session; local records also require matching document
identity. Failures retain the live document and unsaved state. A native imported
document is not automatically associated with an existing saved record. Save conflicts
offer a separate copy with a new document identity. Export downloads native bytes
without replacing the local save or clearing the need to save.
Damaged library records are reported individually while healthy documents remain
openable; no damaged record is silently repaired or deleted. Save failures are
announced inside the unsaved-changes dialog when it is open.

Open also accepts DOCX, ODT, RTF and Unicode TXT. Rust checks each declared profile in a
detached candidate. A supported import is activated only after IndexedDB has
saved its full native file, including the unchanged original. Storage failure
or a changed originating session preserves the previous live work. Unsupported
rich content opens a diagnostic dialog with the original available for download;
it is not saved in the local library or made editable.

**Export Word** regenerates current supported content. **Download original**
returns the exact imported file. A native `.writer` export retains that original,
which can contain text or metadata removed by later edits; the interface discloses
this before exporting the source-bearing file. Native save/reopen preserves the
original independently of undo/redo.

**Save As** selects Writer, DOCX, ODT, RTF, TXT or export-only PDF. Rust reports
format limits for the exact document revision; the dialog requires acknowledgement
before converting a loss-prone copy. TXT accepts strict UTF-8 or BOM-marked UTF-16
and exports UTF-8/LF, with explicit rich-formatting loss. PDF uses Rust page/glyph
geometry and currently writes vector outlines: it is not searchable, selectable
or PDF/UA tagged. File downloads do not replace the native local save. **Format
library notices** in the document menu serves the new dependency license texts.

The browser adapter applies these prototype resource limits:

| Resource | Limit |
| --- | --- |
| Browser file input/output, including native source/base64 overhead | 5 MiB |
| Paragraph text | 200,000 UTF-16 code units in total |
| Paragraphs and sections | 2,000 each |
| Text runs | 20,000 |
| Named styles and inheritance depth | 128 styles; depth 32 |
| Retained undo snapshots | 20 |

These limits do not change the native format or core defaults. Documents,
presentation snapshots, validation, and history involve complete-document work;
the history limit counts snapshots rather than bytes. Storage quota, eviction,
private-mode restrictions, unavailable storage, damaged records, stale saves,
and invalid input produce actionable errors. Export important documents for
backup; browser-local storage is not a durability guarantee.

Paged editing retains a focusable, visually clipped controlled `contenteditable`
input projection. During IME composition its DOM is a transient buffer; Rust commits the final
replacement once. Synthetic browser tests do not certify every operating-system
IME, candidate window, dictation system, or assistive technology. Keyboard and
semantic checks do not constitute accessibility certification. Touch dragging
handles, word-level navigation, native OS IME/assistive certification and advanced
Word layout remain limited. There is no cloud service or synchronization. DOCX conversion
supports the bounded profile documented below; it does not render every Word feature.

The engineering rules in [AGENTS.md](AGENTS.md), primary-source research and
retrospective findings in [RESEARCH.md](docs/RESEARCH.md), and accepted
[decisions](docs/DECISIONS.md) govern this milestone and future work. The authorized
Step 5 scope and preservation policy are recorded in [DOCX.md](docs/DOCX.md).

`apps/writer/cloudflare.config.ts.example` is inert preparation for a separate
assets-only Worker. It attaches no routes or domains, disables workers.dev and
preview URLs, and changes no live access restrictions. `writer.proairetos.com`
is the permanent intended public address; live routing and private-access
protection remain independently unverified. Generating a build or committing
this example does not authorize deployment.

Cloudflare Workers Builds does not guarantee the Rust/WASM tools required by the
ordinary build; the supplied environment lacked wasm-bindgen. Use the researched,
build-only `npm run build:cloudflare`
bootstrap from `apps/writer` after the correction is available in the selected
branch. See [Cloudflare build setup](docs/CLOUDFLARE_BUILD.md) for verified tools,
cold-build testing, and the unresolved remote private-access/permission boundary.

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
demonstrates legacy schema 1 and is migrated explicitly into current schema 2.

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

Session revisions are runtime state, separate from document schema versions.
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

Collapsed insertion can extend an immediately preceding nonempty run with exactly
equal direct properties. Existing adjacent runs are not normalized or merged;
empty/right partitions and noncollapsed replacements retain their semantics.
This keeps ordinary typing from consuming one run per character without changing
the document schema or transaction/undo boundary.

`DocumentSession::new` retains up to `DEFAULT_HISTORY_LIMIT` (100) complete undo
snapshots. `with_history_limit` chooses another count, including zero to disable
history. New changed commits clear redo; no-ops and rejected commits preserve it.
Undo and redo restore exact model values under fresh increasing revisions, so
old revision tokens remain stale. `clear_history` preserves document and revision.
History is bounded by snapshot count rather than bytes and is not serialized;
there is no automatic typing coalescing or grouping beyond a transaction.

## Native document persistence

`.writer` is a provisional internal UTF-8 JSON format whose specification may
evolve before public release. The current envelope identifies `office-writer`,
sets `format_version` to `2`, and contains a schema-2 document plus an optional
original-source record. Model schema, native format, and engine versions are
independent. Legacy native format 1/schema 1 is read through a strict migration;
new fields are not accepted under the older version labels.

The byte API has no filesystem dependency:

```rust
use office_core::{Document, Id, deserialize_document, serialize_document};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = Document::new(Id::new("document-1")?)?;
    let bytes = serialize_document(&document)?;
    let restored = deserialize_document(&bytes)?;
    assert_eq!(restored, document);
    assert_eq!(serialize_document(&restored)?, bytes);
    Ok(())
}
```

Native filesystem targets also expose `save_document` and `load_document`:

```rust
#[cfg(not(target_arch = "wasm32"))]
fn save_and_reopen(
    document: &office_core::Document,
    path: &std::path::Path,
) -> Result<office_core::Document, office_core::StorageError> {
    office_core::save_document(document, path)?;
    office_core::load_document(path)
}
```

Use `WriterFile`, `serialize_writer_file` and `deserialize_writer_file` for a
document with an optional `OriginalSource`; native targets also expose
`save_writer_file` and `load_writer_file`. Original bytes are immutable and shared
through `Arc`, outside the model and undo snapshots. Document-only loaders return
`PersistenceError::SourcePresent` for source-bearing files instead of losing the
original. Core serialization does not interpret an original's foreign format;
the corresponding adapter validates its contents before editable use.

The core file budget is 32 MiB, with at most 16 MiB of original bytes, strict
standard padded base64, and bounded source identification. Browser limits are
smaller: the complete file, including retained source encoding, must fit 5 MiB.

Saving validates and serializes completely before creating a unique temporary
file in the destination directory. It writes, flushes, and synchronizes that file,
then replaces the destination without deleting the old document first. On Unix,
it also synchronizes the parent directory after replacement. Windows has no
equivalent directory-sync step here. Operating-system and filesystem behavior
limit atomicity and crash durability; no universal guarantee is made.

`PersistenceError` describes encoding, version, JSON, and model failures.
`StorageError` adds native paths, I/O stages, replacement state, and any secondary
temporary-cleanup error. In particular, `ReplacementState::Completed` means the
new file was installed before a later synchronization failed; `Uncertain` means
replacement returned an error with potentially ambiguous filesystem results.

The APIs preserve native paths, including Unicode filenames. They do not append
an extension or create parent directories. Saving rejects symlink, non-regular,
and read-only destinations. Ordinary permissions are copied after writing and
flushing, before synchronization. Ownership, ACLs, extended attributes, timestamps,
and hard-link identity are not preserved.
Loading checks file type before opening and checks the opened handle again.
If explicit temporary cleanup fails, its error and path are retained; the file
may remain without an automatic retry.
There is no concurrent-writer conflict detection, and files are processed fully
in memory. Undo history, revisions, and editing endpoints are not persisted;
opening the returned document in a new session starts a fresh lineage and history.

See [architecture](docs/ARCHITECTURE.md#native-writer-persistence-step-4) for the
format specification, safe-save limits, and migration policy, and the
[native sample](tests/fixtures/native/writer-v1.writer) for a realistic document.
The browser prototype uses the full-file byte API with IndexedDB. Native
filesystem replacement guarantees do not transfer to browser storage. Autosave
is not implemented.

## DOCX conversion

`office_docx::import_docx(bytes, document_id)` returns either an `Editable`
`ImportedDocx` containing the complete supported model and original, or
`Preserved` containing the intact source and structured compatibility report.
`export_docx(&Document)` regenerates the supported DOCX profile in Rust.

The profile covers paragraphs, runs, supported paragraph styles/defaults and
inheritance, direct formatting, spacing/indentation, section page geometry,
page-break-before, soft line breaks and tabs. Tables, images, fields, links,
numbering, revisions, headers/footers and unknown semantic properties remain
read-only. Known theme/script and other unsupported visual formatting may use
documented approximations with an import notice and exact original retention.
Ambiguous text whitespace or
literal CR/LF/TAB in Word text elements is reported rather than silently trimmed
or approximated. See [DOCX.md](docs/DOCX.md) for precise restrictions.

Ordinary auxiliary relationships no longer make an otherwise supported document
read-only: inspected inert metadata, unused themes, font descriptors, cached
thumbnails and view/session preferences are retained in its exact original.
Active unsupported formatting/settings still prevent editing. Save As warns
that a newly generated DOCX omits unmodeled original metadata and auxiliary parts.
See [relationship policy and regressions](docs/DOCX_RELATIONSHIPS.md).

[Formatting normalization](docs/DOCX_NORMALIZATION.md) allows ordinary Word and
LibreOffice styles/settings to open for editing with visible adjustment notices.
Content, protection and structure safeguards stay strict. Native save/reopen
preserves the edited model and exact original; exported copies use the normalized
current formatting and disclose source-only information that they omit.

DOCX semantic round trips do not promise arbitrary native ID preservation, exact
run partitions or identical future Word style-editing behavior. Originals remain
recoverable; exported DOCX represents current supported content. LibreOffice
reference checks cover the tested subset, and Microsoft Word interoperability
remains untested. Exact commands and results belong in the
[conversion verification report](docs/DOCX_CONVERSION_VERIFICATION.md).

## Repository

- `crates/office-core/` contains the interface-independent engine and persistence.
- `crates/office-wasm/` owns the browser session, command transport, Unicode
  selection affinity, and effective presentation values.
- `crates/office-docx/` owns DOCX package inspection, declared semantic conversion,
  compatibility diagnostics and exact original-source retention.
- `apps/writer/` contains the React/TypeScript prototype, build tooling, and
  frontend/browser tests.
- `docs/ARCHITECTURE.md` describes long-term boundaries and principles.
- `docs/ROADMAP.md` records the preliminary development sequence.
- `docs/DECISIONS.md` explains the initial technology and scope decisions.
- `AGENTS.md` and `docs/RESEARCH.md` record engineering rules and research.
- `tests/fixtures/` contains model and native Writer samples plus official Unicode
  grapheme-break conformance data.
- `.github/workflows/ci.yml` defines the required validation workflow.

Additional crates will be introduced only as their responsibilities become
necessary.

## Licensing

The licensing decision is pending. The project owner has not yet chosen whether
the engine will be open-source, source-available, or proprietary. No software
license has been selected, and the `LICENSE` file is intentionally absent until
that decision is made.
