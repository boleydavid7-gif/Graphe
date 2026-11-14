import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { fileURLToPath, URL } from 'node:url';

// Deliberate maintenance command, never invoked by the test suite. Review the
// changed layout and font provenance before accepting an updated fingerprint.
const fixtureRoot = new URL('../../../tests/fixtures/layout/', import.meta.url);
const input = 'layout-v1.writer';
const command = ['run', '--offline', '--locked', '--quiet', '-p', 'office-layout',
  '--example', 'layout_snapshot', '--', fileURLToPath(new URL(input, fixtureRoot))];
const snapshot = execFileSync('cargo', command, {
  cwd: fileURLToPath(new URL('../../../', import.meta.url)), encoding: 'utf8', maxBuffer: 32 * 1024 * 1024,
}).trim();
JSON.parse(snapshot);
const sha256 = createHash('sha256').update(snapshot).digest('hex');
await writeFile(new URL('native-parity.json', fixtureRoot), `${JSON.stringify({
  input,
  options: {},
  sha256,
  producer: 'cargo run --offline --locked --quiet -p office-layout --example layout_snapshot -- tests/fixtures/layout/layout-v1.writer',
}, null, 2)}\n`);
