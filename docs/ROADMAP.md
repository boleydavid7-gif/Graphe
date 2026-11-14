# Roadmap

These are preliminary milestones, not fixed delivery dates. They may overlap
when technically necessary. Scope and acceptance criteria will be refined before
each milestone is implemented.

1. **Repository foundation.** Establish the Rust workspace, minimal `office-core`
   library, formatting and lint rules, tests, continuous integration, and initial
   documentation. This is the current milestone.
2. **Document model.** Define Writer's initial document semantics, ownership,
   invariants, and public API boundaries, with focused tests.
3. **Editing operations.** Introduce validated operations that change the document
   model and preserve its invariants.
4. **Serialization.** Define and test persistence of the document model, including
   round-trip behavior and error handling.
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
