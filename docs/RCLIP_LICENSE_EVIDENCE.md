# rclip 0.1.0 licensing evidence

Reviewed 2026-11-18 on `chore/graphe-open-source-prep`.
**Technical evidence assembled; author confirmation and release approval are not obtained.**
The owner authorized the upstream inquiry on 2026-11-18. After the automated
submission was blocked by integration permissions, the owner submitted
[upstream issue #1](https://github.com/fschutt/rich-clipboard/issues/1).
It is open with no comments at the latest check on 2026-11-18.
This supplements the earlier missing-text finding; it does not relabel a missing
upstream file as present or grant commercial/public release clearance.

## What is established

| Package | Exact published source | Published archive SHA-256 |
| --- | --- | --- |
| rclip-core 0.1.0 | [2be8cabe](https://github.com/fschutt/rich-clipboard/tree/2be8cabe96655f98bec3d5dc9781f1713cd46246/crates/rclip-core) | `6414ce752a0d18ee3d000b6c76fccbc30e90b93c2a8c1e7fdd855ab9b7ea0af6` |
| rclip-rtf 0.1.0 | [371c7203](https://github.com/fschutt/rich-clipboard/tree/371c7203faeb3a5f13604a574a7a2e37bba48226/crates/rclip-rtf) | `e64929d6244eea54b6ae4276d5724ae8833c269b8156dd63d48b4f13d39a78ed` |

- Both exact archives match Cargo.lock. Their normalized manifests declare
  `MIT OR Apache-2.0`. Their original manifests inherit that same declaration
  from the exact upstream workspace manifest. This is a positive package-license
  declaration, not merely a guess from a repository badge.
- Both declare author `Felix Schütt <felix.schuett@maps4print.com>`.
  Author metadata is preserved as author metadata; no copyright owner/year has
  been inferred or invented.
- All published Rust source/test files, original manifests and READMEs match
  their recorded upstream commits byte for byte: 16 files for core, 13 for RTF.
  Cargo-generated normalized manifests, VCS metadata and lockfiles are not
  represented as verbatim upstream source files.
- An unshallow clone exposes 31 reachable upstream commits, through
  `371c7203faeb3a5f13604a574a7a2e37bba48226`. No historical filename matches
  LICENSE/LICENCE/COPYING/COPYRIGHT/NOTICE/AUTHORS with the supported suffixes.
  This says nothing about inaccessible/deleted refs or unpublished material.
- Searches of published Rust/Markdown/TOML for copyright, SPDX identifiers,
  license-grant headers and all-rights-reserved notices found none. Root and
  core READMEs separately declare the dual license. This is a bounded notice
  search and source-identity check, not an independent authorship/similarity audit.

## Distribution evidence

Graphe already selects the **Apache-2.0 alternative** for these two packages in
[format notices](../crates/office-formats/THIRD_PARTY_NOTICES.txt). This review
preserves that existing choice; it does not relicense Graphe or either package.
The existing attributed license block matches the official Apache license terms
through END OF TERMS AND CONDITIONS, ignoring whitespace. Its optional
application appendix is omitted from that block; an untouched full reference,
including the template appendix, is retained in the evidence directory.

The reference was obtained from the [Apache Software Foundation website's
source repository](https://github.com/apache/www-site/blob/01b1be9fbc5cd93b6794f5653a58b9b863807f84/content/licenses/LICENSE-2.0.txt),
not from rclip. Its SHA-256 is
`cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`.
Direct shell access to apache.org returned HTTP 403; the GitHub connector
retrieved the official ASF source successfully. The appendix placeholders are
part of that reference, not a copyright assertion about rclip.

The [Cargo manifest specification](https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields)
defines SPDX package-license declarations and alternatives.
[Apache-2.0 section 4](https://www.apache.org/licenses/LICENSE-2.0)
requires supplying the license, marking modifications, retaining applicable
source notices, and carrying an upstream NOTICE when one is supplied. No
upstream NOTICE was located. The absence of such a file does not justify
inventing attribution or assuming that the explicit license declaration is void.
The standard reference text is not itself new permission from rclip's author.

The app already imports the format notices as a Vite URL asset and exposes it
in About. No runtime/deployment change was needed. Verifying the final emitted
asset and complete distribution remains part of the broader release audit.

## Reproducible local checks

[Saved evidence](dependency-evidence/rclip-0.1.0/evidence.json) records hashes,
source comparisons and notice-search results. Adjacent files retain the exact
package manifests, READMEs, VCS records, upstream workspace manifests and
reference license. Historical dated audit evidence remains unchanged.

Run `python3 scripts/verify-rclip-evidence.py` to check the retained snapshots,
current lockfile identities, existing selected license/attribution and official
reference terms. This is an offline consistency check; it does not refetch
upstream or repeat the historical scan. Any dependency change requires a fresh
review, not an automatic exception for all future rclip releases.

Executed during this pass:

- `git clone --quiet https://github.com/fschutt/rich-clipboard.git /tmp/graphe-rclip-source-review`
- `git rev-parse --is-shallow-repository`, `git rev-list --all --count`,
  `git log --all --name-only --format=` in that clone.
- SHA-256 verification and `git show <published-commit>:<source-path>` byte
  comparisons for the two cached published archives; all 29 comparisons passed.
- `python3 scripts/verify-rclip-evidence.py` passed.
- `python3 scripts/test_verify_rclip_evidence.py`: **6 passed** (valid evidence,
  corrupt reference, edited snapshot, changed dependency, missing
  attribution and conflicting source evidence).
- `git diff --check` passed.

No fresh vulnerability scan, product build, browser/device test or upstream
maintainer response is claimed by this focused documentation/tooling pass.

## Remaining decision and prepared upstream inquiry

The technical collection and license-reference supply are addressed. The
upstream packaging omission remains a fact. A release reviewer must decide
whether the explicit declarations and supplied Apache reference/attribution
satisfy the release policy, or require confirmation from upstream. This pass
cannot certify copyright ownership on the author's behalf.

The following prepared inquiry was submitted by the owner as
[upstream issue #1](https://github.com/fschutt/rich-clipboard/issues/1).
It discloses no Graphe/private-repository information:

> Title: License texts and attribution for rclip-core / rclip-rtf 0.1.0
>
> Thank you for rich-clipboard. We are reviewing the published rclip-core 0.1.0
> and rclip-rtf 0.1.0 packages. Their normalized manifests declare
> MIT OR Apache-2.0 and name Felix Schütt as author. The recorded source commits
> are 2be8cabe96655f98bec3d5dc9781f1713cd46246 (core) and
> 371c7203faeb3a5f13604a574a7a2e37bba48226 (RTF).
>
> We could not locate standalone license or NOTICE files in these archives or
> the repository. Could you confirm that both 0.1.0 releases are available under
> either declared license, including Apache-2.0 for redistribution, and provide
> the applicable license texts and copyright/attribution notices? Please also
> identify any additional third-party notices applicable to those versions.
> Adding LICENSE-MIT and LICENSE-APACHE to the repository and future package
> archives would make automated attribution checks easier. Thank you.


### Authorized submission attempt — 2026-11-18

The owner explicitly approved posting the inquiry above. A duplicate search for
`repo:fschutt/rich-clipboard is:issue license` returned no issues. GitHub issue
creation returned HTTP 403, `Resource not accessible by integration`. The local
GitHub CLI also reports invalid configured credentials. No upstream issue or
message was sent by the assistant. This was an access limitation, not a
rejection of the licensing inquiry by the maintainer.

### Owner submission verified — 2026-11-18

The owner supplied https://github.com/fschutt/rich-clipboard/issues/1. Read-only
GitHub API checks confirmed the expected inquiry, opened by `boleydavid7-gif`,
with state `open` and zero comments. No maintainer confirmation has been
received at this check. An open issue is evidence of the request, not agreement
with its contents. Record and review any response before changing the release
disposition; no ongoing automatic monitoring is configured.
