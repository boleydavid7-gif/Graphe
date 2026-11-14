# Step 4.5 verification and delivery

This is the **initial delivery** record for `c3e690c`. The subsequent
[pre-merge review](PRE_MERGE_REVIEW.md) supersedes its test counts, records new
fixes and verification, and distinguishes functional, partial and untested scope.

Verified on 2026-11-14 on `dev/writer-browser-prototype`. Step 4 was merged with
approval at `52ac4e9` ([PR #6](https://github.com/boleydavid7-gif/stuff/pull/6)).
Step 4.5 remains unmerged and undeployed. Step 5 has not started.

## Implemented functionality and files

- `crates/office-core/src/{edit_ops,editing}.rs`: one targeted, atomic paragraph
  break operation; 15 regressions in `crates/office-core/tests/paragraphs.rs`.
  Existing model, public operations, native schema, and format remain intact.
- `crates/office-wasm`: Rust-owned browser session, commands, presentation,
  directed Unicode selections, typing properties, exact core history, native
  bytes, structured errors, resource limits, and 18 native adapter tests.
- `apps/writer/src`: React application, controlled semantic editing surface,
  thin owning WASM client, immutable transport types, CSS, native IndexedDB
  adapter, and 18 storage unit tests. Functional New/Open/Save/Close/Export,
  title, text/paragraph editing, mixed formatting, fonts, alignment, undo/redo,
  keyboard selection, plain-text paste, and composition handling.
- `apps/writer/tests/writer.spec.ts`: 18 production-browser integration tests.
- Frontend package/lock/config files and `scripts/build-wasm.mjs`: pinned,
  reproducible build/test tooling; generated bindings and output are ignored.
- `.github/workflows/ci.yml`: existing three-OS engine checks plus frontend,
  advisory audit, release WASM, and production Chromium integration.
- `AGENTS.md`, README, architecture, roadmap, decisions, research, dependency
  review, this report, and actual application screenshots.
- `apps/writer/cloudflare.config.ts.example`: inert, separate Worker example;
  no live deployment configuration, DNS, or access-control change.

## Commands and actual results

Environment: Rust/Cargo stable **1.100.0**, wasm-bindgen CLI **0.2.130**,
Node **24.22.1**, npm **11.9.0**, Chromium **155.0.8059.79** on Linux.
The managed environment uses `CARGO_HOME=/workspace/.office-rust/cargo`,
`RUSTUP_HOME=/workspace/.office-rust/rustup`, and that Cargo bin directory in PATH.

Before feature changes, the four engine commands below passed on Step 4:
151 unit/integration tests and four doctests, **155 total**. Final results:

| Directory | Exact verification command | Result |
| --- | --- | --- |
| Repository root | `cargo fmt --all -- --check` | Pass |
| Repository root | `cargo clippy --workspace --all-targets -- -D warnings` | Pass, no warnings |
| Repository root | `cargo test --workspace` | Pass: 166 core tests, 18 adapter tests, four doctests; **188 total** |
| Repository root | `cargo build --workspace` | Pass |
| `apps/writer` | `npm run typecheck` | Pass |
| `apps/writer` | `npm run lint` | Pass, zero warnings |
| `apps/writer` | `npm test` | Pass: **18** storage unit tests |
| `apps/writer` | `npm run build` | Pass: locked release WASM, bindings, TypeScript, Vite production bundle |
| `apps/writer` | `npm run test:browser` | Pass: **18** real Chromium tests, 21.7 seconds |
| `apps/writer` | `npm audit --audit-level=moderate` | Pass: **zero vulnerabilities** |
| Repository root | `git diff --check` | Pass |

`npm run build` executes `npm run build:wasm`, which runs:

```sh
cargo build --locked --release -p office-wasm --target wasm32-unknown-unknown
wasm-bindgen target/wasm32-unknown-unknown/release/office_wasm.wasm --target web --out-dir apps/writer/src/wasm --out-name office_wasm
```

The browser runner starts `npm run preview -- --port 4173 --strictPort` against
the production `dist`. Tests verify a real `application/wasm` response, successful
initialization under the preview CSP, and native transactions through the actual
WASM bundle. Production assets: WASM **724.39 kB** (207.73 kB gzip), JavaScript
**255.02 kB** (79.24 kB gzip), CSS **7.32 kB** (2.41 kB gzip).

Development setup also executed `rustup target add wasm32-unknown-unknown`,
verified `wasm-bindgen --version`, and installed the pinned npm graph with
`npm install`. The local binding CLI came from the official 0.2.130 Linux release,
verified with its published SHA-256 and installed with `install -m 755`; local
`cargo install` was not required. `npm ls --depth=0` and `npm ls minimatch` passed. The initial npm
audit's development-only advisory was corrected with a scoped override, as
documented in [DEPENDENCIES.md](DEPENDENCIES.md). The first default-sandbox
production build could not execute the installed binding generator (`EPERM`);
the permitted execution completed successfully. An exploratory npm command run
from the Cargo root failed because it has no package.json; all frontend checks
were then run from `apps/writer` as documented. No check remains blocked.

GitHub Actions runs these checks on push and pull requests. Its current results
are linked from the development pull request; the completion response records
the observed run rather than inferring remote success from local checks.

## Browser regressions and measurement

Real browser coverage includes create/type/delete/Enter, native arrows and
directed selections across split combining/ZWJ graphemes, formatting and font
keyboard focus, paragraph alignment, exact Rust undo/redo, actual Chromium
clipboard plus plain/rich paste policy, CDP Japanese IME and cancellation/tail
events, noncancellable input recovery, native fixture import/export, corrupt
imports, empty native documents, rejected cross-section edits, IndexedDB
save/close/reopen/reload, transaction failure, conflicting tabs/save-copy, and
typing during an outstanding save. A long-document test checks caret visibility.

On **11,840 characters / 50 paragraphs**, 30 native input events measured
**3.3 ms median**, **4.3 ms p95**, **4.7 ms maximum** from capturing `beforeinput`
through synchronous Rust transaction and DOM reconciliation. This is one local
measurement, excluding OS input-to-paint and broad device/workload profiling;
it does not establish large-document performance. No speculative performance
rewrite was introduced.

Actual running application screenshots:

- [Desktop](images/writer-desktop.png)
- [Small window/mobile adaptation](images/writer-mobile.png)

## Review, remaining decisions, and readiness

[RESEARCH.md](RESEARCH.md) records authoritative Microsoft OOXML, LibreOffice,
ONLYOFFICE, ProseMirror, Rust/WASM, Unicode, MDN, and accessibility sources,
alternatives, retrospective classifications, and all critical corrections.
No confirmed critical defect was inherited from Steps 1–4. New integration
defects involving missing selections, empty placeholder recovery, grapheme
navigation/event ordering, focus, and caret visibility were corrected with
regressions before delivery. All local gates pass; no unresolved critical
finding remains.

Important future decisions require approval before major changes: full Word
style/toggle/theme and unsupported-content semantics; document/history byte
budgets and any structural-sharing/incremental-validation redesign. The library
currently lists all saved records, so large libraries can incur cumulative byte
copies. Browser display approximates minimum line spacing and negative
indentation and relies on host font metrics. Resource limits are explicit in
README and ARCHITECTURE. Native editing across section boundaries remains
unsupported. History is session-local, without automatic typing coalescing.

Chromium CDP/synthetic IME tests do not certify real OS candidate windows,
dictation, every input method, or touch/browser combinations. Manual screen-reader
and cross-browser review is still needed. Paste is plain text; rich clipboard
import, drag/drop editing, layout/pagination, DOCX, autosave, cloud storage, and
collaboration are not implemented. Browser storage can be evicted; export is
the backup mechanism.

React/React DOM and wasm-bindgen are the new runtime/binding dependencies.
Build/test dependencies and licenses are reviewed in DEPENDENCIES; no project's
license has been selected. No complete editor engine is embedded.

Production static assets and an inert Cloudflare example are prepared. Deployment
at the permanent address **writer.proairetos.com** still requires approval,
active configuration, and verification of private-development access controls.
ProairetOS and Noesis were not modified. No deployment or live settings change
occurred.

Recommendation for Step 5: first approve a researched DOCX subset and unsupported
information policy, then add bounded OOXML package/XML adapters and compatibility
fixtures without coupling them to React. Do not begin Step 5 automatically.
