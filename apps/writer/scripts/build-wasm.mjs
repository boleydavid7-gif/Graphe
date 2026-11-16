import { execFileSync } from 'node:child_process';
import { mkdirSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const workspace = fileURLToPath(new URL('../../../', import.meta.url));
const output = fileURLToPath(new URL('../src/wasm/', import.meta.url));
const binary = fileURLToPath(new URL('../../../target/wasm32-unknown-unknown/release/office_wasm.wasm', import.meta.url));
const expectedVersion = 'wasm-bindgen 0.2.130';
// Workers static assets must be no larger than 25 MiB, independent of gzip transfer size.
const workerAssetLimitBytes = 25 * 1024 * 1024;

try {
  const actualVersion = execFileSync('wasm-bindgen', ['--version'], { encoding: 'utf8' }).trim();
  if (actualVersion !== expectedVersion) {
    throw new Error(`Expected ${expectedVersion}, found ${actualVersion}. Install the matching locked CLI.`);
  }
  execFileSync('cargo', ['build', '--locked', '--release', '-p', 'office-wasm', '--target', 'wasm32-unknown-unknown'], {
    cwd: workspace,
    stdio: 'inherit',
    // Optimize only the browser binary for size. Preserve the pinned Rust font catalog,
    // Unicode behavior, and the normal release profile for native workspace builds.
    env: {
      ...process.env,
      CARGO_PROFILE_RELEASE_OPT_LEVEL: 'z',
      CARGO_PROFILE_RELEASE_LTO: 'fat',
      CARGO_PROFILE_RELEASE_CODEGEN_UNITS: '1',
      CARGO_PROFILE_RELEASE_STRIP: 'symbols',
    },
  });
  mkdirSync(output, { recursive: true });
  execFileSync('wasm-bindgen', [binary, '--target', 'web', '--out-dir', output, '--out-name', 'office_wasm'], {
    cwd: workspace,
    stdio: 'inherit',
  });
  const wasmBytes = statSync(fileURLToPath(new URL('../src/wasm/office_wasm_bg.wasm', import.meta.url))).size;
  console.log(`Graphe WebAssembly asset: ${(wasmBytes / (1024 * 1024)).toFixed(2)} MiB (Workers limit: 25 MiB).`);
  if (wasmBytes > workerAssetLimitBytes) {
    throw new Error(`WebAssembly is ${wasmBytes} bytes, above Cloudflare Workers' ${workerAssetLimitBytes}-byte static-asset limit. Do not deploy; reduce the bundle without removing fonts or document capabilities.`);
  }
} catch (error) {
  console.error('Writer WebAssembly build failed. Install stable Rust, wasm32-unknown-unknown, and wasm-bindgen-cli 0.2.130.');
  if (process.env.WORKERS_CI === '1') console.error('For Cloudflare Workers Builds, use npm run build:cloudflare as the build command.');
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
}
