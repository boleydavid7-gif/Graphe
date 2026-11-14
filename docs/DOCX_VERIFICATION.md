# Step 5A verification — 2026-11-14

Work is on `dev/docx-package-foundation`, based on main
`c9aa0eadd480d771aa5fd95650911eeae8d846a3`. This is a package/preservation
foundation, not completion of editable DOCX import/export. No merge, deployment,
other-format implementation, native schema or frontend change is included.

## Environment and baseline

Linux x86_64; stable Rust 1.100.0; wasm32-unknown-unknown installed;
wasm-bindgen-cli 0.2.130; Node 24.22.1; npm 11.9.0; Chromium 155.0.8059.79.
Rust commands used the existing workspace-local toolchain:

```sh
export CARGO_HOME=/workspace/.office-rust/cargo
export RUSTUP_HOME=/workspace/.office-rust/rustup
export PATH=/workspace/.office-rust/cargo/bin:$PATH
```

Before changes, the four required engine commands passed: formatting, Clippy,
191 tests including doctests, and build. Existing TypeScript, lint, 20 storage
tests, 2 build-script tests and 8 compiled-WASM tests also passed.

## Final executed checks

From the repository root, all passed:

| Exact command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed, no warnings |
| `cargo test --workspace` | 256 passed, zero failures/ignored |
| `cargo build --workspace` | Passed |
| `cargo check --locked -p office-docx --target wasm32-unknown-unknown` | Passed |
| `git diff --check` | Passed |

The test total is 166 existing core tests, 21 native WASM-adapter tests, 4 core
doctests, and 65 new DOCX tests (21 ZIP, 18 XML, 22 OPC, 4 source integration).
The new tests cover resource bounds before allocation, ZIP integrity and streamed
descriptors, namespaces/encodings/entities, package relationships/content types,
Strict and Transitional identification, relocated main parts, unsupported feature
inventory, and exact owned source retention. Both handcrafted fixtures are
checked in; no self-roundtrip is presented as Office interoperability evidence.

From `apps/writer`, all passed against the unchanged application:

| Exact command | Result |
| --- | --- |
| `npm run typecheck` | Passed |
| `npm run lint` | Passed |
| `npm test` | 20 passed |
| `npm run test:build` | 2 passed |
| `npm run build` | Rust WASM release + TypeScript + production Vite build passed |
| `npm run test:wasm` | 8 passed using actual generated WASM, no engine mock |
| `npm run test:browser` | 27 passed in real Chromium against production preview |

Browser regression coverage includes editing, Unicode/emoji, mixed formatting,
selections, undo/redo, paste, synthetic/CDP IME, native import/export, IndexedDB
save/reopen/reload, complete browser restart persistence, corruption, storage
failures and conflicts. A synchronous typing measurement on 11,840 characters in
50 paragraphs recorded p50 2.4 ms, p95 4.2 ms, maximum 5 ms over 30 events. This
measures engine/reconciliation work, not operating-system input-to-paint latency.

Additional development checks included focused module tests, offline native/WASM
checks, fixture generation with `python3 tests/fixtures/docx/generate.py`, and
target-filtered Cargo metadata/license review. An independent temporary audit
program ran 5,000 deterministic byte-mutation cases before and after the final
descriptor correction: 4,929 structured errors, 71 accepted variants, zero
panics. This is smoke testing, not a fuzzing/security certification. Its corrupted
descriptor reproduction now returns `InvalidArchive`.

GitHub Actions retains the three-OS engine matrix and browser regression job;
the browser job additionally checks the DOCX adapter's WASM target. Remote CI is
a separate result to inspect on the review branch's final commit; the table
above records actual local execution, not an inferred CI outcome.

## Corrections and testing limits

During development, failing tests and independent review exposed missing
namespace/reference checks, an incorrect Strict URI, rejected canonical absolute
targets, MIME metadata allocation amplification and unchecked streamed ZIP
descriptors. Each was corrected with regression coverage before final checks.
No remaining confirmed critical audit finding was identified.

Initial sandbox attempts at the production build and build-script tests failed
with subprocess `EPERM`; the original commands subsequently passed with local
execution permission. No final check is counted as passed from a blocked run.
All-platform offline Cargo metadata initially lacked a cached target-specific
crate; Linux-filtered metadata succeeded and all 17 new locked package versions
were independently checked against the reviewed graph and official metadata.

Not performed: native Windows/macOS execution in this Linux workspace, other
browser engines, manual OS IME/assistive-technology testing, Word/LibreOffice
interoperability, full OOXML schema validation, formal fuzzing, DOCX browser
integration, editable semantic import, reconstructed export or post-edit source
fidelity. WASM compilation of `office-docx` does not expose it through browser
bindings. Original-source persistence is not implemented. Full normative archive
downloads and OSV advisory access returned HTTP 403; research used the primary
sources and manual RustSec review documented in [DOCX.md](DOCX.md).

Step 5B must define the editable subset, source association/persistence and
edit-aware preservation contract without introducing DOCX assumptions into the
shared model. No additional document formats, merges or deployments are part of
this subphase.
