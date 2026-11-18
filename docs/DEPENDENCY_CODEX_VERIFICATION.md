# Dependency verification — evidence pass 2

Date: 2026-11-18. Candidate: `chore/graphe-open-source-prep`, based on
`7bfccd0`. **Not cleared for public release.** No application dependencies,
lockfiles, licenses, main-branch code, or deployment settings changed.

## Results

| Check | Executed result |
| --- | --- |
| `node scripts/audit-dependencies.mjs` | Passed; Cargo metadata for WASM and Linux, 123 external Rust packages, 212 npm lock records, three production npm packages |
| `python3 scripts/audit-license-texts.py` | Passed; all 123 registry archives match their pinned lockfile SHA-256 checksums; 121 contain license/notice/authorship text evidence; two unresolved text gaps |
| `npm ci` in `apps/writer` | Passed; 182 packages installed; optional platform records explain the difference from 212 lock records |
| `npm audit --json` | Exit 1; 13 high-severity development-package findings, propagating the picomatch ReDoS advisory [GHSA-fc5q-39xc-vjpf](https://github.com/advisories/GHSA-fc5q-39xc-vjpf); this scan reports `fixAvailable: false` for picomatch |
| `npm audit --omit=dev --json` | Exit 0; zero production npm vulnerabilities reported |
| `/tmp/graphe-audit-tools/bin/cargo-audit audit --json --file Cargo.lock` | Exit 0; zero vulnerabilities and zero warnings |
| `cargo build --locked --release --target wasm32-unknown-unknown -p office-wasm` | Passed; selected release-mode WASM crate compiles |
| `python3 crates/office-layout/fonts/verify.py` | Passed; pinned font catalog verification |
| `node scripts/check-public-release.mjs` | Passed its metadata checks; 103 tracked documents/images still require provenance/privacy review; this is not publication clearance |

The tooling used Node 24.22.1, Rust 1.100.0 and cargo-audit 0.23.0.
cargo-audit was installed outside the repository with
`cargo install cargo-audit --version 0.23.0 --locked --root /tmp/graphe-audit-tools`.
Its RustSec database contained 1,533 advisories at commit
`472a4c9a785030434f15d7da84964d60fbe789ab`, last updated
2026-11-18T10:14:42+01:00. Results are a dated scan, not a security guarantee.

## Corrected WASM scope

The original inventory seeded all workspace roots and included host procedural
macros. It overcounted browser-runtime dependencies as 108. The corrected script
uses the actual `office-wasm` root:

```sh
cargo tree --locked --target wasm32-unknown-unknown -p office-wasm --edges normal --prefix none --format '{p}'
cargo tree --locked --target wasm32-unknown-unknown -p office-wasm --edges normal,no-proc-macro --prefix none --format '{p}'
```

There are **106 external normal-dependency identities including host macro
inputs**, and **91 excluding procedural macros and their exclusive inputs**.
These are source dependency graphs, not proof that every crate contributes bytes
after optimization. Native reachability in the metadata inventory remains a
workspace graph, not an inventory of one native executable.

The raw release WASM is 6,828,831 bytes, SHA-256
`f0f3baa3c30ffdd4b41027371a36db1d35926724c935163cb6bee61dc18dd375`.
This pass did not build or inspect a final Vite/wasm-bindgen distribution, map
post-LTO code contributions, or certify the production asset bundle.

The six metadata-unresolved lock entries are not six established unlicensed
packages: they are absent from these target metadata resolutions. Their exact
published manifest declarations and text hashes are in the archive evidence.

## Actual license text evidence

The new script verifies cached `.crate` archive checksums before reading any
text. It reads archives without extracting files, records published declarations,
authors, source commits and text hashes, and emits a **draft** browser-runtime
notice bundle. It fails on checksum mismatch or oversized evidence, but missing
archives/texts remain explicit unresolved statuses, not automatic violations.
Local negative-fixture checks passed: missing license text remains unresolved,
while a checksum-mismatched archive exits 2. Node syntax, Python AST parsing and
`git diff --check` also passed.
CI now collects the same evidence, including installed production npm texts.
A clean runner may have fewer cached target-specific archives; check its summary
before claiming all-lock coverage.

All three installed production npm packages contain the MIT license text:
React 19.3.1, React DOM 19.3.1, Scheduler 0.28.0. Their individual LICENSE files
are 1,088 bytes, SHA-256
`da6d3703ed11cbe42bd212c725957c98da23cbff1998c05fa4b3d976d1a58e93`.

`r-efi` includes its full licensing material in `AUTHORS`; it is not a third
missing-text package. It is outside this browser-runtime graph. Its alternative
`MIT OR Apache-2.0 OR LGPL-2.1-or-later` declaration does not automatically impose
LGPL. Compound expressions such as `(MIT OR Apache-2.0) AND Unicode-3.0` retain
the additional Unicode obligations; a permissive alternative does not erase
an `AND` requirement. File presence alone does not establish compliance.

### RTF packages: unresolved, not a confirmed violation

Both `rclip-core@0.1.0` and `rclip-rtf@0.1.0` are in the browser-runtime graph.
Their checksum-verified archives declare `MIT OR Apache-2.0`, name Felix Schütt
as author, and contain no standalone license/notice text. `rclip-core` also has
a README license declaration. The exact upstream workspace manifests confirm
the inherited declaration at:

- [core source commit](https://github.com/fschutt/rich-clipboard/blob/2be8cabe96655f98bec3d5dc9781f1713cd46246/Cargo.toml)
- [RTF source commit](https://github.com/fschutt/rich-clipboard/blob/371c7203faeb3a5f13604a574a7a2e37bba48226/Cargo.toml)

The retrieved workspace manifests both hash to
`74dc6f278438ec3f781e970478428be6f22c3647e0e4dddfb3b03baedd4dbfbc`.
Existing Graphe notices cannot substitute for verified upstream attribution.
Resolve the intended distribution notices with upstream/licensing review before
clearance. No upstream contact was made and no permission was invented.

## Retained evidence and outstanding work

[The dated evidence directory](dependency-evidence/2026-11-18/) contains the
inventory, actual archive/text hashes, draft Rust/npm notices, upstream manifest
evidence and raw RustSec/npm reports. Lockfile hashes bind the snapshot to this
candidate. The draft notice is an input to release review, not a finished legal
notice or SPDX SBOM.

Still required before public release:

- Resolve the RTF text/attribution gaps; review all applicable license obligations
  and consolidate approved notices for the actual distribution.
- Resolve or explicitly assess the npm development-tool vulnerability exposure.
- Review final distributed assets, source provenance, fixture/privacy concerns,
  repository history and the separate font/source licensing requirements.
- Obtain the owner's release approval. Main and the private production app are
  unaffected by this preparation work.

`cargo deny` was not run: no reviewed deny policy exists in this candidate.
No fresh product/browser/device regression suite or Windows/macOS build was run
for these evidence-tool/documentation changes. The release WASM compile above
is not a frontend build or browser test. Hosted GitHub Actions results must be
checked separately; local execution does not establish a hosted CI pass.

### Hosted CI observation

[CI run 44288789099](https://github.com/boleydavid7-gif/stuff/actions/runs/44288789099)
for evidence commit `1a9a4b8` completed with failure. Its macOS, Windows, Linux
and browser jobs all expose no execution steps and no logs through the GitHub
API. This is not evidence that their test commands executed or failed. No
infrastructure cause can be established from those fields alone. The connector
lists pull-request runs only; the separate branch-triggered evidence workflow's
status is not established by this query. Hosted coverage remains unverified.
