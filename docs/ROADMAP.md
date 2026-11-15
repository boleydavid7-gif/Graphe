# Roadmap

These are preliminary milestones, not fixed delivery dates. They may overlap
when technically necessary. Scope and acceptance criteria will be refined before
each milestone is implemented.

1. **Repository foundation — implemented.** Rust workspace, minimal `office-core`
   library, formatting and lint rules, tests, continuous integration, and initial
   documentation. Merged into `main`.
2. **Document model — implemented.** Structured documents, sections, paragraphs,
   text runs, formatting, named paragraph styles, validation, versioned JSON, and
   focused tests with a realistic fixture. No editing, layout, or DOCX conversion.
   Merged into `main`.
3. **Positions and editing, divided into three gated milestones.**

   **3A — Unicode-aware positions and selections — implemented:** explicit UTF-8,
   UTF-16, and grapheme offsets; strict boundary conversions; stable document and
   paragraph IDs; logical selections; comprehensive Unicode tests. The document
   model remains unchanged. Merged into `main`.

   **3B — Atomic validated editing transactions — implemented:** detached drafts,
   complete candidate validation, atomic in-memory publication, session-scoped
   revision tokens, stale/foreign rejection, and explicit no-op behavior.
   Formatting, Clippy, tests, and build checks passed. Merged into `main`.

   **3C — Text editing operations, formatting, and undo/redo — implemented:**
   transaction-local editing endpoints; Unicode-safe insertion, deletion, and
   replacement within a section; character and paragraph patches; named-style
   assignment; and bounded exact-snapshot history. Cross-section text replacement
   remains explicitly unsupported. Formatting, Clippy, tests, and build checks passed.
   Merged into `main`.

   The 3A formatting, Clippy, test, and build gate passed before 3B began; the 3B
   gate passed before 3C began. Each remains a distinct validated milestone.
4. **Native document persistence — implemented and merged.** Add the
   provisional `.writer` UTF-8 JSON envelope around schema 1, deterministic byte
   serialization, strict version checking, native safe-save/load, structured errors,
   fixtures, and preservation/failure tests. Reuse the model validator and loader.
   Merged into `main` at `52ac4e9` through PR #6 after the milestone checks and
   approval.
   Basic model round trips were introduced in Step 2; Step 4 adds the file and
   storage contract. The conversion branch now adds native format 2/model schema 2
   and strict legacy migration, retaining an optional original outside history.
5. **Basic DOCX import/export — implemented and merged.**
   Step 5A package inspection/original retention was merged through PR #9 at
   `a504e2f`. Work on `dev/docx-conversion` adds supported semantic import/export,
   generic page geometry and page-break-before, versioned source-bearing native
   files and browser save-before-editing. Unsupported semantic content remains
   preserved read-only; reconstructed export and original download are distinct.
   The shared model stays format-neutral. No additional formats are implemented.
   See [DOCX.md](DOCX.md), [the conversion contract](DOCX_CONVERSION.md) and
   [actual verification](DOCX_CONVERSION_VERIFICATION.md). This conversion work
   was merged through PR #10 at `8ab784a`.
6. **Typography and layout — implemented and merged.**
   After the DOCX prerequisite passed its checks, `dev/typography-layout-engine`
   adds a separate Rust layout crate with pinned fonts, shaping, Unicode line
   breaking/bidi, paragraph geometry and pagination. A revision-bound WASM API
   supplies glyph outlines and geometry to a read-only page preview. The existing
   semantic editor remains available. Merged through PR #11 at `86f2ad8`;
   [LAYOUT.md](LAYOUT.md) documents the independent layout profile and remaining
   Word-compatibility limits. [Actual verification](LAYOUT_VERIFICATION.md)
   records 399 Rust, 25 compiled-WASM and 41 real-browser tests passing locally,
   plus builds and linting; final remote CI results belong to the PR checks.
7. **Lightweight Writer and Lamplight UI — implemented and merged.**
   `dev/writer-lamplight` connects Rust page geometry to everyday typing,
   selections, navigation, formatting and clipboard input; preserves the existing
   model, transactions, persistence and DOCX adapters; and adds reliable local
   autosave using the existing etag transactions. The interface follows the
   owner's approved desktop and rough mobile references. Existing named styles
   can be assigned; advanced style authoring/templates and other future features
   are absent. [WRITER_STEP7.md](WRITER_STEP7.md) documents the boundary and limits;
   [verification](WRITER_STEP7_VERIFICATION.md) records actual tests and screenshots.
   Merged through PR #12 at `dab4db7`; no manual deployment was performed.

   **Audit gate after Step 7 approval:** pause feature development for the
   comprehensive technical, security, DOCX-compatibility and commercial-licensing
   audit. Do not begin Step 8 automatically.

   **7.5 — File-format support — explicitly authorized follow-up.**
   The owner requested DOCX/ODT/RTF/TXT open/edit/save, native Writer retention
   and PDF export only, including Open/Save As, original retention and loss
   warnings. `dev/writer-file-formats` reuses the existing model and Rust layout
   with concrete format adapters. [FILE_FORMATS.md](FILE_FORMATS.md) records
   declared profiles; [verification](FILE_FORMATS_VERIFICATION.md) records actual
   tests and independent-consumer checks. The owner authorized merging after
   successful verification. Deployment, Step 8 and the comprehensive audit are
   not implied by this additional feature request.

   **Stability Phase 2 — dedicated development branch.** Rust-authoritative touch
   selection, bounded external-format normalization and measured library-listing
   corrections have regression coverage. PDF text experiments were rejected to
   preserve rendering fidelity. Large-document responsiveness, physical-device
   verification and hosted/browser CI availability remain explicit limits. See
   [PHASE2_REPORT.md](PHASE2_REPORT.md); no merge, deployment, ProairetOS integration
   or next feature milestone is authorized automatically.
8. **Advanced Word compatibility.** Expand DOCX fidelity and Word-compatible
   behavior using increasingly demanding documents and regression coverage.

Writer comes first. Spreadsheet and presentation applications are later work;
their detailed milestones will be defined when their development begins.

## Step 4.5 — Browser Writer prototype

Implemented on `dev/writer-browser-prototype`; all local verification gates pass
as recorded in [VERIFICATION.md](VERIFICATION.md).
The follow-up [pre-merge review](PRE_MERGE_REVIEW.md) supersedes initial counts,
adds targeted reliability regressions, and distinguishes functional, partial,
and untested requirements.
This milestone was merged through PR #7 at `0a5d806` with post-merge CI passing.
Cloudflare deployment remains unverified; the follow-up build bootstrap is
documented in [CLOUDFLARE_BUILD.md](CLOUDFLARE_BUILD.md). It adds the independent
`office-wasm` session boundary, a React/TypeScript/Vite interface in `apps/writer`,
semantic browser input with Rust-owned Unicode and edits, a targeted paragraph
break operation, a limited Rust effective-formatting policy, and explicit
IndexedDB save/open plus exact native `.writer` import/export.

The browser prototype supports ordinary typing/deletion, Enter, plain-text
multiline paste, directed selections, formatting controls, and Rust undo/redo.
Local saves use transactional etag comparison and guard dirty state against
asynchronous completion. Input, import, and storage errors must preserve unsaved
work. Resource bounds and remaining limits are documented in README and
ARCHITECTURE; research and retrospective classifications are in RESEARCH.

Acceptance requires the four engine checks, TypeScript, lint, frontend unit tests,
production build and WASM loading, real-browser flows, screenshots, and an honest
report of synthetic IME/accessibility limits. Actual results belong in the
milestone report. No Word page layout, DOCX adapter, autosave, cloud storage,
deployment, DNS, or access-control change is included. The Cloudflare example is
inert preparation for a separate Worker; build output is not a published site.

The owner subsequently authorized completion of missing DOCX conversion before
Step 6. Its profile, preservation policy and regression fixtures are recorded in
[DOCX.md](DOCX.md) and [DOCX_CONVERSION.md](DOCX_CONVERSION.md). Significant changes
outside that scope, merging and deployment still require approval.
