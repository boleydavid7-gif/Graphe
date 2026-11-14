import { execFileSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const workspace = fileURLToPath(new URL('../../../', import.meta.url));
const output = fileURLToPath(new URL('../src/wasm/', import.meta.url));
const binary = fileURLToPath(new URL('../../../target/wasm32-unknown-unknown/release/office_wasm.wasm', import.meta.url));
const expectedVersion = 'wasm-bindgen 0.2.130';

try {
  const actualVersion = execFileSync('wasm-bindgen', ['--version'], { encoding: 'utf8' }).trim();
  if (actualVersion !== expectedVersion) {
    throw new Error(`Expected ${expectedVersion}, found ${actualVersion}. Install the matching locked CLI.`);
  }
  execFileSync('cargo', ['build', '--locked', '--release', '-p', 'office-wasm', '--target', 'wasm32-unknown-unknown'], {
    cwd: workspace,
    stdio: 'inherit',
  });
  mkdirSync(output, { recursive: true });
  execFileSync('wasm-bindgen', [binary, '--target', 'web', '--out-dir', output, '--out-name', 'office_wasm'], {
    cwd: workspace,
    stdio: 'inherit',
  });
} catch (error) {
  console.error('Writer WebAssembly build failed. Install stable Rust, wasm32-unknown-unknown, and wasm-bindgen-cli 0.2.130.');
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
}
