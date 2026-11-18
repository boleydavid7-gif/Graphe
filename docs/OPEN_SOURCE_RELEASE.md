# Graphe open-source release preparation

**Status:** NOT PUBLIC / NOT CLEARED. This page is the publication gate for a proposed
**complete Graphe app and independent Rust Office Engine** open-source release under MPL 2.0.
It does **not** include the wider ProairetOS source.

**Preparation branch:** `chore/graphe-open-source-prep` in the private
`boleydavid7-gif/stuff` repository. Do not merge these license changes into the
currently proprietary production `main` branch, make `stuff` public, tag an
open-source release, or deploy the staging branch before the gates below pass.

## Completed in the private preparation branch

- [x] Stage the **complete canonical Mozilla Public License 2.0** in the root `LICENSE`.
- [x] Mark the workspace, five Rust crates and browser package as `MPL-2.0`.
- [x] Update the README and engineering guide to distinguish the staged license
      from the private, proprietary production branch.
- [x] Add `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`,
      `GOVERNANCE.md`, `TRADEMARKS.md` and `NOTICE.md`.
- [x] Review the current top-level Git tree for potentially sensitive binary/test
      artifacts, and inspect the provenance notes of the official Open XML SDK fixtures.
- [x] Prepare a practical process for a **new, clean-history** public `graphe` repo.

## Blocking checks before the public source grant

- [ ] **Copyright authority and provenance:** confirm the rights to distribute
      every original source file (including AI-generated or imported portions),
      confirm any other contributor/employer rights, and independently examine
      third-party snippets. A 100-commit author scan showed a small set of
      identities but cannot prove ownership of the entire repository history.
- [ ] **Complete Rust dependency clearance:** identify each released-WASM/native
      dependency and its exact license terms; generate accurate third-party
      notice texts and an SBOM from the *release* artifacts. Keep historical
      `docs/DEPENDENCIES.md` and incomplete subset notices separate. Resolve
      the `rclip-rtf`/`rclip-core` upstream license-file question.
- [ ] **Asset provenance:** verify all Git-tracked SVG/PNG logos, image samples,
      screenshots, Office documents, RTF/ODT fixtures and other binaries.
      Non-public/restricted fixtures must be removed or replaced **before**
      public export. Official Microsoft Open XML SDK fixture license evidence is
      in `tests/fixtures/docx/relationships/README.md`; check the exact imported
      source/version and all remaining fixtures as well.
- [ ] **Font licensing:** recheck the canonical 42 TTF hashes, 21 OFL family
      notices, reserved font names and dist notice availability, using
      `python3 crates/office-layout/fonts/verify.py`.
- [ ] **Code copyright notices:** add the MPL Exhibit A notice to each intended
      newly MPL-covered source file, or to an appropriate directory-level
      LICENSE/notice location where recipients would expect it, in accordance with
      Mozilla's guidance. Preserve historical third-party notices.
- [ ] **Run the metadata preflight:** after cloning or creating the snapshot, execute
      `node scripts/check-public-release.mjs`. It checks current license metadata,
      font notice links, obvious key patterns and suspicious paths. It is a
      **supplement**, not a credential audit or publication clearance.
- [ ] **Privacy/secrets in entire Git history:** inspect all commits, tags,
      unusual GitHub assets, binary metadata, GitHub Actions logs and **all 512
      tracked files in the examined baseline**. Do not rely on a filename scan or
      GitHub code search alone. Run a local full-history scanner such as `gitleaks
      git --redact`; manually review matches and rotate exposed credentials.
      A clean *new-history* snapshot mitigates publication of the old commit
      history, but it does not prevent secrets in current files from being exposed.
- [ ] **Public snapshot review:** include the original app+engine source, lockfiles,
      needed fixtures, canonical fonts/notices and public docs; exclude anything
      with non-public distribution rights. Do not publish the private repo's
      current branch/tag/PR histories, secret-dependent configuration, or old
      Actions logs. If excluding a fixture breaks tests, generate and update tests
      to use an independently owned synthetic replacement first.
- [ ] **Build and checks:** run the repository's Rust fmt/Clippy/test/build;
      `npm ci`, `npm run typecheck`, `npm run lint`, `npm test`,
      `npm run test:wasm`, `npm run test:build`, `npm run build` and
      appropriate browser tests. Verify the final production JS/WASM/font
      bundle and the `About Graphe` links to **every** required notice.
- [ ] **Fresh dependency advisories:** run `npm audit`, `cargo audit`,
      `cargo deny` and/or comparable license/SBOM scanners against the pinned
      release lockfiles. Record versions, dates, reachable severity and
      dispositions; old reported test results do not count as a fresh run.
- [ ] **Brand/fork usability:** keep a clear trademark boundary; ensure an
      outside contributor can build and run the source, and a differently
      branded fork can replace the official logos/manifest assets.
- [ ] **Documentation accuracy:** remove stale assertions that Graphe has no
      license or is proprietary **from the candidate's current-position
      documentation**; retain accurately marked dated historical reports where
      valuable. Confirm that the license scope includes the frontend + Rust
      engine but excludes third-party files and brand/trademark permission.
- [ ] **Maintainer review:** examine the final clean snapshot with human review
      and seek professional legal guidance where copyright, patents,
      redistribution rights or trademarks remain uncertain.

## Proposed clean-history publication

GitHub's current connector permits commits/branches/PRs but **does not provide
the create-repository or repository-visibility actions needed to publish a
separate GitHub repo**. Publishing therefore requires a GitHub account operation
from a tool with those permissions, GitHub's website, or the authenticated
`gh` command-line client. Do not claim a public repository exists until it does.

After **all** above items are checked, and after the owner approves the final
public snapshot, use a trusted local checkout of the approved commit to build
a *new directory without the original Git history*:

```sh
mkdir -p ../graphe-public
git archive <approved-open-source-commit> | tar -x -C ../graphe-public
cd ../graphe-public
# Inspect contents; remove/replace only files already approved for exclusion.
# Recheck secrets, licenses, assets and tests before continuing.
git init -b main
git add .
git commit -m "Initial open-source Graphe release (MPL-2.0)"
gh repo create boleydavid7-gif/graphe --private --source . --remote origin --push
```

These commands are **instructions, not actions already executed**. The new repo
must start **private**, so the public snapshot can be reviewed before GitHub
visibility is changed. Do **not** use a fork, mirror-push or copy of the old
`.git` directory, or upload the complete older repo history.

After the new private repo is reviewed and its first checks pass:

1. Enable private vulnerability reporting, Issues, relevant branch protection
   and repository descriptions/topics.
2. Confirm public build instructions and font/format notices work without
   private ProairetOS services, inaccessible tokens or internal links.
3. Change **only the new `graphe` repository** to public with deliberate owner
   authorization. Keep `stuff` private.
4. Tag a reviewed initial release and link the source tree from the published
   Graphe application (where applicable).

## Audit evidence from initial review

- Git tree inspection at the then-current `main`: **512 tracked blobs**
  (plus directories). Binary formats included many DOCX/ODT/RTF fixtures,
  42 licensed TTFs, UI screenshots and icons.
- The root `BDB10EA5-0031-4DD8-92FA-63C7ACE2A96D.png` is byte-identical
  (same Git blob SHA) to `apps/writer/public/graphe-icon.png`. The redundant
  root copy can be excluded from a clean release.
- A limited default-branch GitHub keyword search found no `API_KEY`,
  `secret_key`, `private_key`, `client_secret`, `SUPABASE` or
  `CLOUDFLARE_API_TOKEN` code-search matches. Such a search is **not**
  comprehensive and is not a replacement for local current-tree, dependency,
  image/document metadata and complete-history scanning.
- The official Word SDK sample fixtures have MIT provenance documented in
  `tests/fixtures/docx/relationships/README.md`, with a retained upstream
  license file. Other fixture classes are not automatically cleared by this.
- A sample of the 100 most recent commits showed a limited set of author
  name/login variants, not proof of exclusive ownership or a full-history audit.

## References

- [Mozilla's official MPL 2.0](https://www.mozilla.org/en-US/MPL/2.0/)
- [Mozilla's MPL FAQ and source-file notice guidance](https://www.mozilla.org/en-US/MPL/2.0/FAQ/)
- [GitHub guidance on sensitive data in Git history](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/removing-sensitive-data-from-a-repository)
- [Commercial licensing and IP review](COMMERCIAL_LICENSING_AUDIT.md)

**Gate decision: HOLD.** Nothing in this document authorizes making any repo
public or changing the production release license.
