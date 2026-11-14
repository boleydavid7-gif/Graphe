# Step 7.5 — file-format verification

This milestone extends merged Step 7 (`dab4db7`) on `dev/writer-file-formats`.
The owner authorized merging after successful verification. No manual deployment,
Cloudflare setting change, additional format or comprehensive audit is included.

## Delivered behavior

| Format | Verified workflow | Limits |
| --- | --- | --- |
| `.writer` | Open, Rust edits, native local save, Save As and exact reopen | Full model/source retained; history remains runtime-only |
| DOCX | Existing adapter reused through Open/Save As, edits/formatting, native recovery and export/reimport | Existing declared Word profile; unsupported semantics read-only |
| ODT | Paragraphs/runs/styles, formatting and geometry mapped into Rust, edits, native recovery, export/reimport | Declared ODF 1.2–1.4 core profile, deterministic 1.3 output |
| RTF | Unicode runs, paragraph styles/geometry, edits, native recovery, export/reimport | Bounded RTF 1 profile, CP1252/Latin-1, unknown rich semantics read-only |
| TXT | UTF-8/BOM-marked UTF-16 open, edits, native recovery, UTF-8/LF export/reopen | Explicit rich-format loss; no guessed legacy encodings, NUL or UTF-32 |
| PDF | Rust multi-page geometry, colors, underlines and glyph forms exported and independently reopened/rendered | Export only; vector outlines, no search/selection/tagged PDF/UA/PDF-A claim |

Open uses detached candidates and commits the native document plus exact external
original to IndexedDB before activating editing. Save/automatic saving retain
the existing native etag lane. Save As publishes a copy only after origin-bound
Rust warnings and acknowledgement; it does not mark newer work saved or replace
the local association. Native original-content privacy disclosure remains active.
Unknown rich semantics retain the existing read-only preservation policy.

See [the format contract](FILE_FORMATS.md), [research](RESEARCH.md),
[decisions](DECISIONS.md), [dependency review](DEPENDENCIES.md) and
[fixture provenance/consumer commands](../tests/fixtures/formats/README.md).

## Final local commands and results

Environment: stable Rust 1.100.0, wasm-bindgen-cli 0.2.130, Node 24.22.1,
npm 11.9.0 and system Chromium 155 on Linux. The clean baseline passed 442
Rust tests and all required Rust checks before implementation.

Commands below run from the repository root unless marked `apps/writer`.
Local Cargo variables select the installed workspace toolchain:

```sh
export CARGO_HOME=/workspace/.office-rust/cargo
export RUSTUP_HOME=/workspace/.office-rust/rustup
export PATH=/workspace/.office-rust/cargo/bin:$PATH
```

| Exact command | Executed result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed after applying `cargo fmt --all` |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace` | **518 passed**, zero failed/ignored |
| `cargo build --workspace` | Passed |
| `cargo check --locked --workspace --target wasm32-unknown-unknown` | Passed |
| `node apps/writer/tests/layout-check-native.mjs` | Passed; complete layout/outline golden unchanged |
| `npm run build` (`apps/writer`) | Passed release WASM, TypeScript and production Vite build |
| `npm run typecheck` (`apps/writer`) | Passed |
| `npm run lint` (`apps/writer`) | Passed with warnings forbidden |
| `npm test` (`apps/writer`) | **42 passed**, including existing autosave/storage regressions |
| `npm run test:build` (`apps/writer`) | **2 passed**, zero skipped |
| `npm audit --audit-level=moderate` (`apps/writer`) | Registry reported zero vulnerabilities; not a Rust/security audit |
| `npm run test:wasm` (`apps/writer`) | **42 passed**, zero failed/skipped; includes native/WASM PDF byte equality |
| `npm run test:browser -- --project=chromium` (`apps/writer`) | **76 passed** in 2.9 minutes, zero failed/skipped |

The final bundle identifies `index-8-SqcOIQ.js`,
`office_wasm_bg-C3J_F_cW.wasm`, the existing font notice and a new shipped
48,519-byte format-library notice. Generated assets are not committed source.
Complete native geometry/outline hash remains
`7741b146df0ad6128c653190af74ccad0daa8efad249200a82d7c0b9c03b0236`.

The nine new compiled-WASM cases exercise all four external editable formats,
native exact copies, UTF-16/UTF-8/BOM Unicode, source bytes, malformed/unsupported
imports, stale/acknowledgement guards and PDF-only behavior. The PDF case compares
the complete four-page WASM output byte-for-byte with the actual native exporter.
Eleven new browser cases use the real Open file chooser, Rust edits/formatting,
Save As downloads, fresh browser contexts, IndexedDB/reload/reopen, unsupported
content, corrupted input, PDF, Unicode filenames, notices and focus continuation.
Existing 65 Chromium cases remain in the suite. No editor/layout mock substitutes
for the engine or production bundle.

## Cross-browser and CI boundaries

Firefox/WebKit binaries are absent locally; the previously attempted official
downloads returned HTTP 403. Those local cases **could not run**. CI installs all
three browser engines. Chromium runs 76 cases; Firefox and WebKit each run the
two portable Step 7 smoke cases plus all eleven Step 7.5 cases: 102 configured
browser cases in total. Final-commit CI results belong to the PR/check links;
configuration alone is not a pass. Native CI additionally checks Windows/macOS
and the full shared geometry/outline golden.

No claim is made for manual OS IME, screen readers, real mobile hardware,
Microsoft Word application testing or every feature/producer variant of these
formats. Synthetic input and independent LibreOffice checks establish only the
reported coverage. The planned comprehensive security/commercial audit remains
pending.

## Critical corrections with regressions

- The new document-only format facade could naturally discard a retained original
  on Writer export. It now refuses that path; source-aware `export_file` preserves
  the full native file. Browser Writer export already used the safe full API.
- TXT export now refuses embedded NUL before review/output, normalizes CR/CRLF
  across run boundaries, and protects a semantic leading U+FEFF with an additional
  transport BOM. Export/reopen cannot silently consume that Unicode character.
- ODT titles concatenate all immediate text segments across comments/processing
  instructions/CDATA. Metadata TAB/LF/CR uses XML numeric references so attribute
  and line-ending normalization cannot alter titles/style names. Both title
  regressions were observed failing before their targeted corrections.
- External editable output is checked against its own bounded import profile
  before publication. Excessive labels, numeric values, generated structures or
  tokens fail explicitly instead of yielding a file Writer itself cannot reopen.
  This check establishes reopenability, not independent semantic fidelity.
- Save As Cancel/Escape/Download restores focus during the close commit. A real
  browser failure showed hidden closed-`details` menu content retaining nonzero
  rectangles, causing `focus()` to do nothing. Menu invokers now return to the
  visible summary; keyboard invocation permits immediate continued typing.
- Download filenames truncate by complete Unicode scalars, not UTF-16 slices.
  Open errors remain visible inside the file dialog. Failed import retains live
  work/history and cannot consume future identifiers or substitute source bytes.

An initial WASM attempt failed because the command omitted the installed Cargo
PATH needed by the native golden subprocess; corrected reruns actually passed.
Intermediate test-crate documentation/import errors and the real focus failures
were corrected before final verification. They are not counted as passing runs.

## Independent format-consumer evidence

The checked-in `export_format` example generated all six formats from
`project-notes.writer`; native output preserves full `WriterFile` source state.
LibreOfficeDev 26.8.0.0.alpha0 opened the exported DOCX/ODT/RTF/TXT and recovered
all 405 native Unicode scalars, adding one terminal LF. No combining-mark or
emoji normalization was used. Flat-ODF properties and independent PyMuPDF 1.26.6
inspection of LibreOffice PDFs verified the 20-point bold serif heading,
12-point ordinary/bold text and `#24211d` on this fixture. Installed fonts were
substituted by LibreOffice; this is not identical font metrics/pagination proof.

Poppler 26.05.0 accepted the engine's PDF, rasterized the page(s) and showed the
expected typography. pypdf strictly reopened the four-page fixture and checked
page dimensions/forms, with less than 0.001-point serialization error. Outlined
PDF text extraction returned no searchable body, matching the warning. External
tools are validation consumers only; no Office/PDF engine is embedded or shipped.
Exact reproducible commands and fixture hashes are in the fixture README.

## Limits and handoff

Real LibreOffice-generated RTF/ODT fixtures contain unrepresented producer
defaults/extensions and remain read-only. Their exact originals and diagnostics
are preserved; do not describe them as editable support. Unsupported tables,
images, fields, lists, headers/footers and other absent model features remain
outside the profiles. ODT additionally rejects empty sections, binding gutters
and literal body CR; inert native header/footer offsets have explicit warnings.
Existing font, layout, browser-size/history and cross-section editing limits
remain unchanged. All import/export work is local; no cloud conversion or sync.

New direct libraries are permissively declared `pdf-writer`, `svgtypes` and
`rclip-rtf` (with `rclip-core`), with twelve new resolved packages. Full additional
notices ship in production. The rclip packages' lack of standalone license files
and low maturity are explicitly recorded despite their Apache/MIT declarations;
this is not commercial legal clearance. The project's license remains undecided.
No new npm dependencies or native/database schemas were introduced.

[Actual Save As screenshot](images/writer-file-formats.png) shows the running
application and its explicit loss warning. Step 8, the comprehensive audit and
manual deployment must not begin automatically after this milestone.
