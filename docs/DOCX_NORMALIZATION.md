# DOCX formatting normalization

The owner authorized known formatting approximations after the relationship
review in PR #14. Ordinary Word and LibreOffice documents must not become
read-only merely because Writer cannot reproduce every typography or layout
preference. This supersedes the earlier exact-formatting-only import profile;
it does not authorize dropping document content or bypassing protection.

## Evidence and policy

The supplied private document was produced by Microsoft Word for the web. Its
nine paragraphs and 17 runs contain no tables, images, fields or revision
content. The previous importer produced 82 blocking diagnostics for metadata,
unused styles, theme/script formatting and ordinary settings, including
explicitly disabled tracking. The original remains outside the repository.
Checked-in regressions use newly authored text and public, licensed structures.

Three unmodified public producer samples also exercise the profile: official
Microsoft SDK Word 15/16 fixtures and actual LibreOffice output. Their provenance
and hashes remain in [the fixture record](../tests/fixtures/docx/relationships/README.md).
Microsoft Word itself is unavailable in this environment; these samples are
not a claim of running the current Word application.

An editable result must retain every supported text/control and paragraph order,
validate the resulting native model, and keep its exact original. Known visual
formatting may become a documented approximation. Unknown semantics are not
automatically reclassified as formatting.

| Source construct | Deliberate behavior |
| --- | --- |
| Style gallery, authoring IDs and latent styles | Validate known structure; retain source metadata without blocking content editing. Active style locking stays guarded. |
| Paragraph styles and inheritance | Keep supported properties and stable references. Reject duplicate IDs, missing parents and cycles. Normalize display names deterministically when necessary. |
| Used character styles | Resolve their inheritance and materialize supported appearance into run properties, with paragraph/style toggles and direct-format precedence tested. |
| Unused character/table/numbering libraries | Preserve in the original. They cannot introduce current body content; unknown namespaces or nested body content still block. |
| Unused paragraph list styles | Keep them and unused descendants in the original. Default, referenced or inherited active numbering still blocks; generated labels cannot disappear. |
| Script/theme fonts and colors | Use an explicit concrete family or inherited Writer fallback; retain cached RGB where available. Warn about script/theme differences. This is not full theme resolution. |
| Kerning, language, ligatures and known visual treatments | Validate known values, preserve supported properties and disclose approximations. No new typography engine is introduced. |
| Paragraph/page preferences | Preserve supported spacing, indentation and geometry. Normalize known unsupported borders, wrapping, grids, columns and writing-flow preferences with notices. |
| Observed Word/LibreOffice settings | Normalize named layout/compatibility preferences and validate inert shape/math defaults. They do not enable actual shapes, equations or executable content. |
| Explicitly disabled tracking and section form-protection flags | Validate and accept the disabled state. Missing/true tracking and document/write protection remain guarded. |
| Bookmarks | Validate pairing and identifiers, preserve enclosed text and retain original anchors; regenerated copies omit unmodeled anchors with disclosure. |
| Unknown content or relationships, tables/images/fields/revisions, active protection, embedded fonts | Keep existing read-only/error safeguards. No partial editable content projection is published. |

Malformed values, duplicate properties or ambiguous structural records are not
silently repaired. Foreign namespaces cannot borrow a recognized local name to
obtain normalization. A known leaf cannot contain unknown attributes, nested
content or non-whitespace text. Bound `mc:Ignorable` declarations do not grant
permission to skip actual unknown markup. External targets are never fetched.

## API, persistence and user notice

`CompatibilityReport` keeps blockers and `normalizations` in separate bounded
collections, each with its own omitted count. More than 128 formatting notices
cannot conceal a later revision, protection or content blocker. `is_supported`
continues to require zero blocking findings. `ImportedDocx::report()` exposes
notices without changing the existing document/original `into_parts()` API.

The format facade adds `import_document_with_warnings`, returning the existing
outcome plus notices from the same parse. Its existing `import_document` API is
retained for compatibility. User-facing adapters should use the richer result.

The WASM session keeps notices beside its immutable source record, outside the
document model, undo history and native file schema. Native reopen recomputes
notices during the existing source validation; it never replaces the stored
edited document with a fresh import. New clears notices, edits/undo retain them,
and rejected imports leave all prior content, history and notices intact.

The browser activates a detached imported document only after its full native
file and exact original have been saved in the existing IndexedDB transaction.
An accessible **Formatting adjusted for editing** notice explains approximations
and offers bounded details. Save As now uses brief meaningful-loss warnings;
Save copy confirms the current assessment without an acknowledgment checkbox.
Source-aware DOCX preservation is described in [SAVE_COPY.md](SAVE_COPY.md).
Native save/reopen retains original bytes; regenerated external copies use the
current model and omit unmodeled original features. Original download is a
separate operation and may include text removed by editing.

No shared document types, file versions, dependencies, browser storage formats,
editing transactions or layout algorithms change. This is an adapter policy and
reporting correction, not an OOXML schema validator or a promise of Word-identical
layout. Actual complex content remains a separate compatibility responsibility.

## Research and review

- [Microsoft style contracts](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.style?view=openxml-3.0.1)
  and [latent styles](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.latentstyles?view=openxml-3.0.1)
  distinguish instantiated formatting, gallery metadata and locking behavior.
- [Run fonts](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.runfonts?view=openxml-3.0.1)
  and [color properties](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.color?view=openxml-3.0.1)
  establish script/theme precedence. Using a cached value or a Writer fallback is
  an approximation and must be disclosed.
- [Tracking revisions](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.trackrevisions?view=openxml-3.0.1)
  distinguishes disabled tracking from actual recorded revision content; turning
  tracking off does not authorize accepting/deleting existing changes.
- [Columns](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.columns?view=openxml-3.0.1),
  [document grids](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.docgrid?view=openxml-3.0.1)
  and [borders](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.border?view=openxml-3.0.1)
  describe formatting differences that do not themselves supply paragraph text.

Focused independent review checks character-style toggle parity, used/default
numbering closures, hidden/protected content, warning-budget separation and
atomic publication. It also checks importer resource amplification: duplicate
display names use cached suffix counters, and oversized inherited font names
must not multiply into unbounded allocations. Regression coverage accompanies
these corrections. The planned comprehensive audit remains pending.

The private browser check also exposed a toolbar summary defect: blank paragraphs
made a fully formatted text selection appear mixed. Character summaries now use
empty-paragraph defaults only when the selection contains no actual text;
collapsed-caret and paragraph alignment/style summaries retain their behavior.
This does not alter document content or formatting transactions.

## Verification

Executed on Linux on 2026-11-15. Repository-root commands unless otherwise shown:

| Command | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace` | 581 passed; zero failed/ignored |
| `cargo build --workspace` | Passed |
| `cargo check --locked --workspace --target wasm32-unknown-unknown` | Passed |
| `node apps/writer/tests/layout-check-native.mjs` | Passed; shared page/glyph fingerprint unchanged |
| `apps/writer`: `npm run typecheck` | Passed |
| `apps/writer`: `npm run lint` | Passed |
| `apps/writer`: `npm test` | 42 passed |
| `apps/writer`: `npm run test:build` | 2 passed |
| `apps/writer`: `npm run test:wasm` | 52 passed against final compiled release WASM; zero failures/skips |
| `apps/writer`: `npm run build` | Passed: Rust release WASM, TypeScript and production Vite assets |
| `apps/writer`: `npm run test:browser -- --project=chromium` | 79 passed against final production assets; zero failures/skips/retry markers |
| `apps/writer`: `npm run test:browser -- tests/docx-relationships.spec.ts --project=firefox --project=webkit --reporter=line --output=/tmp/writer-docx-normalization-other-browser-results` | Six launch failures: browser executables missing; no interaction assertions executed |

The final bundle contains `office_wasm_bg-h6oS-u55.wasm`. Native/WASM geometry and
PDF parity checks also passed. No dependency lockfile, native model/schema,
deployment configuration or approved design reference changed. The existing
screenshots remain unchanged; [the new screenshot](images/writer-docx-normalization.png)
shows the actual final application with a public synthetic document.

The private supplied file was additionally checked locally, outside the repository:

- A temporary Rust probe imported it, validated the model, preserved all nine
  paragraphs/17 runs, saved/reopened native bytes with the exact original and
  exported/reopened current DOCX. Every paragraph's text and order matched.
- `node /tmp/writer-docx-private-browser.mjs` passed against the final production
  preview: import notice, typing, bold, undo/redo, native save/reload, original
  retention and DOCX export/reopen. Zero external requests occurred; the supplied
  attachment's checksum remained unchanged. The pre-fix mixed-toolbar failure
  was reproduced before correction and passes afterward.
- Independent LibreOfficeDev 26.8.0.0.alpha0 (revision
  `2c87e51eeaa2b413ff4ae097b2705eea1995d8e5`) opened the normalized DOCX export:
  `soffice -env:UserInstallation=file:///tmp/writer-private-normalized-lo-profile --headless --convert-to fodt --outdir /tmp/writer-private-normalized-lo /tmp/writer-private-normalized.docx`.
  Conversion exited zero. Expanding ODF spaces, tabs and line breaks produced
  exact text/order for nine paragraphs, including four empty paragraphs, with
  zero mismatches. This verifies logical text interoperability, not visual parity.

Automated regressions also cover all three public producer files, deterministic
formatting, character-style precedence/toggles, active/default/inherited numbering,
unknown nested content, malformed values, revision/protection rejection after
warning overflow, exact originals, failed-import identifier rollback and empty/
zero-length-run summaries. All 40 checked-in fixture hashes/sizes were verified.

Firefox/WebKit and current Microsoft Word application execution remain untested.
Local success does not establish hosted CI or Windows/macOS results; the current
head's Actions status is recorded on PR #14. Earlier hosted runs failed before
runner allocation with no steps executed. This branch remains review-only;
do not merge or deploy without approval.
