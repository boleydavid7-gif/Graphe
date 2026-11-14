# Step 4.5 pre-merge review

Date: 2026-11-14. Reviewed initial implementation `c3e690c` on
`dev/writer-browser-prototype`, then applied the targeted corrections below.
This review supersedes the initial delivery's test counts and blanket completion
wording. Step 4.5 remains unmerged and undeployed. Step 5 has not started.

“Functional” below means implemented and verified for the supported prototype
in Linux Chromium, with the listed limits. It does not mean universal Office
compatibility, every browser/input method, or public-release certification.

## Findings corrected before recommending merge

| Priority | Confirmed finding | Correction and evidence |
| --- | --- | --- |
| Critical editing fidelity | DOM copy/cut represented two paragraphs as `Alpha\n\nBeta`; pasting created an unintended third paragraph. The previous 18-test suite missed this. | Rust now extracts the validated selected text, with one LF per paragraph boundary. Copy/cut use this read; deletion follows successful clipboard writing. Three native regressions, direct WASM assertions, and actual Chromium Ctrl+C/X/V tests cover reverse Unicode ranges, paragraph structure, exact undo, and failed clipboard writing. |
| Important recovery | One damaged IndexedDB record caused the entire library listing to fail, hiding unrelated healthy documents. | Listing returns readable metadata plus explicit per-record warnings, leaving damaged records untouched. Strict loading and overwrite validation remain. Unit and browser regressions prove healthy documents remain openable. |
| Important error accessibility | A failed Save-and-continue showed its error outside the modal, where the modal made it inaccessible. | The dialog now contains its own alert and a save-copy conflict action. Failed saving preserves the dialog, unsaved work, and previous stored version. |
| Accessibility | Several muted labels fell just below 4.5:1 contrast, and the translucent editor focus indicator measured about 1.49:1. | Darkened affected labels and used an opaque focus indicator. Source-color calculations give at least 4.81:1 for the revised muted text and 5.25:1 for the editor indicator. This is not a full accessibility certification. The identity is now noninteractive branding rather than a link with no action. |
| Important test gap | Adapter tests ran natively; there was no independent suite for the generated WASM ABI. | Added eight Node/V8 tests executing the actual release WASM and target-web bindings, without engine mocks. CI runs this suite separately; Playwright discovers only browser `.spec.ts` files. |
| Important test gap | Full fixture import/export and simple IndexedDB reopen tests were separate; no test restarted the browser process. | Added edited complete-fixture save/close/reload/reopen equality, plus a fresh Chromium-process restart using the same on-disk profile and exact exported-document comparison. |

Independent engine, browser, storage, and requirements reviews found no further
confirmed critical architectural or document-integrity defect after these
targeted corrections. No document schema/native format change, new dependency,
complete editor engine, or major architectural refactor was introduced.

## Every completion criterion

| Criterion from section 20 | Assessment and evidence |
| --- | --- |
| 1. Writer opens in a browser | Functional: production build initializes actual WASM with `application/wasm` MIME and preview CSP. Blocked WASM fetch displays an error; retry initializes successfully. |
| 2. A document can be created | Functional: blank creation, stable Rust-allocated IDs, first edit, and new-document guards are tested. Imported structurally empty documents remain empty until deliberate editing. |
| 3. Editing uses Rust through WASM | Functional: real typing, deletion, Enter, mouse selection, keyboard ranges, paste, and formatting drive Rust transactions. React/DOM does not own a parallel document or undo history. Direct ABI tests also exercise rejected/stale commands. |
| 4. Text formatting works | Functional for the displayed controls: bold, italic, underline, fonts/sizes, mixed ranges, and all four alignments. Stored styles and supported properties survive native persistence. Full Word style/layout semantics remain partial, described below. |
| 5. Undo and redo work | Functional in the document surface and toolbar: Rust owns exact history; multiline paste, composition, paragraph operations, formatting, cut/paste, and failed edits are covered. Twenty snapshots; no automatic typing coalescing. Ordinary title/form input retains standard form-field keyboard behavior while title updates still become Rust transactions. |
| 6. Documents save locally | Functional: exact native bytes in IndexedDB, transaction-completion success, overwrite tokens, errors, and snapshot-versus-newer-edit handling are tested. |
| 7. Saved documents reopen | Functional: simple and edited full fixtures reopen identically after close, page reload, and a clean browser-process restart. This does not prove crash/power-loss durability or protection against eviction. |
| 8. Native files import/export | Functional: complete supported structure, metadata, IDs, named styles, formatting, and Unicode preserve exact model equality. Invalid/future data leaves current work intact. |
| 9. Existing engine tests pass | Fresh workspace run: 166 core tests, 21 adapter tests, four doctests; 191 total, zero failures. |
| 10. Browser integration tests pass | Complete production Chromium rerun recorded in the results below; targeted checks alone are not counted as completion. |
| 11. Research/review documented | Complete: RESEARCH, DECISIONS, AGENTS, architecture, roadmap, dependency review, initial report, and this pre-merge review. |
| 12. No unresolved critical architectural defect | No confirmed critical finding remains after the fixes and final regression gate. This is a reviewed prototype assessment, not proof of absence of every defect. |

## Functional, partial, and untested capabilities

| Area | Fully functional within the supported subset | Partial or deliberately limited | Not executed/certified |
| --- | --- | --- | --- |
| Browser editing | Real keyboard typing/deletion/Enter, physical mouse drag selection, directed selection, multiline plain paste, copy/cut, safe fallback after unknown browser mutation, caret scrolling | Rich HTML formatting is not imported; plaintext cannot preserve source styling or distinguish literal soft line breaks from paragraph separators. Drag/drop editing and cross-section structural replacement are rejected. | Exhaustive native navigation, visual RTL/bidi behavior, spellcheck/dictation and all browser-generated input types |
| Rust/WASM | Native and WASM compilation/lint, eight direct release-ABI tests, real production-browser transactions, structured failures, stale revision rejection, separate instances/disposal | JSON snapshots and whole-document work are prototype costs; bridge native-file errors share `invalid_file` while messages and core errors carry details | No separate `wasm-bindgen-test` harness exists; it is not claimed to have run. Node ABI and real-browser suites execute compiled WASM instead. |
| Unicode | UTF-8 storage, explicit strict UTF-16/scalar conversion, Rust Unicode 17 graphemes across runs, combining/emoji/flags, navigation affinity, reverse ranges | Logical coordinates, not a custom visual bidi/layout implementation | OS-wide language/input-method matrix and Unicode 18 upgrade |
| Formatting | Toolbar/direct properties, mixed runs, simple inherited styles, native preservation | Full OOXML toggles/themes/defaults, minimum-line-spacing and negative-indent display fidelity, font fallback and pagination | Word/LibreOffice visual comparison and cross-platform font/layout fidelity |
| Undo/redo | Atomic Rust commits and exact restoration; failed operations preserve history | Twenty complete snapshots, count rather than byte budget; no automatic typing grouping or persisted history | Long-session memory/maximum-document stress and every form-focus shortcut combination |
| Persistence | Native UTF-8 format 1/schema 1, byte ownership, deterministic supported content, Unicode paths in native tests, IndexedDB CAS and completion, full-fixture reopen and clean process restart | Browser-local storage is evictable and not a backup; library listing still loads cumulative record bytes | Real quota exhaustion, crash/power loss, storage eviction, private-mode matrix and abrupt browser termination |
| Unsaved/error recovery | Cancel, successful/failed Save-and-continue, discard, canceled browser reload, stale-tab conflict/save-copy, malformed imports, corrupt neighboring records, blocked startup/retry | No autosave/cloud recovery; users must explicitly save/export | OS/browser session restoration after a crash |
| IME | Japanese composition via Chromium CDP; synthetic cancellation/final-event de-duplication; one Rust commit | Active DOM is a transient composition buffer; unidentifiable mutations are restored rather than adopted | Real OS candidate windows, full international IME, dictation, handwriting and mobile keyboards |
| Interface/accessibility | Connected compact controls, labels, mixed/pressed states, keyboard focus, modal errors, desktop and 390px layout assertions | Basic mobile layout and measured color contrast, not complete mobile/AT support | NVDA/JAWS/VoiceOver, touch devices, complete WCAG audit and Firefox/WebKit/Safari |
| Performance | Incremental paragraph/run DOM reconciliation and measured 11,840-character/50-paragraph workload | Full-document clones/validation/snapshots; explicit browser resource caps | Maximum-sized documents, broad hardware/device matrix, OS input-to-paint latency and memory profiling |
| Cloudflare | Production static bundle and reviewed inert independent-Worker example | Deployment readiness is preparatory only | No active Cloudflare validation, DNS, route, private-access verification or deployment: approval is absent and live changes are prohibited. |

## Remaining section-by-section obligations

| Step 4.5 section | Review result |
| --- | --- |
| 1. Existing project inspection | Core model, Unicode, transactions, history, serialization/persistence and prior checks were reviewed; all available suites were rerun here. |
| 2. Engineering research | All nine questions and required established sources/alternatives are documented in RESEARCH; this audit rechecked the implementation against them. |
| 3. Mandatory retrospective | Independent reviews classified actual findings; corrections are targeted and tested. No major unapproved architectural rewrite occurred. |
| 4. Permanent rules | AGENTS contains all 14 requested rules and current ownership/scope boundaries. |
| 5. Research/review record | Updated with pre-merge findings and clear evidence limits. |
| 6. Frontend architecture | Requested React/TypeScript/Vite/CSS plus isolated Rust binding crate; no competing JS document engine. |
| 7. WASM integration | All requested operations plus clipboard read; real ABI and production loading verified, including initialization failure/retry. |
| 8. Functional editor | Requested basic capabilities work within the explicit subset; IME/clipboard limits are stated above. |
| 9. Professional design | Actual restrained Lamplight interface/screenshots; no hero/dashboard/AI content. |
| 10. Layout/controls | Top bar, title and document actions, working font/format/alignment ribbon, central canvas. No disconnected editing controls. |
| 11. Local-first storage | IndexedDB stores native bytes with explicit saving and transactional overwrite checks. No complete documents in localStorage; no accounts/cloud. |
| 12. Responsive behavior | Desktop and narrow-window tests pass; real touch/mobile keyboard certification is untested. |
| 13. Accessibility | Semantics/labels/focus/errors and specific contrast corrections verified; advanced AT/manual audit remains untested. |
| 14. Testing | Rust, frontend, direct compiled WASM and real production Chromium suites run; unexecuted matrices are listed explicitly. |
| 15. Reliability/performance | Transaction/input/storage failure preservation, incremental DOM and measured workload verified; broad stress/durability claims are not made. |
| 16. Cloudflare readiness | Static output/inert example only. Public address remains `writer.proairetos.com`; no live change/deployment/access validation. |
| 17. Documentation | README, ARCHITECTURE, ROADMAP, DECISIONS, RESEARCH and AGENTS exist; audit and exact verification are linked. |
| 18. Required verification | Exact commands/current results below; historical CI is distinguished from the follow-up commit's CI. |
| 19. Git workflow | Dedicated branch/PR #7; main is untouched by this review. No deployment or ProairetOS/Noesis changes. |
| 20. Completion gate | Twelve individual criteria reviewed above, with scoped evidence and testing limitations. |
| 21. Delivery report | This review plus actual screenshots, dependencies and result table supersede unsupported broad interpretations of the initial report. Step 5 remains unstarted. |

## Actual commands and results

Same local toolchain as the initial report: Rust/Cargo 1.100.0, Node 24.22.1,
npm 11.9.0, wasm-bindgen 0.2.130, Linux Chromium 155.0.8059.79.

| Directory | Command executed | Current result |
| --- | --- | --- |
| Repository root | `cargo fmt --all -- --check` | Pass |
| Repository root | `cargo clippy --workspace --all-targets -- -D warnings` | Pass |
| Repository root | `cargo test --workspace` | Pass: 191 tests including four doctests; zero failures |
| Repository root | `cargo build --workspace` | Pass |
| Repository root | `cargo clippy --workspace --target wasm32-unknown-unknown -- -D warnings` | Pass for WASM-specific code |
| `apps/writer` | `npm run typecheck` | Pass |
| `apps/writer` | `npm run lint` | Pass |
| `apps/writer` | `npm test` | Pass: 20 unit tests |
| `apps/writer` | `npm run test:wasm` | Pass: eight real compiled-WASM tests; zero skipped |
| `apps/writer` | `npm run build` | Pass: locked release WASM, regenerated bindings, TypeScript and Vite |
| `apps/writer` | `npm run test:browser` | Pass: 27 production Chromium tests, 31.4 seconds, zero skipped |
| `apps/writer` | `npm audit --audit-level=moderate` | Pass: zero vulnerabilities |
| `apps/writer` | `npm ls --depth=0` | Pass: pinned dependency graph unchanged |
| Repository root | `git diff --check` | Pass |

The initial audit rerun on unchanged delivery code passed 188 Rust tests and 18
storage units. New review regressions increased coverage. One intermediate full
browser run had 25 passes and one timeout: Playwright waited for `load` after a
deliberately canceled reload, which must not navigate. The test now requests the
real reload via Chromium CDP and checks the unchanged exported document after
dismissing `beforeunload`; the corrected targeted test passed. No failed run is
reported as a pass. Default sandbox execution restrictions were resolved through
permitted execution of the binding generator/Node/browser tools; these are not
skipped checks.

Final production assets are WASM 728.19 kB (208.93 kB gzip), JavaScript
256.52 kB (79.64 kB gzip), and CSS 7.43 kB (2.42 kB gzip). The final 30-event
typing measurement on 11,840 characters / 50 paragraphs was 2.5 ms median,
5.0 ms p95, and 5.4 ms maximum. It measures synchronous handling through Rust
and DOM reconciliation, not OS input-to-paint or maximum-document performance.

Browser production output is served through Vite preview with actual WASM and
CSP/MIME assertions. Direct ABI tests run generated target-web bindings with
real WASM bytes in Node/V8. Synthetic clipboard/composition/error injection is
identified in tests; actual native clipboard, keyboard/mouse and clean browser
restart paths are separate real-browser checks. Tests do not use another editor
or a mocked Rust engine.

No configured automated suite is intentionally omitted. Firefox/WebKit, OS IME,
screen readers, real mobile hardware, fault-level durability and Cloudflare
deployment tests were not run for the reasons above. They are untested scopes,
not passing or skipped configured test cases.

## Recommendation

All local configured suites passed after correction. The follow-up commit's
GitHub Actions results are reported in PR #7 and the review response; the initial
commit's green checks are not substituted for the changed code. Retain this as a bounded private-development prototype;
do not describe it as universally verified, production-ready Word compatibility.
Full style semantics, byte-budgeted history/structural performance changes and
broader platform/accessibility certification require separately scoped work.
No Step 5 implementation, merge or deployment is authorized by this audit.
