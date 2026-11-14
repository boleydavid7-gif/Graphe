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
   storage contract. No migration exists until another version needs one.
5. **Basic DOCX import/export — not started.** Research and obtain approval for
   an explicitly bounded subset of DOCX, with compatibility fixtures and a clear
   policy for unsupported information. Completing Step 4.5 does not authorize it.
6. **Typography and layout.** Develop text measurement, line layout, and pagination
   toward platform-independent document layout.
7. **Rendering and browser integration beyond the prototype.** Extend the
   Step 4.5 WebAssembly/React baseline with rendering of laid-out documents and
   the acceptance criteria needed for professional browser integration.
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

After this milestone, propose a researched Step 5 scope with explicit fidelity
goals, supported constructs, unsupported-element handling, and regression
fixtures. Wait for approval before starting that work or making significant new
architectural changes.
