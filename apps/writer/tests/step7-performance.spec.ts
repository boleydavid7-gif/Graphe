import { test, expect } from '@playwright/test';
import { focusEditor, readyPages } from './step7-test-helpers';

interface VisibleSample {
  readonly revision: string;
  readonly trusted: boolean;
  readonly geometryMs: number;
  readonly nextFrameMs: number;
}

interface MeasurementState {
  readonly samples: VisibleSample[];
  readonly pending: { at: number; trusted: boolean }[];
}

test('measure real keyboard input through committed Rust SVG geometry on a ten-thousand-character document', async ({ page }, testInfo) => {
  test.setTimeout(60000);
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  const input = page.getByRole('textbox', { name: 'Document content', exact: true });
  const sentence = 'Review document integrity, international text, and local save recovery. Preserve formatting and stable identifiers. ';
  const content = Array.from({ length: 50 }, (_, index) => `${index + 1}. ${sentence}${sentence}`).join('\n');
  await focusEditor(page);
  await input.evaluate((element, value) => {
    const clipboard = new DataTransfer();
    clipboard.setData('text/plain', value);
    element.dispatchEvent(new ClipboardEvent('paste', { clipboardData: clipboard, bubbles: true, cancelable: true }));
  }, content);
  await readyPages(page);
  await page.keyboard.press('Control+End');
  await readyPages(page);
  await page.evaluate(() => {
    const input = document.querySelector('[data-testid="editor"]');
    const root = document.querySelector('[data-testid="paged-editor"]');
    if (!input || !root) throw new Error('Missing actual page input and Rust SVG projection');
    const state: MeasurementState = { samples: [], pending: [] };
    (window as Window & { writerStep7VisibleMeasurement?: MeasurementState }).writerStep7VisibleMeasurement = state;
    let previousRevision = root.getAttribute('data-layout-revision');
    input.addEventListener('beforeinput', (event) => {
      if (event instanceof InputEvent && event.inputType === 'insertText') {
        state.pending.push({ at: performance.now(), trusted: event.isTrusted });
      }
    }, true);
    new MutationObserver(() => {
      const revision = root.getAttribute('data-layout-revision');
      if (root.getAttribute('aria-busy') !== 'false' || !revision
        || revision === previousRevision || !state.pending.length) return;
      previousRevision = revision;
      const pending = state.pending.shift();
      if (!pending) throw new Error('No trusted input awaits its Rust geometry');
      const geometryAt = performance.now();
      requestAnimationFrame(() => state.samples.push({ revision, trusted: pending.trusted,
        geometryMs: geometryAt - pending.at, nextFrameMs: performance.now() - pending.at }));
    }).observe(root, { attributes: true, attributeFilter: ['data-layout-revision', 'aria-busy'] });
  });
  for (let count = 1; count <= 30; count += 1) {
    await page.keyboard.type('x');
    // Wait for every real key's visible geometry. Batching synthetic events in
    // one JS task would measure only editing and defer the entire layout pass.
    await page.waitForFunction((expected) => {
      const state = (window as Window & { writerStep7VisibleMeasurement?: MeasurementState }).writerStep7VisibleMeasurement;
      return (state?.samples.length ?? 0) >= expected;
    }, count, { polling: 'raf', timeout: 10000 });
  }
  const samples = await page.evaluate(() => (window as Window & { writerStep7VisibleMeasurement?: MeasurementState })
    .writerStep7VisibleMeasurement?.samples ?? []);
  expect(samples).toHaveLength(30);
  expect(samples.every((sample) => sample.trusted && Number.isFinite(sample.geometryMs)
    && Number.isFinite(sample.nextFrameMs) && sample.geometryMs >= 0 && sample.nextFrameMs >= sample.geometryMs)).toBe(true);
  expect(new Set(samples.map((sample) => sample.revision)).size).toBe(30);
  await expect(input).toHaveText(`${content.replaceAll('\n', '')}${'x'.repeat(30)}`);
  await readyPages(page);
  const summarize = (values: number[]) => {
    const sorted = [...values].sort((left, right) => left - right);
    const at = (quantile: number) => sorted[Math.max(0, Math.ceil(sorted.length * quantile) - 1)];
    return { p50Ms: at(0.5), p95Ms: at(0.95), maxMs: sorted.at(-1) };
  };
  const result = { browser: testInfo.project.name, charactersBeforeTyping: content.length,
    paragraphs: 50, trustedKeys: samples.length,
    committedSvgGeometry: summarize(samples.map((sample) => sample.geometryMs)),
    nextAnimationFrame: summarize(samples.map((sample) => sample.nextFrameMs)),
    scope: 'Trusted beforeinput capture through committed Rust SVG geometry, plus the next requestAnimationFrame callback. Warm pinned-font catalog; not OS input-to-paint or measured pixel presentation latency.' };
  console.log(`Writer visible layout measurement: ${JSON.stringify(result)}`);
  await testInfo.attach('visible-layout-measurement', { body: JSON.stringify({ result, samples }, null, 2), contentType: 'application/json' });
});
