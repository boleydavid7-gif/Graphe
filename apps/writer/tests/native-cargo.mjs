import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { delimiter, join } from 'node:path';
import process from 'node:process';
import { fileURLToPath, URL } from 'node:url';

const writerDirectory = fileURLToPath(new URL('../', import.meta.url));
const toolchainDirectory = join(writerDirectory, '.cache', 'rust-toolchain');
const cachedCargoHome = join(toolchainDirectory, 'cargo');
const cachedRustupHome = join(toolchainDirectory, 'rustup');
const cachedCargo = join(cachedCargoHome, 'bin', 'cargo');

/**
 * Cloudflare's build:cloudflare script installs Rust under .cache and supplies
 * its PATH only to the build subprocess. Subsequent npm test commands run with
 * the original PATH, so the native parity probes need the cached toolchain.
 * Ordinary developer and hosted native test runs retain their existing cargo.
 */
export function execCargo(args, options = {}) {
  if (process.platform !== 'linux' || !existsSync(cachedCargo) || !existsSync(cachedRustupHome)) {
    return execFileSync('cargo', args, options);
  }
  const env = {
    ...process.env,
    ...options.env,
    CARGO_HOME: cachedCargoHome,
    RUSTUP_HOME: cachedRustupHome,
    PATH: `${join(cachedCargoHome, 'bin')}${delimiter}${options.env?.PATH ?? process.env.PATH ?? ''}`,
  };
  return execFileSync(cachedCargo, args, { ...options, env });
}
