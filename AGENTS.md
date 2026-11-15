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
   and compatibility. Pin resolved dependencies and keep lockfiles. Dependency
   licensing does not decide this project's pending software license.
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
  The current save/export/mobile reliability correction is on
  `fix/writer-save-export-mobile-reliability`: create a review PR, but do not
  merge or deploy. Do not change Cloudflare or begin unrelated feature work.
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
- Recognize inert DOCX auxiliary data by exact relationship URI/source role,
  content type and inspected payload, never by filename alone. Preserve it in
  the original. Known theme/script formatting and layout preferences may be
  normalized with notices; content, embedded fonts and unknown relationships
  remain guarded. See `docs/DOCX_RELATIONSHIPS.md`; current-content
  export must disclose omitted unmodeled metadata and auxiliary parts.
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
  explicit acknowledgement of losses. Downloading an external copy does not
  mark the local native save current or replace its etag. Native exports retain
  sources, which may include deleted text; external copies do not retain them.
- Keep a prepared export's Blob URL valid for a fresh native download/open link;
  release it on replacement, dismissal or unmount. A browser download request
  cannot prove a file reached disk. Never equate export with a committed local save.
- Mobile focus must occur synchronously in a completed trusted tap, including
  compatibility click. Stale layout must retain the Rust selection, not prevent
  focus. Scroll/cancel must not summon input. VisualViewport adjustments only
  reveal Rust geometry; bound keyboard padding within the workspace allocation.
- TXT decoding uses strict UTF-8 (optional BOM) or explicitly BOM-marked UTF-16;
  never guess legacy encodings or replace invalid Unicode silently. PDF export
  renders Rust page geometry, not browser print layout. Disclose vector-outline
  export's search/selection/accessibility limitations before download.
- The planned comprehensive technical, security, DOCX-compatibility and commercial-
  licensing audit remains pending. The owner explicitly requested Step 7.5
  after Step 7; do not interpret that as completion of the audit or permission
  to begin Step 8 or additional feature work automatically.

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
