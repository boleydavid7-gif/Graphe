# Writer engineering rules

These rules apply to Step 4.5 and all future Writer milestones. User instructions
and explicit approvals govern the task's scope.

1. Research before adding a technical subsystem or making a major architectural
   decision. Record the question, alternatives, evidence, and tradeoffs first.
2. Consult relevant standards, official documentation, and established
   implementations. Prefer primary sources and link the versions reviewed.
3. Consider older proven solutions where technically relevant; do not select an
   approach merely because it is newest.
4. Evaluate mature low-level libraries before implementing specialized algorithms.
   Do not embed another complete editor or Office engine.
5. Check dependency licensing, maintenance, security advisories, target support,
   and compatibility. Pin resolved dependencies and keep lockfiles. Graphe's proprietary license is recorded in the repository-root
   LICENSE; third-party dependency licensing remains separate and must be
   respected. Review docs/COMMERCIAL_LICENSING_AUDIT.md before expanding
   dependencies, bundled assets, or production distribution.
6. Review existing relevant code against the new research, including public API
   assumptions and test gaps. Passing tests alone do not establish correctness.
7. Classify weaknesses as critical, important, or optional and explain their
   effect on future milestones.
8. Fix critical correctness, security, or document-integrity defects before
   extending affected systems; add regression coverage. Stop for a major blocker.
9. Obtain approval before significant architectural changes that are not already
   authorized by the user's task. Present concrete recommendations and tradeoffs.
10. Preserve working APIs where reasonable. Prefer targeted changes; avoid
    unnecessary rewrites, speculative layers, and unrelated subsystem expansion.
11. Keep concise research and retrospective findings in `docs/RESEARCH.md`.
12. Record accepted architectural decisions and consequences in
    `docs/DECISIONS.md`.
13. Require appropriate automated and regression tests. Browser editing changes
    need real-browser integration checks alongside Rust and frontend tests;
    document the limits of synthetic IME and accessibility testing.
14. Do not merge or deploy without approval. Never change live DNS, Cloudflare
    settings, deployment configuration, or development-access restrictions as an
    incidental implementation step.

## Current boundaries

- Work only in this repository; do not modify ProairetOS or Noesis.
- Rust owns document content, validation, edits, and history. React/TypeScript
  presents immutable snapshots, translates browser input, and owns UI/storage
  interaction state. It must not become a second document or undo engine.
- Keep browser bindings out of `office-core`. Keep native persistence and the
  native format compatible with their documented version policy.
- Imported IDs are opaque: never interpolate them into unescaped CSS selectors.
- Use Rust's pinned grapheme rules and explicit UTF-8/UTF-16 conversions. Do not
  substitute JavaScript string length or a different segmenter for engine offsets.
- Keep unsaved Rust state intact on input, import, or storage failure.
- Step 7 was merged through PR #12 at `dab4db7`; Step 7.5 was merged through
  PR #13 at `3f447c4` with the owner's explicit approval. Preserve its file
  formats, research, regression tests and disclosed verification limits.
  The focused DOCX correction was merged through PR #14 at `8ed5144`.
  The save/export/mobile reliability correction was merged through PR #15 at
  `22dfe38`. Preserve its safeguards and disclosed device-testing limits.
  The document-menu correction was merged through PR #16 at `b3e879e`.
  The stability/file-compatibility audit was committed to PR #17 at `f78e3ba`.
  Phase 2 starts on `dev/writer-stability-phase2` from latest audited PR #17
  head `e501d76`, including the Lamplight bridge also merged through PR #19.
  Phase 2 was merged through PR #24 into the audited branch at `f33e086`.
  The owner now authorizes merging the audited branch into main through PR #17
  after verification, preserving main's trusted Lamplight embedding and mobile
  appearance changes. This does not authorize deployment commands, Cloudflare
  changes, unrelated application changes or future merges. Preserve all audit
  corrections, regression coverage and disclosed verification limitations.
- Keep file-format structures and parser dependencies in their adapters. Generic
  page geometry and native source records must not introduce DOCX structures
  into the shared semantic model or copy source bytes into undo snapshots.
- Editable DOCX import requires a complete supported projection and persistence
  of its exact original before browser editing is activated. Unsupported content
  stays preserved read-only with diagnostics; do not publish lossy editable copies.
- The owner subsequently authorized safe normalization of known unsupported
  formatting so ordinary Word/LibreOffice documents can be edited. Preserve all
  text and its order, retain exact originals, expose bounded adjustment notices,
  and disclose regenerated-export losses. This supersedes the earlier demand
  that every formatting value fit the native model exactly. Content, protection,
  revision, malformed-structure and unknown-markup guards remain strict; an
  independent warning budget must never hide a blocking finding.
- Phase 2's bounded ODT/RTF normalization follows that same original-preservation
  policy. Classify exact known settings before blocker budgeting, retain a
  meaningful-loss summary before notice truncation, and use full-file copy
  assessment across external destinations. Unknown/protected/content-bearing
  constructs remain guarded. See `docs/PHASE2_COMPATIBILITY.md`.
- Recognize inert DOCX auxiliary data by exact relationship URI/source role,
  content type and inspected payload, never by filename alone. Preserve it in
  the original. Known theme/script formatting and layout preferences may be
  normalized with notices; content, embedded fonts and unknown relationships
  remain guarded. Source-aware DOCX copies may carry validated static metadata
  and auxiliary parts around regenerated content; never graft old body/styles,
  caches or active relationships. Disclose meaningful omissions. See
  `docs/SAVE_COPY.md` and `docs/DOCX_RELATIONSHIPS.md`.
- Use full `WriterFile` APIs when source state is present. Document-only loaders
  must reject source loss. Preserve strict legacy decoding and explicit migrations;
  do not accept newer fields under old schema/envelope version labels.
- Distinguish current-content DOCX export from original download. Disclose that a
  source-bearing native file may contain text removed from the edited document.
- Formats outside the authorized Step 7.5 set and deployments require separate
  authorization. Do not infer Office-format fidelity from self-roundtrips or
  library probes; use independent consumers and document declared profiles.
- Rust owns font resolution, shaping, line breaking and pagination. Browser
  layout previews render Rust-provided glyph paths and geometry; do not use
  browser text measurement or CSS wrapping as a second layout engine.
- Deterministic layout requires the documented font catalog and versioned
  layout profile. Do not discover host fonts implicitly. Preserve font licenses
  in source distributions and production browser assets.
- Layout limits must return structured errors without mutating the document.
  Keep layout snapshots and caches outside persisted documents and undo history;
  reject stale revision snapshots at the browser boundary.
- The owner's primary desktop and rough mobile visual references are
  `docs/design/writer-lamplight-reference.png` and
  `docs/design/writer-lamplight-mobile-reference.png`. See `docs/design/README.md`.
  Preserve these files and document meaningful deviations. Use opaque warm
  brown/charcoal surfaces, restrained gold and white paper, without glass or
  decorative gradients. Show only working commands; local saving is not cloud sync.
- In paged editing, visible glyphs, caret/selection geometry and navigation come
  from Rust. A controlled semantic DOM input projection may buffer composition;
  it is not an independent document or layout engine. Do not hide it from keyboard
  or assistive input with `display:none`. Keep browser visual navigation from
  overriding Rust page geometry, while retaining valid logical input selections.
- Autosave and explicit Save share one serialized write lane and existing etag
  checks. Guard completions by engine/session/document/revision and replacement
  epoch. Do not clear newer dirty work, overwrite conflicts, serialize transient
  IME text, or label active composition saved. Lifecycle flushing is best effort.
- Save As converts immutable Rust state with origin-bound assessments and
  brief meaningful-loss warnings confirmed by Save copy. Ordinary copies have
  no checkbox. Use full `WriterFile` source-aware APIs at this boundary.
  Downloading an external copy does not
  mark the local native save current or replace its etag. Native exports retain
  sources, which may include deleted text; external copies do not retain them.
- Keep a prepared export's Blob URL valid for a fresh native download/open link;
  release it on replacement, dismissal or unmount. A browser download request
  cannot prove a file reached disk. Never equate export with a committed local save.
- Mobile focus must occur synchronously in a completed trusted tap, including
  compatibility click. Stale layout must retain the Rust selection, not prevent
  focus. Scroll/cancel must not summon input. VisualViewport adjustments only
  reveal Rust geometry; bound keyboard padding within the workspace allocation.
- Mobile word/endpoint selection uses Rust layout actions and pinned Unicode
  segmentation. JavaScript may track gesture coordinates and render handles,
  but must not segment text or derive document offsets from browser layout.
  Keep logical anchor/focus identity across crossings and bidi ranges. Cancel
  gestures on changed identity/revision/selection, composition, conflicting
  multi-touch or replacement. Selection-only changes do not advance revisions.
- Clipboard cut/paste from custom selection controls must recheck the live Rust
  identity, revision and exact directed selection after browser I/O, as well as
  composition and control lifetime. Never cut before clipboard writing succeeds.
  Never restore an unfocused contenteditable DOM range from a hold timer: that
  can itself focus the editor. Use trusted release/Edit for focus.
- Do not close a disclosure menu on ambiguous null-target blur: Safari may
  decline to focus a pointer-activated button, and closing before click swallows
  the command. Preserve outside-pointer, Escape and known-outside keyboard
  dismissal. Dialog actions must have a visible return-focus target. Keep menu
  options reachable by scrolling within the visible viewport.
- TXT decoding uses strict UTF-8 (optional BOM) or explicitly BOM-marked UTF-16;
  never guess legacy encodings or replace invalid Unicode silently. PDF export
  renders Rust page geometry, not browser print layout. Disclose vector-outline
  export's search/selection/accessibility limitations before download.
- Phase 2's Type 3 PDF experiment failed independent rendering acceptance; keep
  the verified outlined profile and warning. Do not revive experimental layout
  metadata or claim searchable/tagged/archival PDF without the new acceptance
  evidence described in `docs/PHASE2_PDF.md`.
- Library metadata listing uses measured cursor traversal and shared validation,
  without the owned body copies required by load/save. Keep completion, corruption,
  sorting and etag guards. Do not infer one-record total memory or add a database
  migration/cache from estimates; use the manual tools and recorded measurements
  in `docs/PHASE2_PERFORMANCE.md`.
- The planned comprehensive technical, security, DOCX-compatibility and commercial-
  licensing audit remains pending. The owner explicitly requested Step 7.5
  after Step 7; do not interpret that as completion of the audit or permission
  to begin Step 8 or additional feature work automatically.

## Table capability

- The owner authorized basic tables in the renamed Graphe app (`apps/writer`).
  Work from current main on `dev/graphe-tables`; preserve separate unfinished
  audit work. This request does not authorize a merge or deployment.
- Tables are format-neutral typed blocks with stable table/row/cell IDs and
  existing paragraphs. Keep destructive text edits inside one cell or a body
  flow that does not cross a table. Structural edits use core transactions.
- Tables shipped in native envelope/model 5; Step 8A advances the current
  version to 6. Keep strict migrations; never accept tables under pre-5 labels,
  images under pre-6 labels, or discard retained originals.
- Render Rust-provided cell and paragraph geometry. The semantic HTML table is
  an input/accessibility projection, not a competing document/layout engine.
- Preserve explicit total width when editing columns. Keep unsupported merged,
  nested or content-bearing table imports guarded. Never silently flatten a
  table on export; consult `docs/TABLES.md` and `docs/TABLES_RESEARCH.md`.

## Step 8A images

- The owner authorized embedded PNG/JPEG images on `dev/graphe-step8a-images`
  from main `184364a`. This authorizes implementation and a review PR, not merge
  or deployment. Preserve separate unfinished audit work.
- Images are stable, format-neutral standalone body blocks. Rust owns raster
  validation, selected-image identity, transactions/history and page geometry.
  Text edits cannot silently cross an image. Do not use DOM measurements or a
  JavaScript asset model to replace engine decisions.
- Keep exact raster originals and original imported files. Derived normalized
  PNG/RGBA caches are shared runtime data, not persisted undo payloads. Preserve
  resource preflight before pixel allocation, strict CRC/metadata handling and
  explicit color-approximation notices. Consult `docs/IMAGES.md` and research.
- Keep unsupported mixed/floating/cropped/cell images guarded. Rebuild exported
  relationships from current content; retained source media must not resurrect
  deleted images. RTF export must fail and TXT must warn rather than omit images.
- Keep pixel bytes out of ordinary snapshots. Preserve image Blob URL cleanup,
  trusted tap focus, IME exclusion, revision/selection checks after file I/O,
  and all local-save protections. Physical-device checks remain distinct from
  emulated mobile tests. Enforce the production WASM static asset size budget.

## Required engine checks

Run from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

For the browser prototype, also run its documented TypeScript, lint, unit,
real-browser, and production-build checks. Verify production WASM loading and
capture screenshots from the running application. Report commands and actual
results; do not claim deployment or unperformed verification.

## External fonts

The owner authorized external browser font loading on dev/graphe-external-fonts.
Keep all canonical 42 font binaries, manifest, checksums, notices and provenance.
Native Rust remains embedded; WASM requires the complete independently verified
catalog before layout, selection or PDF. Preserve registration order and catalog
identity. Every cache read must verify length/hash; no host-font fallback or
editable partial readiness. Keep CSS previews on verified binary FontFace bytes
and versioned asset URLs. No R2, deployment/security-setting changes, merge or
deploy are authorized. See docs/EXTERNAL_FONTS.md and its research.
