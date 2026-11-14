import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// Workers Builds currently uses Ubuntu x86_64. Keep local/native builds unchanged.
const writerDirectory = fileURLToPath(new URL('../', import.meta.url));
const toolchainDirectory = join(writerDirectory, '.cache', 'rust-toolchain');
const cliVersion = '0.2.130';
const cliDirectory = `wasm-bindgen-${cliVersion}-x86_64-unknown-linux-musl`;
const cliArchive = `${cliDirectory}.tar.gz`;
// Official release asset digest, reviewed alongside the pinned Cargo dependency.
const cliSha256 = '9687f3bb551b9f66f37ed32a40d7913a00f7962db653054e56857bc306c3227f';

function run(command, args, env) {
  return execFileSync(command, args, { env, cwd: writerDirectory, stdio: 'inherit' });
}

function download(url, destination, env) {
  run('curl', ['--proto', '=https', '--proto-redir', '=https', '--tlsv1.2', '-fsSL', '--retry', '3', url, '-o', destination], env);
}

let scratch;
try {
  if (process.platform !== 'linux' || process.arch !== 'x64') {
    throw new Error('Cloudflare bootstrap supports its Linux x86_64 build image. Use the documented local Rust setup and npm run build on other platforms.');
  }
  const env = { ...process.env };
  env.CARGO_HOME ??= join(toolchainDirectory, 'cargo');
  env.RUSTUP_HOME ??= join(toolchainDirectory, 'rustup');
  const cargoBin = join(env.CARGO_HOME, 'bin');
  env.PATH = `${cargoBin}${delimiter}${env.PATH ?? ''}`;
  mkdirSync(cargoBin, { recursive: true });
  scratch = mkdtempSync(join(tmpdir(), 'writer-rust-tools-'));

  if (spawnSync('rustup', ['--version'], { env, encoding: 'utf8' }).status !== 0) {
    const installer = join(scratch, 'rustup-init.sh');
    download('https://sh.rustup.rs', installer, env);
    run('sh', [installer, '-y', '--profile', 'minimal', '--default-toolchain', 'stable', '--no-modify-path'], env);
  }
  run('rustup', ['toolchain', 'install', 'stable', '--profile', 'minimal', '--component', 'rustfmt', '--component', 'clippy', '--target', 'wasm32-unknown-unknown'], env);

  const current = spawnSync('wasm-bindgen', ['--version'], { env, encoding: 'utf8' });
  if (current.status !== 0 || current.stdout.trim() !== `wasm-bindgen ${cliVersion}`) {
    const archive = join(scratch, cliArchive);
    download(`https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${cliVersion}/${cliArchive}`, archive, env);
    const digest = createHash('sha256').update(readFileSync(archive)).digest('hex');
    if (digest !== cliSha256) throw new Error('The wasm-bindgen release checksum did not match. No downloaded executable was installed.');
    run('tar', ['-xzf', archive, '-C', scratch, `${cliDirectory}/wasm-bindgen`], env);
    const executable = join(cargoBin, 'wasm-bindgen');
    copyFileSync(join(scratch, cliDirectory, 'wasm-bindgen'), executable);
    chmodSync(executable, 0o755);
  }

  // Run setup and build with the same PATH; an npm prebuild subprocess cannot
  // export its environment to a subsequent npm build command.
  const verified = execFileSync('wasm-bindgen', ['--version'], { env, encoding: 'utf8' }).trim();
  if (verified !== `wasm-bindgen ${cliVersion}`) throw new Error(`Unexpected binding generator version: ${verified}`);
  run('rustc', ['--version'], env);
  console.log(`Writer toolchain ready: ${verified}, wasm32-unknown-unknown.`);
  run('npm', ['run', 'build'], env);
} catch (error) {
  console.error('Writer Cloudflare build failed during toolchain setup or compilation. No deployment command was run by this script.');
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
} finally {
  if (scratch) rmSync(scratch, { recursive: true, force: true });
}
