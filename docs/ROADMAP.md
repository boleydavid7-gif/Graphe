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
   Formatting, Clippy, tests, and build checks passed.

   **3C — Text editing operations,
   formatting, and undo/redo — not started:** build operations and history on the
   transaction foundation.

   The 3A formatting, Clippy, test, and build gate passed before 3B began. Require
   the same checks for 3B before beginning 3C; these remain distinct milestones.
4. **Serialization.** Extend the schema-1 JSON baseline introduced in Step 2 with
   persistence requirements, explicit migration policy, and additional error and
   compatibility coverage as the model grows. Basic lossless model round trips
   were brought forward because the document-model milestone requires them.
5. **Basic DOCX import/export.** Support an explicitly bounded subset of DOCX,
   using compatibility fixtures and clear handling of unsupported features.
6. **Typography and layout.** Develop text measurement, line layout, and pagination
   toward platform-independent document layout.
7. **Rendering and browser integration.** Render laid-out documents and connect
   the engine to a browser through WebAssembly, with a future React and
   TypeScript interface.
8. **Advanced Word compatibility.** Expand DOCX fidelity and Word-compatible
   behavior using increasingly demanding documents and regression coverage.

Writer comes first. Spreadsheet and presentation applications are later work;
their detailed milestones will be defined when their development begins.
