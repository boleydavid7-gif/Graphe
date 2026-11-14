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
- Step 5 is not authorized by completing Step 4.5.

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
