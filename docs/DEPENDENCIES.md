# Writer dependency review

Reviewed on 2026-11-14. This inventory records upstream dependency licenses;
the Office engine's own license remains undecided. No Office engine or existing
rich-text editor is embedded. The npm lockfile and Cargo.lock pin the resolved
graphs, including platform-specific optional packages.

## Runtime and binding dependencies

| Dependency | Version | License | Purpose |
| --- | --- | --- | --- |
| React / React DOM | 19.3.1 | MIT | UI presentation and lifecycle |
| wasm-bindgen | 0.2.130 | MIT OR Apache-2.0 | Small Rust/WebAssembly binding boundary |

The JavaScript production graph also contains React's MIT-licensed scheduler.
Existing Serde and Unicode segmentation libraries remain the engine's source
of serialization and grapheme behavior. IndexedDB uses native browser APIs;
there is no storage wrapper, JavaScript document model, or browser undo library.

## Development dependencies

| Dependency | Version | License | Purpose |
| --- | --- | --- | --- |
| Vite | 8.3.8 | MIT | Development server and production asset bundling |
| @vitejs/plugin-react | 6.3.0 | MIT | React integration |
| TypeScript | 6.0.3 | Apache-2.0 | Static checking |
| ESLint / @eslint/js | 10.15.0 / 10.0.1 | MIT | Linting |
| typescript-eslint | 8.72.2 | MIT | TypeScript lint rules |
| eslint-plugin-react-hooks | 7.1.1 | MIT | Hook correctness rules |
| eslint-plugin-react-refresh | 0.5.7 | MIT | Component export checks |
| Vitest | 5.1.0 | MIT | Frontend unit tests |
| @playwright/test | 1.65.1 | Apache-2.0 | Real Chromium integration tests |
| fake-indexeddb | 6.2.8 | Apache-2.0 | Isolated storage unit tests |
| @types/react / @types/react-dom | 19.3.0 | MIT | React declarations |
| @types/node | 24.19.2 | MIT | Node build/test declarations |

Node 24 is the shared local/CI runtime. TypeScript 7 was considered but the
current TypeScript ESLint peer range is `<6.1`; TypeScript 6.0.3 satisfies the
published compatibility ranges. Vite, its React plugin, Vitest, and Playwright
support Node 24. These maintained upstream projects are used for their defined
low-level build/test roles.

The transitive npm graph includes MIT, Apache-2.0, ISC, BSD-2-Clause,
BSD-3-Clause, 0BSD, and BlueOak-1.0.0 packages. Two noteworthy build-only cases:

- Lightning CSS and its optional native binaries use **MPL-2.0**. They are
  unmodified build tools, not linked into Writer's browser runtime. Preserve
  their notices/source obligations if redistributing those tools or modifying
  their covered source files.
- `caniuse-lite` browser-compatibility data uses **CC-BY-4.0**. It is build-time
  data; preserve its attribution if redistributing the data or toolchain.

Upstream license texts remain in installed packages. Review distribution notices
again before a public release; no public licensing choice is inferred here.

## Security and supply chain

`npm audit` initially identified the moderate development-only
[minimatch globparts amplification advisory](https://github.com/advisories/GHSA-g3hv-75wg-m249)
through TypeScript ESLint's minimatch 10 dependency. A scoped npm override pins
`@typescript-eslint/typescript-estree`'s minimatch to **11.0.8**. The dependency's
actual use is the unchanged named `minimatch()` API with the `dot` option; the
replacement supports Node 24. Keep the override until upstream's range permits
the fixed major version, and check it when updating the toolchain.
After clean lockfile regeneration, `npm ls --depth=0` and `npm ls minimatch`
passed. `npm audit --json` reported **zero vulnerabilities** across 212 locked
package entries on the review date; this is a point-in-time advisory check.

The binding CLI must match wasm-bindgen **0.2.130**. Generated bindings and
binary assets are build outputs rather than checked-in sources. GitHub Actions
check-out and Node setup actions are pinned to commit hashes. No Cloudflare CLI,
deployment token, telemetry service, or external font service is required.

Sources: package registry manifests and installed license texts;
[React license](https://github.com/facebook/react/blob/main/LICENSE),
[Vite license](https://github.com/vitejs/vite/blob/main/LICENSE),
[TypeScript license](https://github.com/microsoft/TypeScript/blob/main/LICENSE.txt),
[Playwright license](https://github.com/microsoft/playwright/blob/main/LICENSE),
[wasm-bindgen licenses](https://github.com/wasm-bindgen/wasm-bindgen),
[Lightning CSS](https://github.com/parcel-bundler/lightningcss), and
[caniuse-lite](https://github.com/browserslist/caniuse-lite).
