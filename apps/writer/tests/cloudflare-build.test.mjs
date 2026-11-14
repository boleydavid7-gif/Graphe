import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join } from 'node:path';
import { arch, env as parentEnvironment, execPath, platform } from 'node:process';
import test from 'node:test';
import { fileURLToPath, URL } from 'node:url';

const script = fileURLToPath(new URL('../scripts/build-cloudflare.mjs', import.meta.url));
const supportedPlatform = platform === 'linux' && arch === 'x64';
const options = { skip: supportedPlatform ? false : 'Cloudflare bootstrap targets Linux x86_64.' };

/** Isolate only build orchestration. Engine/WASM behavior has separate real tests. */
function fixture(context) {
  const root = mkdtempSync(join(tmpdir(), 'writer-bootstrap-test-'));
  context.after(() => rmSync(root, { recursive: true, force: true }));
  const writer = join(root, 'writer');
  const scripts = join(writer, 'scripts');
  const commands = join(root, 'commands');
  const cargoHome = join(root, 'caller cargo home');
  const rustupHome = join(root, 'caller rustup home');
  const cargoBin = join(cargoHome, 'bin');
  const log = join(root, 'commands.jsonl');
  for (const directory of [scripts, commands, cargoBin, rustupHome]) mkdirSync(directory, { recursive: true });
  const copiedScript = join(scripts, 'build-cloudflare.mjs');
  copyFileSync(script, copiedScript);
  const environment = {
    ...parentEnvironment,
    PATH: commands,
    CARGO_HOME: cargoHome,
    RUSTUP_HOME: rustupHome,
    WRITER_TEST_COMMAND_LOG: log,
  };

  function command(name, behavior, directory = commands) {
    const source = `#!${execPath}
const fs = require('node:fs');
const args = process.argv.slice(2);
fs.appendFileSync(process.env.WRITER_TEST_COMMAND_LOG, JSON.stringify({
  command: ${JSON.stringify(name)}, args, cwd: process.cwd(),
  path: process.env.PATH, cargoHome: process.env.CARGO_HOME,
  rustupHome: process.env.RUSTUP_HOME, home: process.env.HOME ?? null,
}) + '\\n');
${behavior}
`;
    writeFileSync(join(directory, name), source, { mode: 0o755 });
  }

  command('rustup', "if (args[0] === '--version') console.log('rustup 1.29.0 (test stub)');");
  command('rustc', "console.log('rustc 1.100.0 (test stub)');");
  command('npm', "if (JSON.stringify(args) !== JSON.stringify(['run', 'build'])) process.exit(90);");

  return {
    writer, cargoHome, rustupHome, cargoBin, command,
    run: () => spawnSync(execPath, [copiedScript], { cwd: writer, env: environment, encoding: 'utf8' }),
    calls: () => existsSync(log) ? readFileSync(log, 'utf8').trim().split('\n').map((line) => JSON.parse(line)) : [],
  };
}

test('an incorrect downloaded CLI checksum prevents extraction, installation and build', options, (context) => {
  const setup = fixture(context);
  setup.command('curl', `
const destination = args[args.indexOf('-o') + 1];
if (!destination) process.exit(91);
fs.writeFileSync(destination, 'Not the published wasm-bindgen release archive.');
`);
  setup.command('tar', 'process.exit(92);');

  const result = setup.run();
  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stderr, /release checksum did not match/);
  const calls = setup.calls();
  const download = calls.find((call) => call.command === 'curl');
  assert.ok(download, 'The missing CLI must be downloaded before its digest can be checked.');
  assert.ok(download.args.includes('https://github.com/wasm-bindgen/wasm-bindgen/releases/download/0.2.130/wasm-bindgen-0.2.130-x86_64-unknown-linux-musl.tar.gz'));
  assert.deepEqual(calls.filter((call) => ['tar', 'rustc', 'npm'].includes(call.command)), []);
  assert.equal(existsSync(join(setup.cargoBin, 'wasm-bindgen')), false);
  const archive = download.args[download.args.indexOf('-o') + 1];
  assert.equal(existsSync(dirname(archive)), false, 'Failed setup must clean temporary download files.');
});

test('a matching CLI skips downloading and preserves the caller environment for npm build', options, (context) => {
  const setup = fixture(context);
  setup.command('wasm-bindgen', "console.log('wasm-bindgen 0.2.130');", setup.cargoBin);
  setup.command('curl', 'process.exit(93);');
  setup.command('tar', 'process.exit(94);');

  const result = setup.run();
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Writer toolchain ready: wasm-bindgen 0\.2\.130, wasm32-unknown-unknown/);
  const calls = setup.calls();
  assert.deepEqual(calls.filter((call) => ['curl', 'tar'].includes(call.command)), []);
  const target = calls.find((call) => call.command === 'rustup' && call.args[0] === 'toolchain');
  assert.ok(target?.args.includes('stable'));
  assert.ok(target?.args.includes('wasm32-unknown-unknown'));
  const build = calls.find((call) => call.command === 'npm');
  assert.ok(build, 'The prepared environment must be used for the existing production build.');
  assert.deepEqual(build.args, ['run', 'build']);
  assert.equal(build.cwd, setup.writer);
  assert.equal(build.cargoHome, setup.cargoHome);
  assert.equal(build.rustupHome, setup.rustupHome);
  assert.equal(build.home, parentEnvironment.HOME ?? null);
  assert.equal(build.path.split(delimiter)[0], setup.cargoBin);
});
