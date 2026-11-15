# Phase 2, work area 5: hosted CI investigation

This records observed evidence, not a claim that unavailable runners or browser
engines passed. No workflow job or test is removed or weakened. No merge,
deployment, account-setting change or unrelated application change is authorized.

## Hosted failure stage

The new Phase 2 push at `6f0bc4191c2c6db12594fd18dbca0a15d70f9c60`
created [CI run 43702226799](https://github.com/boleydavid7-gif/stuff/actions/runs/43702226799)
on 2026-11-15 at 10:03:44 UTC. It completed with failure at 10:03:48 UTC.

| Job | GitHub job/check ID | Allocated runner | Executed steps | Check annotations |
| --- | --- | --- | --- | --- |
| Rust checks, Ubuntu | `132775633216` | `runner_id=0`, empty name | None | 2 |
| Rust checks, Windows | `132775633199` | `runner_id=0`, empty name | None | 1 |
| Rust checks, macOS | `132775633202` | `runner_id=0`, empty name | None | 2 |
| Writer browser integration | `132775633018` | `runner_id=0`, empty name | None | 2 |

The same pre-allocation pattern appears on latest audited PR #17 head `e501d76`
in [run 43686429644](https://github.com/boleydavid7-gif/stuff/actions/runs/43686429644),
and on independently advanced main `efde8a8` in
[run 43696141723](https://github.com/boleydavid7-gif/stuff/actions/runs/43696141723).
This confirms that test commands did not execute. It does **not** establish
billing, a source defect, a GitHub incident or a particular repository setting
as the cause. Pre-allocation workflow/configuration errors remain possible until
the actual annotation messages are available.

The repository workflow retains Rust checks on `ubuntu-latest`, `windows-latest`
and `macos-latest`, plus the complete Writer pipeline and Chromium/Firefox/WebKit
installation and execution. Local Linux results are separate evidence and do
not substitute for these unexecuted hosted jobs.

## Diagnostic access and exact limitation

The connected GitHub fetch tool can read Actions runs/jobs and this approved
commit subresource:

```text
GET /repos/boleydavid7-gif/stuff/commits/6f0bc4191c2c6db12594fd18dbca0a15d70f9c60/check-runs
```

It returned the annotation counts above, but all four Actions checks have null
output title, summary and text. The standalone `check-runs/{id}` and
`check-runs/{id}/annotations` endpoints are rejected by the connector with
`400 INVALID_ARGUMENT: not an allowed ... endpoint`. That is a connector endpoint
limitation, not evidence of inadequate repository permissions or a billing cause.
The Writer job-steps tool returned `[]`; its log fetch returned 404 `BlobNotFound`,
consistent with there being no started-runner log to inspect.

The same read-only CLI request was retried outside the filesystem/network
sandbox, with proxy settings unchanged:

```sh
gh api repos/boleydavid7-gif/stuff/check-runs/132775633018/annotations --jq '.[] | {annotation_level,message}'
curl --head --max-time 15 https://api.github.com
```

The first exited 1 with `Get ...: Forbidden`. The unauthenticated public-header
probe exited 56 with `CONNECT tunnel failed, response 403`, from the environment's
Envoy proxy. This establishes a direct-network block before GitHub authentication.
`gh auth status` also reported an invalid token, but that message cannot establish
credential validity while its API connection is blocked. No token or other secret
was printed, changed or extracted; no proxy restriction was bypassed.

## Precise owner/infrastructure action

Open the linked failed run in GitHub, select a failed job and read its annotation
messages. Share the exact messages so the indicated cause can be corrected.
Alternatively, enable the connected tool's read support for check-run annotations,
or permit HTTPS CONNECT to `api.github.com` with an authenticated account having
Checks read and Actions read access. Reconnecting repository permissions alone
does not change the current connector endpoint allowlist.

After the identified account/repository/workflow restriction is corrected, rerun
all failed jobs on the final Phase 2 commit. Do not accept only local Linux/Chromium
results as macOS, Windows, Firefox or WebKit passes. An annotation identifying a
workflow problem should lead to that specific correction; this investigation
does not justify speculative billing or workflow changes.

Reference: [GitHub check-run annotations API](https://docs.github.com/en/rest/checks/runs#list-check-run-annotations).

## Branch and external-check context

PR #17 remains open, draft and unmerged at `e501d76`; Phase 2 preserves that audited
starting point. Main advanced independently through CSS-only Writer Lamplight
changes, including `b7499b2` (#22), `11faa140` and `efde8a8` (#23). No rebase,
cherry-pick, merge or integration change was made as part of this CI investigation.

The commit also has a failed **Workers Builds: proairetos-writer** external check.
No deployment command was issued here; Cloudflare remains owner-managed and its
build/deployment settings were not changed. This separate check is not a GitHub
Actions runner/test result and is not reported as passed.

## Local browser coverage and restoration attempts

The only coverage configuration change adds the twelve new portable Phase 2
compatibility/library workflows to both Firefox and WebKit test matching. Every
existing test, matrix job and CI step remains. Installed Playwright 1.65.1 source
and current type documentation support Firefox touch/mobile overrides; historical
limitations are not a reason to add speculative skips. Existing CDP-only capability
skips remain explicit. Test discovery passed: Firefox **85** tests across twelve
files, WebKit **103** across fourteen. Discovery is not execution.

Executed from `apps/writer`:

```sh
npx playwright install firefox webkit
npx playwright install webkit
npm run test:browser -- tests/step7-compat.spec.ts --project=firefox --max-failures=1 --output=/tmp/writer-phase2-firefox-availability-results --reporter=list
npm run test:browser -- tests/step7-compat.spec.ts --project=webkit --max-failures=1 --output=/tmp/writer-phase2-webkit-availability-results --reporter=list
npm run test:browser -- --list --project=firefox
npm run test:browser -- --list --project=webkit
```

Both installations failed with HTTP 403 `Domain forbidden`, including retries on
official alternate paths/hosts. Examples:

```text
https://cdn.playwright.dev/dbazure/download/playwright/builds/firefox/1572/firefox-debian-13.zip
https://cdn.playwright.dev/dbazure/download/playwright/builds/webkit/2388/webkit-debian-13.zip
```

`playwright.download.prss.microsoft.com` also returned 403. Required infrastructure
action: allow these official pinned HTTPS downloads or provision approved exact
browser bundles, then rerun the installer and complete projects. No proxy bypass,
replacement engine, spoofed user agent or skipped-project pass is substituted.

Actual launch probes each exited 1 before application execution: Firefox's
`firefox-1572/firefox/firefox` and WebKit's `webkit-2388/pw_run.sh` are missing.
Each probe selected two cases; the first could not launch and the second did not
run after the local availability cap. These are environment failures, not observed
application assertion failures. Neither full suite is reported as passed.

Logs: `/tmp/writer-phase2-browser-install.log`, `writer-phase2-webkit-install.log`,
`writer-phase2-{firefox,webkit}-launch.log` and corresponding discovery logs.
No physical Safari, Android/PWA, Windows or macOS runtime is available here.

The complete available Chromium run passed **183 tests**, zero failed/skipped,
on the frozen production bundle. Rust 650, frontend 64, actual-WASM 65 and
build-bootstrap two also passed; commands and scope are in
[PHASE2_REPORT.md](PHASE2_REPORT.md). These do not substitute for unavailable
engines/platforms. Owner confirmation that there are no steps reinforces the
observed failure stage but does not supply the missing annotation cause.

Final-commit run/retry results are reported in the review PR and delivery without
assuming a cause. The missing reason remains a blocker to a justified hosted-
infrastructure correction. A public web-read fallback for the annotations URL
was also unavailable; no inaccessible message is inferred.
