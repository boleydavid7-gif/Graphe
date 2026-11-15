import { readFile } from 'node:fs/promises';
import { test, expect, type Locator, type Page, type Download } from '@playwright/test';
import { focusEditor, readyPages } from './step7-test-helpers';

const input = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const menu = (page: Page) => page.locator('details.document-menu');
const summary = (page: Page) => menu(page).locator('summary');
const text = 'Menu writing — Café e\u0301 👩🏽‍💻';
const runtimeErrors = new WeakMap<Page, string[]>();

test.beforeEach(({ page }) => {
  const errors: string[] = [];
  runtimeErrors.set(page, errors);
  page.on('pageerror', (error) => errors.push(error.message));
});
test.afterEach(({ page }) => { expect(runtimeErrors.get(page)).toEqual([]); });

async function activate(control: Locator, touch: boolean) {
  if (touch) await control.tap(); else await control.click();
}
async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
}
async function openMenu(page: Page, touch: boolean) {
  if (!await menu(page).evaluate((element) => element.hasAttribute('open'))) await activate(summary(page), touch);
  await expect(menu(page)).toHaveAttribute('open', '');
}
async function action(page: Page, name: string, touch: boolean) {
  await openMenu(page, touch);
  await activate(menu(page).getByRole('button', { name, exact: true }), touch);
  await expect(menu(page)).not.toHaveAttribute('open', '');
}
async function scrollMenuToEnd(page: Page) {
  const panel = page.locator('.document-menu-panel');
  const box = await panel.boundingBox();
  if (!box) throw new Error('The scrollable menu was not displayed');
  if (!await panel.evaluate((element) => element.scrollHeight > element.clientHeight)) return;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, 2000);
  await expect.poll(() => panel.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
}
async function saved(page: Page, touch: boolean) {
  await activate(page.getByRole('button', { name: 'Save', exact: true }), touch);
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
}
async function holdForegroundAutosave(page: Page) {
  // Keep these unsaved-decision tests deterministic on slow browser runners.
  // Only the documented autosave timers are delayed; explicit Save still uses
  // the real Rust serialization and IndexedDB transaction/etag lane.
  await page.addInitScript(() => {
    const schedule = window.setTimeout.bind(window);
    window.setTimeout = ((handler: TimerHandler, delay?: number, ...args: unknown[]) => schedule(handler,
      delay === 750 || delay === 5000 ? 60_000 : delay, ...args)) as typeof window.setTimeout;
  });
}
async function downloadBytes(download: Download) {
  const path = await download.path();
  if (!path) throw new Error('The browser did not retain the exported file');
  return readFile(path);
}
async function reviewDownload(page: Page, format: 'writer' | 'docx' | 'pdf', touch: boolean) {
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption(format);
  const acknowledge = dialog.getByRole('checkbox', { name: 'I understand the format limitations', exact: true });
  if (await acknowledge.count()) await acknowledge.check();
  const pending = page.waitForEvent('download');
  await activate(dialog.getByRole('button', { name: 'Download copy', exact: true }), touch);
  const downloaded = await pending;
  expect(downloaded.suggestedFilename()).toMatch(new RegExp(`\\.${format}$`));
  await expect(dialog).toHaveCount(0);
  return downloadBytes(downloaded);
}

for (const touch of [false, true]) {
  test.describe(touch ? 'touch document menu' : 'desktop document menu', () => {
    if (touch) test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

    test('New, title rename, Save, Close and Open retain actual document content', async ({ page }) => {
      await start(page);
      await page.getByRole('textbox', { name: 'Document title', exact: true }).fill('Menu saved writing');
      await focusEditor(page);
      await page.keyboard.insertText(text);
      await saved(page, touch);
      await action(page, 'Open document', touch);
      await activate(page.getByRole('dialog', { name: 'Open document', exact: true })
        .getByRole('button', { name: 'Cancel', exact: true }), touch);
      await expect(summary(page)).toBeFocused();
      await action(page, 'Close', touch);
      await expect(input(page)).toHaveCount(0);
      await openMenu(page, touch);
      for (const name of ['Save As…', 'Export', 'Export Word', 'Page preview', 'Close']) {
        await expect(menu(page).getByRole('button', { name, exact: true })).toBeDisabled();
      }
      await action(page, 'Open document', touch);
      const dialog = page.getByRole('dialog', { name: 'Open document', exact: true });
      await activate(dialog.getByRole('button', { name: /Menu saved writing/ }), touch);
      await expect(dialog).toHaveCount(0);
      await expect(input(page)).toHaveText(text);
      await action(page, 'New document', touch);
      await expect(input(page)).toHaveText('');
      await expect(page.getByRole('textbox', { name: 'Document title', exact: true })).toHaveValue('');
      await page.keyboard.insertText('A new document');
      await expect(input(page)).toHaveText('A new document');
      await saved(page, touch);
    });

    test('unsaved New cancellation preserves content and Discard accepts the first new keystroke', async ({ page }) => {
      await holdForegroundAutosave(page);
      await start(page);
      await focusEditor(page);
      await page.keyboard.insertText(text);
      await action(page, 'New document', touch);
      const guard = page.getByRole('dialog', { name: 'Save your changes?', exact: true });
      await activate(guard.getByRole('button', { name: 'Cancel', exact: true }), touch);
      await expect(guard).toHaveCount(0);
      await expect(input(page)).toHaveText(text);
      await expect(summary(page)).toBeFocused();
      await action(page, 'New document', touch);
      await activate(guard.getByRole('button', { name: 'Discard changes', exact: true }), touch);
      // Do not focus the editor or wait for a focus assertion: the first input
      // after the dialog's click must belong to the replacement Rust session.
      await page.keyboard.insertText('The first new keystroke');
      await expect(guard).toHaveCount(0);
      await expect(input(page)).toHaveText('The first new keystroke');
      await expect(input(page).locator('p')).toHaveCount(1);
    });

    test('unsaved Close can Save and continue then reopen the exact newer revision', async ({ page }) => {
      await holdForegroundAutosave(page);
      await start(page);
      await page.getByRole('textbox', { name: 'Document title', exact: true }).fill('Guarded menu writing');
      await focusEditor(page);
      await page.keyboard.insertText(text);
      await saved(page, touch);
      await focusEditor(page);
      await page.keyboard.press('Control+End');
      await page.keyboard.insertText(' — the revision to retain');
      await action(page, 'Close', touch);
      const guard = page.getByRole('dialog', { name: 'Save your changes?', exact: true });
      await activate(guard.getByRole('button', { name: 'Save and continue', exact: true }), touch);
      await expect(guard).toHaveCount(0);
      await expect(input(page)).toHaveCount(0);
      await action(page, 'Open document', touch);
      const dialog = page.getByRole('dialog', { name: 'Open document', exact: true });
      await activate(dialog.getByRole('button', { name: /Guarded menu writing/ }), touch);
      await expect(dialog).toHaveCount(0);
      await expect(input(page)).toHaveText(`${text} — the revision to retain`);
      await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      await page.keyboard.press('Control+End');
      await page.keyboard.insertText('!');
      await expect(input(page)).toHaveText(`${text} — the revision to retain!`);
    });

    test('nested library unsaved guards return on Cancel and preserve both saved documents', async ({ page }) => {
      await holdForegroundAutosave(page);
      await start(page);
      const targetTitle = 'Nested menu target';
      const sourceTitle = 'Nested menu source';
      let targetText = `Target — ${text}`;
      let sourceText = `Source — ${text}`;
      await page.getByRole('textbox', { name: 'Document title', exact: true }).fill(targetTitle);
      await focusEditor(page);
      await page.keyboard.insertText(targetText);
      await saved(page, touch);
      await action(page, 'New document', touch);
      await page.getByRole('textbox', { name: 'Document title', exact: true }).fill(sourceTitle);
      await focusEditor(page);
      await page.keyboard.insertText(sourceText);
      await saved(page, touch);

      for (const decision of ['Discard changes', 'Save and continue'] as const) {
        await page.keyboard.press('Control+End');
        const draft = `${sourceText} — draft for ${decision}`;
        await page.keyboard.insertText(` — draft for ${decision}`);
        await action(page, 'Open document', touch);
        const library = page.getByRole('dialog', { name: 'Open document', exact: true });
        const targetEntry = library.getByRole('button', { name: new RegExp(targetTitle) });
        await activate(targetEntry, touch);
        const guard = page.getByRole('dialog', { name: 'Save your changes?', exact: true });
        await activate(guard.getByRole('button', { name: 'Cancel', exact: true }), touch);
        await expect(guard).toHaveCount(0);
        await expect(library).toBeVisible();
        await expect(targetEntry).toBeFocused();
        // The source is inert behind the library, but its Rust-backed DOM
        // projection must still retain the exact uncommitted text.
        await expect(page.locator('[data-testid="editor"]')).toHaveText(draft);
        await activate(targetEntry, touch);
        await activate(guard.getByRole('button', { name: decision, exact: true }), touch);
        await expect(guard).toHaveCount(0);
        await expect(library).toHaveCount(0);
        await expect(input(page)).toHaveText(targetText);
        await page.keyboard.press('Control+End');
        const marker = ` [opened via ${decision}]`;
        await page.keyboard.insertText(marker);
        targetText += marker;
        await expect(input(page)).toHaveText(targetText);
        await saved(page, touch);

        await action(page, 'Open document', touch);
        const reopen = page.getByRole('dialog', { name: 'Open document', exact: true });
        await activate(reopen.getByRole('button', { name: new RegExp(sourceTitle) }), touch);
        await expect(reopen).toHaveCount(0);
        if (decision === 'Save and continue') sourceText = draft;
        await expect(input(page)).toHaveText(sourceText);
        await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      }
      await action(page, 'Open document', touch);
      const reopen = page.getByRole('dialog', { name: 'Open document', exact: true });
      await activate(reopen.getByRole('button', { name: new RegExp(targetTitle) }), touch);
      await expect(reopen).toHaveCount(0);
      await expect(input(page)).toHaveText(targetText);
    });

    test('Save As, Export and Export Word deliver files and preserve editing focus and history', async ({ page }) => {
      await start(page);
      await focusEditor(page);
      await page.keyboard.insertText(text);
      await saved(page, touch);
      await action(page, 'Save As…', touch);
      const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
      await activate(dialog.getByRole('button', { name: 'Cancel', exact: true }), touch);
      await expect(summary(page)).toBeFocused();
      await action(page, 'Save As…', touch);
      expect((await reviewDownload(page, 'pdf', touch)).subarray(0, 5).toString()).toBe('%PDF-');
      const nativeDownload = page.waitForEvent('download');
      await action(page, 'Export', touch);
      const native = JSON.parse((await downloadBytes(await nativeDownload)).toString('utf8')) as {
        document: { sections: { blocks: { content: { runs: { text: string }[] } }[] }[] };
      };
      expect(native.document.sections.flatMap((section) => section.blocks)
        .flatMap((block) => block.content.runs).map((run) => run.text).join('')).toBe(text);
      await action(page, 'Export Word', touch);
      expect((await reviewDownload(page, 'docx', touch)).subarray(0, 2).toString()).toBe('PK');
      await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      await readyPages(page);
      const box = await page.locator('svg[data-layout-page]').first().boundingBox();
      if (!box) throw new Error('Missing actual Rust document page');
      if (touch) await page.touchscreen.tap(box.x + box.width * 0.2, box.y + box.width * 0.15);
      else await page.mouse.click(box.x + box.width * 0.2, box.y + box.width * 0.15);
      await expect(input(page)).toBeFocused();
      await page.keyboard.press('Control+End');
      await page.keyboard.insertText(' — after export');
      await expect(input(page)).toHaveText(`${text} — after export`);
      await page.keyboard.press('Control+z');
      await expect(input(page)).toHaveText(text);
      await saved(page, touch);
    });

    test('Page preview toggles real Rust pages and returns to the same editable selection', async ({ page }) => {
      await start(page);
      await focusEditor(page);
      await page.keyboard.insertText(text);
      await action(page, 'Page preview', touch);
      await expect(page.locator('#writer-page-preview')).toBeVisible();
      await expect(page.locator('#writer-page-preview')).toBeFocused();
      await openMenu(page, touch);
      await expect(menu(page).getByRole('button', { name: 'Page preview', exact: true })).toHaveAttribute('aria-pressed', 'true');
      await action(page, 'Page preview', touch);
      await expect(page.locator('#writer-page-preview')).toHaveCount(0);
      await expect(input(page)).toBeFocused();
      await page.keyboard.insertText('!');
      await expect(input(page)).toHaveText(`${text}!`);
    });

    test('Open file, Download original and source-aware Export preserve the exact original', async ({ page }) => {
      await start(page);
      const original = await readFile(new URL('../../../tests/fixtures/docx/supported-basic.docx', import.meta.url));
      await action(page, 'Open document', touch);
      const dialog = page.getByRole('dialog', { name: 'Open document', exact: true });
      const choosing = page.waitForEvent('filechooser');
      await activate(dialog.getByRole('button', { name: 'Open file…', exact: true }), touch);
      await (await choosing).setFiles({ name: 'Menu original.docx', buffer: original,
        mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' });
      await expect(dialog).toHaveCount(0);
      await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      const downloading = page.waitForEvent('download');
      await action(page, 'Download original', touch);
      expect(await downloadBytes(await downloading)).toEqual(original);
      await action(page, 'Export', touch);
      const disclosure = page.getByRole('dialog', { name: 'Export Writer file', exact: true });
      await expect(disclosure).toContainText('text or metadata removed');
      const exported = page.waitForEvent('download');
      await activate(disclosure.getByRole('button', { name: 'Download Writer file', exact: true }), touch);
      const native = JSON.parse((await downloadBytes(await exported)).toString('utf8')) as { source: { bytes_base64: string } };
      expect(Buffer.from(native.source.bytes_base64, 'base64')).toEqual(original);
    });

    test('both notice links open actual bundled attribution resources', async ({ page }) => {
      await start(page);
      for (const [name, contents] of [['Font licenses', 'SIL OPEN FONT LICENSE'], ['Format library notices', 'Apache License']] as const) {
        await openMenu(page, touch);
        const pending = page.waitForEvent('popup');
        await activate(menu(page).getByRole('link', { name, exact: true }), touch);
        const popup = await pending;
        try {
          await popup.waitForLoadState();
          await expect(popup.locator('body')).toContainText(contents);
          expect(new URL(popup.url()).origin).toBe(new URL(page.url()).origin);
        } finally { await popup.close(); }
      }
    });

    test('null-target focus loss during a trusted menu activation does not swallow its click', async ({ page }) => {
      await start(page);
      await openMenu(page, touch);
      await summary(page).focus();
      const target = menu(page).getByRole('button', { name: 'Save As…', exact: true });
      // Safari can decline to focus a clicked button and blur the prior control
      // with relatedTarget=null. Reproduce only that focus policy; the activation
      // itself below remains trusted mouse/touch input, not an element.click().
      await target.evaluate((element) => element.addEventListener('pointerdown', (event) => {
        event.preventDefault();
        const focused = document.activeElement;
        if (focused instanceof HTMLElement) focused.blur();
      }, { capture: true, once: true }));
      const box = await target.boundingBox();
      if (!box) throw new Error('The menu command was not displayed');
      if (touch) await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
      else await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
      await expect(page.getByRole('dialog', { name: 'Save As', exact: true })).toBeVisible();
      await expect(menu(page)).not.toHaveAttribute('open', '');
    });

    test('all commands remain reachable in a short window and outside taps dismiss the menu', async ({ page }) => {
      await page.setViewportSize(touch ? { width: 390, height: 400 } : { width: 900, height: 320 });
      await start(page);
      await openMenu(page, touch);
      const panel = await page.locator('.document-menu-panel').boundingBox();
      if (!panel) throw new Error('The menu panel was not displayed');
      expect(panel.y).toBeGreaterThanOrEqual(0);
      expect(panel.y + panel.height).toBeLessThanOrEqual(page.viewportSize()!.height);
      expect(await page.locator('.writer-app').evaluate((element) => element.scrollTop)).toBe(0);
      const last = menu(page).getByRole('link', { name: 'Format library notices', exact: true });
      await scrollMenuToEnd(page);
      const box = await last.boundingBox();
      if (!box) throw new Error('Last menu command is not reachable');
      expect(box.y).toBeGreaterThanOrEqual(0);
      expect(box.y + box.height).toBeLessThanOrEqual(page.viewportSize()!.height);
      const pending = page.waitForEvent('popup');
      await activate(last, touch);
      await (await pending).close();
      expect(await page.locator('.writer-app').evaluate((element) => element.scrollTop)).toBe(0);
      await openMenu(page, touch);
      const outside = page.getByRole('button', { name: 'Write', exact: true });
      await activate(outside, touch);
      await expect(menu(page)).not.toHaveAttribute('open', '');
    });

    test('composition disables document mutations and commands recover after cancellation', async ({ page }) => {
      await start(page);
      await focusEditor(page);
      await page.keyboard.insertText(text);
      await saved(page, touch);
      await focusEditor(page);
      await input(page).evaluate((element) => element.dispatchEvent(new CompositionEvent('compositionstart', {
        bubbles: true, data: '',
      })));
      await expect(page.locator('.save-state')).toHaveText('Composing · not saved');
      await openMenu(page, touch);
      for (const name of ['New document', 'Open document', 'Save As…', 'Export', 'Export Word', 'Page preview', 'Close']) {
        await expect(menu(page).getByRole('button', { name, exact: true })).toBeDisabled();
      }
      await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeDisabled();
      if (touch) {
        await activate(summary(page), true);
        await page.getByRole('button', { name: 'More formatting', exact: true }).tap();
      }
      for (const name of ['Font family', 'Font size']) {
        await expect(page.getByRole('combobox', { name, exact: true })).toBeDisabled();
      }
      await expect(page.getByRole('button', { name: 'Align right', exact: true })).toBeDisabled();
      await input(page).evaluate((element) => element.dispatchEvent(new CompositionEvent('compositionend', {
        bubbles: true, data: '',
      })));
      await expect(input(page)).toHaveText(text);
      await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      for (const name of ['Font family', 'Font size']) {
        await expect(page.getByRole('combobox', { name, exact: true })).toBeEnabled();
      }
      await expect(page.getByRole('button', { name: 'Align right', exact: true })).toBeEnabled();
      if (touch) await page.getByRole('button', { name: 'More formatting', exact: true }).tap();
      await action(page, 'Save As…', touch);
      const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
      await activate(dialog.getByRole('button', { name: 'Cancel', exact: true }), touch);
      await expect(input(page)).toHaveText(text);
    });

    test('menu remains reachable above an overlay keyboard and returns to actual page typing', async ({ page }) => {
      await page.addInitScript(() => {
        const viewport = Object.assign(new EventTarget(), { offsetTop: 0, offsetLeft: 0,
          width: window.innerWidth, height: window.innerHeight, scale: 1 });
        (window as Window & { menuViewport?: typeof viewport }).menuViewport = viewport;
        Object.defineProperty(window, 'visualViewport', { configurable: true, get: () => viewport });
      });
      await start(page);
      await focusEditor(page);
      await page.keyboard.insertText(text);
      await readyPages(page);
      await page.evaluate(() => {
        const viewport = (window as Window & { menuViewport?: EventTarget & { offsetTop: number; height: number } }).menuViewport!;
        Object.assign(viewport, { offsetTop: 80, height: 380 });
        viewport.dispatchEvent(new Event('resize'));
        viewport.dispatchEvent(new Event('scroll'));
      });
      await openMenu(page, touch);
      const panel = await page.locator('.document-menu-panel').boundingBox();
      if (!panel) throw new Error('The keyboard-constrained menu was not displayed');
      expect(panel.y).toBeGreaterThanOrEqual(88);
      expect(panel.y + panel.height).toBeLessThanOrEqual(452);
      await scrollMenuToEnd(page);
      const pending = page.waitForEvent('popup');
      await activate(menu(page).getByRole('link', { name: 'Format library notices', exact: true }), touch);
      await (await pending).close();
      if (await menu(page).evaluate((element) => element.hasAttribute('open'))) await activate(summary(page), touch);
      const box = await page.locator('svg[data-layout-page]').first().boundingBox();
      if (!box) throw new Error('The document page is not displayed');
      const x = box.x + box.width * 0.2;
      const y = box.y + box.width * 0.15;
      if (touch) await page.touchscreen.tap(x, y); else await page.mouse.click(x, y);
      await expect(input(page)).toBeFocused();
      await page.keyboard.press('Control+End');
      await page.keyboard.insertText(' — resumed');
      await expect(input(page)).toHaveText(`${text} — resumed`);
    });

    if (touch) test('the formatting three-dot expands working font, size and alignment controls', async ({ page }) => {
      await start(page);
      await focusEditor(page);
      await page.keyboard.insertText(text);
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('Control+Shift+End');
      const more = page.getByRole('button', { name: 'More formatting', exact: true });
      await expect(more).toHaveAttribute('aria-expanded', 'false');
      await more.tap();
      await expect(more).toHaveAttribute('aria-expanded', 'true');
      await page.getByRole('combobox', { name: 'Font family', exact: true }).selectOption('Tinos');
      await page.getByRole('combobox', { name: 'Font size', exact: true }).selectOption('18');
      await page.getByRole('button', { name: 'Align right', exact: true }).tap();
      await expect(page.getByRole('button', { name: 'Align right', exact: true })).toHaveAttribute('aria-pressed', 'true');
      await expect(page.getByRole('combobox', { name: 'Font family', exact: true })).toHaveValue('Tinos');
      await expect(page.getByRole('combobox', { name: 'Font size', exact: true })).toHaveValue('18');
      await more.tap();
      await expect(more).toHaveAttribute('aria-expanded', 'false');
      await expect(page.getByRole('combobox', { name: 'Font family', exact: true })).not.toBeVisible();
      await saved(page, true);
      const pending = page.waitForEvent('download');
      await action(page, 'Export', true);
      const native = JSON.parse((await downloadBytes(await pending)).toString('utf8')) as {
        document: { sections: { blocks: { content: { format: { alignment: string };
          runs: { text: string; format: { font_family: string; font_size: number } }[] } }[] }[] };
      };
      const paragraph = native.document.sections[0]?.blocks[0]?.content;
      expect(paragraph?.format.alignment).toBe('right');
      expect(paragraph?.runs.map((run) => run.text).join('')).toBe(text);
      for (const run of paragraph?.runs ?? []) expect(run.format).toMatchObject({ font_family: 'Tinos', font_size: 36 });
    });
  });
}

test('keyboard disclosure, Escape and Tab preserve visible focus and close on leaving', async ({ page }) => {
  await start(page);
  await summary(page).focus();
  await page.keyboard.press('Enter');
  await expect(menu(page)).toHaveAttribute('open', '');
  await page.keyboard.press('Tab');
  await expect(menu(page).getByRole('button', { name: 'New document', exact: true })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(menu(page)).not.toHaveAttribute('open', '');
  await expect(summary(page)).toBeFocused();
  await page.keyboard.press('Enter');
  for (let index = 0; index < 9; index += 1) await page.keyboard.press('Tab');
  await expect(menu(page).getByRole('link', { name: 'Format library notices', exact: true })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(menu(page)).not.toHaveAttribute('open', '');
});
