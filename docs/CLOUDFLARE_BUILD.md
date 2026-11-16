# Writer Cloudflare build setup

The connected Worker is `proairetos-writer`. Its supplied build log failed at
`spawnSync wasm-bindgen ENOENT`, before Rust compilation. Node 24.18.0 and npm
10.9.2 installed the locked dependencies successfully; they are not the cause.
The standard `npm run build` expects the Rust toolchain to have been installed,
as it is in GitHub Actions and the documented local setup.

## Build-only correction

Set the Cloudflare **root directory** to `apps/writer` and, after this change is
available in the selected branch, use this **build command**:

```sh
npm run build:cloudflare
```

Cloudflare can retain its normal locked dependency installation. The new command:

1. Requires the documented Linux x86_64 build image.
2. Preserves supplied `CARGO_HOME` and `RUSTUP_HOME`; otherwise isolates tools in
   `apps/writer/.cache/rust-toolchain`, which is ignored by Git. It does not
   change `HOME` or shell startup files.
3. Installs Rustup from its official HTTPS installer if absent, then installs
   stable Rust, formatting/lint components requested by the workspace, and the
   `wasm32-unknown-unknown` target.
4. Installs the official wasm-bindgen **0.2.130** Linux musl executable if missing
   or mismatched. It verifies the pinned release SHA-256 before extraction or
   execution:
   `9687f3bb551b9f66f37ed32a40d7913a00f7962db653054e56857bc306c3227f`.
5. Runs the existing `npm run build` with the same toolchain environment and PATH.

The output remains `apps/writer/dist`. Existing local `npm run build` behavior
is unchanged. No engine/frontend dependencies, model changes, WASM wrapper,
workers-rs, wasm-pack, or deployment CLI are introduced. The CLI version must
continue matching the locked wasm-bindgen crate and existing binding generator.
Tool installation is checked each time; correctness does not depend on Cargo or
Rustup caches surviving between Cloudflare builds.

This script **does not deploy**. A connected Cloudflare pipeline can run its
configured deployment command after the build succeeds. Do not switch commands
or push a deployable branch until the private-access requirements below are met.
Do not activate the repository's inert example as though it described remote
settings; it names `writer-office-engine`, while the connected Worker is
`proairetos-writer`.

## Verification

On 2026-11-14, an actual cold build ran with empty Cargo/Rustup homes and a PATH
containing Node/system tools but neither Rust nor wasm-bindgen. The script
installed Rust **1.100.0**, the WASM target and CLI **0.2.130**, then completed
release WASM, TypeScript and Vite production builds. This tests the missing-tool
failure condition rather than relying only on an already-provisioned machine.

The exact cold-build command was equivalent to:

```sh
CARGO_HOME="$task_toolchain_root/cargo" \
RUSTUP_HOME="$task_toolchain_root/rustup" \
PATH="<Node 24 bin>:/usr/bin:/bin" npm run build:cloudflare
```

`task_toolchain_root` was created with `mktemp -d /tmp/writer-cloudflare-cold.XXXXXX`.
No global Rust installation was used by that test. The log is retained locally
at `/tmp/writer-cloudflare-cold-build.log`. Script syntax, lint, and TypeScript
checks passed; engine, direct WASM and production-browser verification results
are recorded with this change's completion report. This is local verification,
not a claim that the remote Cloudflare build or deployment has succeeded.

Executed local checks on the correction:

| Command | Result |
| --- | --- |
| `npm run build:cloudflare` with empty toolchain homes/no Rust on PATH | Pass: real cold setup and production build |
| `npm run build:cloudflare` with existing caller toolchain | Pass: real warm setup and production build |
| `node --check scripts/build-cloudflare.mjs` | Pass |
| `npm run typecheck` | Pass |
| `npm run lint` | Pass |
| `npm run test:build` | 2 passed; installer-command fakes test rejection and environment propagation |
| `npm test` | 20 storage unit tests passed |
| `npm run test:wasm` | 8 actual compiled-WASM tests passed on the cold-build bundle |
| `npm run test:browser` | 27 production Chromium tests passed on the cold-build bundle, 32.1 seconds |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass |
| `cargo test --workspace` | 191 passed including doctests |
| `cargo build --workspace` | Pass |

Frontend commands ran from `apps/writer`; Cargo commands ran from the repository
root. The CLI checksum test rejects a deliberately corrupt download before
extraction/install/build; it does not claim that the real release was corrupt.
Native tool installers are isolated only in the two orchestration regressions.
Cold/warm builds, engine/WASM and browser tests use the actual compiler/engine.
No suite above was skipped on the tested Linux x86_64 environment.

## Workers static-asset size gate

Cloudflare Workers rejects an individual static asset larger than **25 MiB (26,214,400 bytes)**. On 2026-11-16 the standalone Graphe build compiled successfully but `wrangler preview` refused the generated `office_wasm_bg-*.wasm` file at about **25.6 MiB** (26,846,320 bytes). This is the *uncompressed* WASM size; the smaller gzip figure in Vite's output does not affect the Cloudflare limit.

The `apps/writer/scripts/build-wasm.mjs` release build now requests size-optimized Rust code generation (opt-level z, full LTO, one codegen unit and symbol stripping) **only for the browser WASM build**. All remaining pinned font bytes and Rust document/layout features remain in the binary. Before packaging the frontend, the script checks the actual wasm-bindgen output against the Workers limit and fails with a precise byte count if still oversized. The post-build asset test independently checks the file Vite places in `dist/assets`.

These are compilation settings, not a verified reduction: the resulting size and regression tests must be measured on an environment with Rust/wasm-bindgen and runnable CI. Fraunces (regular and italic; 775,344 original bytes) was explicitly retired in the size-reduction branch, with Playfair Display as a fallback for existing documents. If the new binary remains too large, stop rather than silently removing more font families or document capabilities. Consider a separately reviewed external-font loader only after validating its compatibility, download/offline behavior and licensing. Any optimization that changes layout or editing behavior is a regression.

## Remote access and permission blocker

The current Cloudflare connection can identify the correct account, but Worker
settings, domains, subdomains and build configuration reads return **403** due
to missing Workers/Workers CI permissions. Listing Access applications returned
**403 code 9999, "Access is not enabled."** Private protection is therefore not
established. An attempted HTTPS probe was denied by the environment's outbound
proxy before reaching the origin; that 403 is not evidence of application
protection.

Before a successful pipeline may expose Writer:

- Establish and verify the intended Access application and allow policy for
  `writer.proairetos.com`, including unauthenticated-denial and authorized access.
- Verify workers.dev, preview/version URLs and other alternate endpoints do not
  bypass that restriction.
- Supply sufficient connection permissions to inspect the Worker and builds;
  changing build settings needs Workers CI Write. Access setup/verification
  needs the corresponding Access Apps and Policies permissions.

No DNS, Worker settings, build commands, access policies or deployment were
changed by the local build correction. Automatic Git deployment can run on
watched branches, so the fix is kept local until publication can preserve the
user's explicit private-access requirement.

## Sources and tradeoffs

- [Cloudflare build image](https://developers.cloudflare.com/workers/ci-cd/builds/build-image/): Ubuntu x86_64 and supported runtimes; Rust tooling is not promised.
- [Build configuration](https://developers.cloudflare.com/workers/ci-cd/builds/configuration/) and [caching](https://developers.cloudflare.com/workers/ci-cd/builds/build-caching/): root/command configuration and documented caches.
- [Build limits](https://developers.cloudflare.com/workers/ci-cd/builds/limits-and-pricing/): cold builds must fit the hosted time limit.
- [Official Rustup installation](https://rust-lang.github.io/rustup/installation/other.html).
- [wasm-bindgen CLI](https://wasm-bindgen.github.io/wasm-bindgen/reference/cli.html) and [0.2.130 release](https://github.com/wasm-bindgen/wasm-bindgen/releases/tag/0.2.130).

The official prebuilt binding CLI avoids compiling its large dependency graph
on each cold hosted build. The pinned asset checksum protects installation;
stable Rust follows the existing workspace toolchain policy. Setup and build run
in one environment because a separate npm prebuild subprocess cannot export PATH
back to its parent. No new deployment subsystem is introduced by this correction.
