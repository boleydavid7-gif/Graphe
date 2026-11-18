# Graphe dependency licensing audit — evidence pass 1

**Date:** 2026-11-18  
**Scope:** The complete Graphe application and Rust Office Engine, against
`chore/graphe-open-source-prep`, for an intended MPL-2.0 source release.  
**Status:** **OPEN — not commercially or publicly cleared.**  
**Change policy:** No dependency was added, upgraded, removed, or relicensed by this report.

## Follow-up verification

The initial-pass limitations below are historical. See
[Codex evidence pass 2](DEPENDENCY_CODEX_VERIFICATION.md) for executed Cargo
graphs, checksum-verified archive/license evidence, fresh advisory scans, and
remaining release blockers. Neither pass grants publication clearance.

## Summary

| Category | Checked evidence | Assessment |
| --- | --- | --- |
| Cargo lockfile | 128 package entries: five Graphe workspace crates and **123 external, checksummed crates.io registry packages** | Exact source/version/checksum inventoried; licenses are **not encoded in Cargo.lock**, so runtime license clearance still requires installed registry metadata and the actual license texts |
| Browser npm lockfile | 212 resolved package records; all contain a `license` field | No blank npm license metadata observed |
| Browser production npm packages | `react@19.3.1`, `react-dom@19.3.1`, `scheduler@0.28.0` | All three declare `MIT`; relatively low license-compliance risk if their required notices are preserved |
| npm development/build packages | 209 entries, including optional OS/CPU binary packages | 12 Lightning CSS entries declare `MPL-2.0` (all flagged as development); `caniuse-lite` declares `CC-BY-4.0` (development). Build-only use does not automatically impose those licenses on Graphe source, but redistribution of toolchain/data requires separate review |
| Rust third-party notice coverage | Existing subsystem notice files: `crates/office-core/THIRD_PARTY_NOTICES.txt`, `crates/office-layout/THIRD_PARTY_NOTICES.txt`, `crates/office-formats/THIRD_PARTY_NOTICES.txt` | **19/123** external Rust package-and-version identities have an explicit exact-version string match in one or more of these three notices; **104/123 do not**. This is only a **presence search**, neither proof of 104 missing legal notices nor proof of completeness for the 19 matches. The wider project documentation covers more dependencies as research, not as consolidated release notices |
| Direct Rust dependencies | Workspace manifests include Serde, ZIP/XML, Parley/Skrifa, PDF writer, PNG/JPEG/EXIF libraries, Rust/WASM binding code and RTF tokenizer | No complete third-party office engine imported; specific low-level crates still require direct/transitive license review |
| RTF tokenizer | `rclip-rtf@0.1.0` + `rclip-core@0.1.0` | Both inherit `MIT OR Apache-2.0` through published upstream workspace metadata; pinned Git source trees contain **no standalone LICENSE/NOTICE file**. Requires exact distributed-package and author-rights review, not automatic rejection |
| Fresh advisories and built artifact | Not run in this evidence pass | **Unverified.** Historical audit statements in `docs/DEPENDENCIES.md` cannot be carried forward as present-day clearance |

## Findings

### D-01 — Incomplete compiled Rust/WASM license graph (**release blocker**)

`Cargo.lock` identifies 123 external registry packages with immutable checksums,
but does not encode SPDX license expressions, copyright notices, or whether a
particular package ends up in the emitted WebAssembly binary. The three
subsystem notices and the dated `docs/DEPENDENCIES.md` are partial inputs.
It is not defensible to mark all 123 cleared without inspecting exact upstream
manifests/license files and the dependency graph selected by the final build.

**Action:** In a checkout with Cargo available run
`node scripts/audit-dependencies.mjs`. It invokes `cargo metadata --locked`
for `wasm32-unknown-unknown` and the native build platform, then writes
`artifacts/dependency-audit/dependency-audit.json` and `.md`.
The script intentionally marks valid SPDX declarations **declaration-only**,
not legally cleared. Generate a complete release notice with `cargo-about`
or an equivalent library once upstream-license text and reachability are checked.

### D-02 — RTF source copyright notice evidence (**release review pending**)

**Follow-up:** [rclip evidence dossier](RCLIP_LICENSE_EVIDENCE.md) completes
the technical source/declaration/reference-text collection. No author
confirmation has been received; final redistribution approval remains pending.

`rclip-rtf 0.1.0` and `rclip-core 0.1.0` inherit `MIT OR Apache-2.0` from
`[workspace.package]` in the published `fschutt/rich-clipboard` source.
The commits recorded in Graphe's prior audit,
`371c7203faeb3a5f13604a574a7a2e37bba48226` and
`2be8cabe96655f98bec3d5dc9781f1713cd46246`,
each have a source tree with **no file path containing `LICENSE`, `LICENCE`,
`COPYING`, or `NOTICE`**. The upstream workspace names Felix Schütt as
author and uses the dual-license expression.

**Interpretation:** This does **not** establish a copyright violation or
incompatibility with MPL-2.0. A manifest SPDX declaration is useful
permission evidence; missing accompanying text and incomplete published
attribution still need attention for a robust redistributed source/build.

**Action:** Retrieve the exact registry release archives and the pinned
repository sources, review copyright comments/provenance, confirm
the intended Apache-2.0 or MIT permission and appropriate notices with
upstream/legal guidance, then document the conclusion. If the license terms
remain genuinely uncertain, replace this narrow tokenizer with an
independently verified permissively licensed alternative. **Do not rewrite
the parser solely on the assumption that the dual-license declaration is invalid.**

### D-03 — npm license classifications (**review before packaging**)

All 212 npm lock entries have a license declaration. License counts, including
build tools and optional platform binaries:

| Lockfile license expression | Count |
| --- | ---: |
| MIT | 158 |
| Apache-2.0 | 20 |
| BlueOak-1.0.0 | 2 |
| ISC | 10 |
| CC-BY-4.0 | 1 |
| BSD-2-Clause | 6 |
| BSD-3-Clause | 2 |
| MPL-2.0 | 12 |
| 0BSD | 1 |
| **Total** | **212** |

Only three entries lack the lockfile `dev` flag, and all are MIT. There is no
GPL/AGPL-labeled npm entry in this lockfile. **Do not extrapolate this to
Rust packages or assert that an npm lockfile accurately describes every
resource in the final production output.**

**Action:** Verify whether any build-generated code/data is redistributed,
retain attribution, and inspect the actual emitted JS, font, WASM, and
notice assets. Preserve proper notices for React, Scheduler and any other
copied/bundled code. For offline toolchain distribution, review build
dependencies separately.

### D-04 — Build evidence and fresh advisory results (**release blocker**)

The initial GitHub Actions dependency-evidence run was reported as failed
without any recorded execution steps, similar to previously unallocated
CI jobs. **No Cargo metadata or audit output was collected from that run.**
The user's reported green build is not treated as proof of a complete license
or advisory scan unless it includes the relevant checks and output.

**Action:** Use a working runner or Codex/Linux checkout to run:

```sh
node scripts/audit-dependencies.mjs
node scripts/check-public-release.mjs
cd apps/writer && npm ci && npm audit --audit-level=moderate
cd ../..
# Install/execute Cargo audit and denial tools in a controlled environment.
cargo metadata --locked --format-version 1 --filter-platform wasm32-unknown-unknown
cargo audit
cargo deny check licenses advisories sources
```

Record tool versions, advisory database revision, exact findings and any
dispositions. `cargo deny` requires an intentionally reviewed `deny.toml`
configuration before it can serve as a policy gate. Install the tool and
configuration deliberately; the commands above are **not claimed executed**.

### D-05 — Reproducible inventory and reporting (**complete for lockfile identity only**)

The committed [CSV inventory](DEPENDENCY_LOCK_INVENTORY.csv) records all
123 external crates, exact versions, registry source identifiers and checksums,
whether their identity appears verbatim in the three existing notice files,
and whether upstream license metadata and files have been checked.
Every entry lacking verified metadata is expressly labeled as requiring review.

The repeatable `scripts/audit-dependencies.mjs` distinguishes lockfile
identity, upstream declared license, graph reachability and notice-presence
hints. Neither the CSV nor the generated metadata is a release-ready copyright
notice by itself.

## Clearance policy

A package is only **ready for distribution** after:

1. Its precise resolved name, version, checksum and source are verified.
2. Its applicable license expression and permission evidence are obtained
   from the exact upstream release; dual-license choice is recorded.
3. Its actual license/copyright notice obligations are incorporated in the
   release distribution when required.
4. The package's normal/build/dev/target role is known, and no known
   problematic license or advisory remains unreviewed.
5. Exact release artifacts and all public notice URLs are inspected, with
   tests and documented provenance.

**Decision:** Dependency audit **in progress; no release clearance yet**.
Original Graphe code remains staged for MPL-2.0 publication, and the current
private `main` branch is not changed by this audit.
