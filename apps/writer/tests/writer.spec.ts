import { readFile } from 'node:fs/promises';
import { test, expect, type Page } from '@playwright/test';

interface NativeRun { text: string; format: Record<string, unknown> }
interface NativeParagraph { id: string; style: string | null; format: Record<string, unknown>; runs: NativeRun[] }
interface NativeSection { id: string; properties: Record<string, unknown> | null; blocks: { kind: string; content: NativeParagraph }[] }
interface NativeDocument {
  schema_version: number;
  id: string;
  metadata: { title: string };
  sections: NativeSection[];
  styles: Record<string, unknown>[];
}
interface NativeFile { format: string; format_version: number; document: NativeDocument }

const editor = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const paragraph = (page: Page, index: number) => editor(page).locator('p').nth(index);
const save = (page: Page) => page.getByRole('button', { name: 'Save', exact: true });

async function start(page: Page) {
  await page.goto('/');
  await expect(editor(page)).toBeVisible();
  await expect(save(page)).toBeEnabled();
}

/** Set native UTF-16 endpoints, including reverse selections across runs. */
async function select(page: Page, anchorParagraph: number, anchorOffset: number, focusParagraph = anchorParagraph, focusOffset = anchorOffset) {
  await editor(page).evaluate((element, endpoints) => {
    const paragraphs = element.querySelectorAll('p');
    function point(index: number, offset: number): { node: Node; offset: number } {
      const paragraph = paragraphs[index];
      if (!paragraph) throw new Error('Missing test paragraph');
      const walker = document.createTreeWalker(paragraph, NodeFilter.SHOW_TEXT);
      let node = walker.nextNode();
      while (node) {
        const length = node.textContent?.length ?? 0;
        if (offset <= length) return { node, offset };
        offset -= length;
        node = walker.nextNode();
      }
      if (offset !== 0) throw new Error('Invalid test UTF-16 offset');
      return { node: paragraph, offset: paragraph.childNodes.length };
    }
    (element as HTMLElement).focus();
    const anchor = point(endpoints.anchorParagraph, endpoints.anchorOffset);
    const focus = point(endpoints.focusParagraph, endpoints.focusOffset);
    window.getSelection()?.setBaseAndExtent(anchor.node, anchor.offset, focus.node, focus.offset);
  }, { anchorParagraph, anchorOffset, focusParagraph, focusOffset });
  // Let Chromium deliver selectionchange and React render the Rust selection.
  await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
}

async function paste(page: Page, plain: string, html = '<b>untrusted rich HTML</b>') {
  await editor(page).evaluate((element, data) => {
    const clipboard = new DataTransfer();
    clipboard.setData('text/plain', data.plain);
    clipboard.setData('text/html', data.html);
    element.dispatchEvent(new ClipboardEvent('paste', { clipboardData: clipboard, bubbles: true, cancelable: true }));
  }, { plain, html });
}

async function exported(page: Page): Promise<NativeFile> {
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export', exact: true }).click();
  const file = await download;
  expect(file.suggestedFilename()).toMatch(/\.writer$/);
  const path = await file.path();
  if (!path) throw new Error('Browser did not return downloaded native file');
  return JSON.parse(await readFile(path, 'utf8')) as NativeFile;
}

async function importNative(page: Page, contents: unknown, name = 'sample.writer') {
  await page.getByLabel('Import Writer file', { exact: true }).setInputFiles({ name, mimeType: 'application/json', buffer: Buffer.from(typeof contents === 'string' ? contents : JSON.stringify(contents)) });
}

async function fixture(): Promise<NativeFile> {
  return JSON.parse(await readFile(new URL('../../../tests/fixtures/native/writer-v1.writer', import.meta.url), 'utf8')) as NativeFile;
}

test('production WebAssembly initializes and keyboard editing uses engine history', async ({ page }) => {
  const failures: string[] = [];
  page.on('pageerror', (error) => failures.push(error.message));
  const wasm = page.waitForResponse((response) => response.url().endsWith('.wasm'));
  await start(page);
  const response = await wasm;
  expect(response.ok()).toBe(true);
  expect(response.headers()['content-type']).toMatch(/application\/wasm/);
  await page.getByRole('textbox', { name: 'Document title' }).fill('Research notes');
  await editor(page).click();
  await page.keyboard.type('A dependable beginning.');
  await page.keyboard.press('Enter');
  await page.keyboard.type('Second paragraph');
  await expect(editor(page).locator('p')).toHaveCount(2);
  await expect(paragraph(page, 0)).toHaveText('A dependable beginning.');
  await expect(paragraph(page, 1)).toHaveText('Second paragraph');
  await page.keyboard.press('Backspace');
  await expect(paragraph(page, 1)).toHaveText('Second paragrap');
  await page.keyboard.press('Control+z');
  await expect(paragraph(page, 1)).toHaveText('Second paragraph');
  await page.keyboard.press('Control+Shift+z');
  await expect(paragraph(page, 1)).toHaveText('Second paragrap');
  await page.keyboard.press('Home');
  await page.keyboard.press('Backspace');
  await expect(editor(page).locator('p')).toHaveCount(1);
  await expect(paragraph(page, 0)).toHaveText('A dependable beginning.Second paragrap');
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(editor(page).locator('p')).toHaveCount(2);
  expect(failures).toEqual([]);
});

test('mixed formatting, directed selections, font controls and paragraph alignment', async ({ page }) => {
  await start(page);
  await editor(page).click();
  await page.keyboard.type('Plain and styled');
  await select(page, 0, 16, 0, 10);
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  await page.getByRole('button', { name: 'Italic', exact: true }).click();
  await page.getByRole('button', { name: 'Underline', exact: true }).click();
  const family = page.getByRole('combobox', { name: 'Font family' });
  await family.focus();
  await family.selectOption('Georgia');
  await expect(family).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(family).toHaveValue('Times New Roman');
  await expect(family).toBeFocused();
  await page.keyboard.press('ArrowUp');
  await expect(family).toHaveValue('Georgia');
  const size = page.getByRole('combobox', { name: 'Font size' });
  await size.focus();
  await size.selectOption('18');
  await expect(size).toBeFocused();
  await expect(paragraph(page, 0).locator('span').last()).toHaveCSS('font-weight', '700');
  await expect(paragraph(page, 0).locator('span').last()).toHaveCSS('font-style', 'italic');
  await expect(paragraph(page, 0).locator('span').last()).toHaveCSS('text-decoration-line', 'underline');
  await expect(paragraph(page, 0).locator('span').last()).toHaveCSS('font-size', '24px');
  await page.getByRole('button', { name: 'Align center' }).click();
  await expect(paragraph(page, 0)).toHaveCSS('text-align', 'center');
  await select(page, 0, 0, 0, 16);
  await expect(page.getByRole('button', { name: 'Bold', exact: true })).toHaveAttribute('aria-pressed', 'mixed');
  const native = await exported(page);
  const runs = native.document.sections[0]?.blocks[0]?.content.runs;
  expect(runs?.map((run) => run.text).join('')).toBe('Plain and styled');
  expect(runs?.filter((run) => run.format.bold === true && run.format.italic === true && run.format.underline === 'single').map((run) => run.text).join('')).toBe('styled');
  await select(page, 0, 16);
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('Shift+ArrowLeft');
  await page.keyboard.type('X');
  await expect(paragraph(page, 0)).toHaveText('Plain and stylXd');
});

test('Unicode deletion treats combining marks, emoji and flags as graphemes', async ({ page }) => {
  await start(page);
  await editor(page).click();
  await page.keyboard.insertText('Cafe\u0301 👩🏽‍💻 🇺🇳');
  await expect(paragraph(page, 0)).toHaveText('Cafe\u0301 👩🏽‍💻 🇺🇳');
  await page.keyboard.press('Backspace');
  await expect(paragraph(page, 0)).toHaveText('Cafe\u0301 👩🏽‍💻 ');
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Backspace');
  await expect(paragraph(page, 0)).toHaveText('Cafe\u0301 ');
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Backspace');
  await expect(paragraph(page, 0)).toHaveText('Caf');
  await page.keyboard.press('Control+z');
  await expect(paragraph(page, 0)).toHaveText('Cafe\u0301');
  await select(page, 0, 0);
  await page.keyboard.press('Delete');
  await expect(paragraph(page, 0)).toHaveText('afe\u0301');
});

test('native arrows cross graphemes split across differently formatted runs', async ({ page }) => {
  await start(page);
  const native: NativeFile = { format: 'office-writer', format_version: 1, document: {
    schema_version: 1, id: 'split-grapheme-document', metadata: { title: 'Split graphemes' }, styles: [],
    sections: [{ id: 'section', properties: null, blocks: [{ kind: 'paragraph', content: {
      id: 'opaque "paragraph"]👩🏽‍💻', style: null, format: {}, runs: [
        { text: 'Ae', format: {} }, { text: '\u0301', format: { bold: true } },
        { text: '👩', format: {} }, { text: '🏽‍💻', format: { italic: true } }, { text: 'Z', format: {} },
      ],
    } }] }],
  } };
  await importNative(page, native);
  await expect(paragraph(page, 0)).toHaveText('Ae\u0301👩🏽‍💻Z');
  await select(page, 0, 3);
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.insertText('X');
  await expect(paragraph(page, 0)).toHaveText('AXe\u0301👩🏽‍💻Z');
  await page.keyboard.press('Control+z');
  await select(page, 0, 10);
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.insertText('Y');
  await expect(paragraph(page, 0)).toHaveText('Ae\u0301Y👩🏽‍💻Z');
  await page.keyboard.press('Control+z');
  await select(page, 0, 3);
  await page.keyboard.press('Shift+ArrowLeft');
  await expect.poll(() => page.evaluate(() => window.getSelection()?.toString())).toBe('e\u0301');
  await page.keyboard.press('Shift+ArrowRight');
  await expect.poll(() => page.evaluate(() => window.getSelection()?.isCollapsed)).toBe(true);
  await page.keyboard.insertText('C');
  await expect(paragraph(page, 0)).toHaveText('Ae\u0301C👩🏽‍💻Z');
  await page.keyboard.press('Control+z');
  await select(page, 0, 10);
  await page.keyboard.press('Shift+ArrowLeft');
  await expect.poll(() => page.evaluate(() => window.getSelection()?.toString())).toBe('👩🏽‍💻');
  await page.keyboard.press('Shift+ArrowRight');
  await expect.poll(() => page.evaluate(() => window.getSelection()?.isCollapsed)).toBe(true);
  await page.keyboard.insertText('D');
  await expect(paragraph(page, 0)).toHaveText('Ae\u0301👩🏽‍💻DZ');
});

test('plain-text multiline paste is one atomic undo step and ignores rich HTML', async ({ page, context }) => {
  await start(page);
  await editor(page).click();
  await page.keyboard.type('Before ');
  await paste(page, 'First\r\nSecond 👩🏽‍💻\rThird e\u0301');
  await expect(editor(page).locator('p')).toHaveCount(3);
  await expect(paragraph(page, 0)).toHaveText('Before First');
  await expect(paragraph(page, 1)).toHaveText('Second 👩🏽‍💻');
  await expect(paragraph(page, 2)).toHaveText('Third e\u0301');
  await expect(editor(page).locator('b, strong, script')).toHaveCount(0);
  await page.keyboard.press('Control+z');
  await expect(editor(page).locator('p')).toHaveCount(1);
  await expect(paragraph(page, 0)).toHaveText('Before ');
  await page.keyboard.press('Control+Shift+z');
  await expect(editor(page).locator('p')).toHaveCount(3);
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.evaluate(() => navigator.clipboard.writeText(' from the system clipboard'));
  await page.keyboard.press('Control+v');
  await expect(paragraph(page, 2)).toHaveText('Third e\u0301 from the system clipboard');
  await page.keyboard.press('Control+z');
  await expect(paragraph(page, 2)).toHaveText('Third e\u0301');
});

test('keyboard Select All replaces a multi-paragraph selection in one undo step', async ({ page }) => {
  await start(page);
  await editor(page).click();
  await paste(page, 'First paragraph\nSecond paragraph');
  await page.keyboard.press('Control+a');
  await page.keyboard.insertText('Replacement 👩🏽‍💻');
  await expect(editor(page).locator('p')).toHaveCount(1);
  await expect(paragraph(page, 0)).toHaveText('Replacement 👩🏽‍💻');
  await page.keyboard.press('Control+z');
  await expect(editor(page).locator('p')).toHaveCount(2);
  await expect(paragraph(page, 0)).toHaveText('First paragraph');
  await expect(paragraph(page, 1)).toHaveText('Second paragraph');
});

test('Chromium IME composition commits once and synthetic cancellation preserves selection', async ({ page, context }) => {
  await start(page);
  await editor(page).click();
  const cdp = await context.newCDPSession(page);
  await cdp.send('Input.imeSetComposition', { text: 'に', selectionStart: 1, selectionEnd: 1 });
  await cdp.send('Input.imeSetComposition', { text: '日本語', selectionStart: 3, selectionEnd: 3 });
  await cdp.send('Input.insertText', { text: '日本語' });
  await expect(paragraph(page, 0)).toHaveText('日本語');
  await page.keyboard.press('Control+z');
  await expect(paragraph(page, 0)).toHaveText('');
  await page.keyboard.press('Control+Shift+z');
  await select(page, 0, 0, 0, 3);
  await editor(page).evaluate((element) => {
    element.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true, data: '' }));
    element.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: '' }));
  });
  await expect(paragraph(page, 0)).toHaveText('日本語');
  await expect(page.getByRole('button', { name: 'Bold', exact: true })).toBeEnabled();
  await select(page, 0, 3);
  await editor(page).evaluate((element) => {
    element.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true, data: '' }));
    element.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: 'é' }));
    element.dispatchEvent(new InputEvent('beforeinput', { bubbles: true, cancelable: true, inputType: 'insertText', data: 'é' }));
    element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: 'é' }));
  });
  await expect(paragraph(page, 0)).toHaveText('日本語é');
  await page.keyboard.press('Control+z');
  await expect(paragraph(page, 0)).toHaveText('日本語');
  await cdp.detach();
});

test('noncancellable browser input is committed safely and unknown DOM mutations are reverted', async ({ page }) => {
  await start(page);
  await editor(page).click();
  await page.keyboard.type('cat');
  await select(page, 0, 0, 0, 3);
  await editor(page).evaluate((element) => {
    element.dispatchEvent(new InputEvent('beforeinput', { bubbles: true, cancelable: false, inputType: 'insertReplacementText', data: 'dog' }));
    const span = element.querySelector('span');
    if (span) span.textContent = 'dog';
    element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertReplacementText', data: 'dog' }));
  });
  await expect(paragraph(page, 0)).toHaveText('dog');
  await page.keyboard.press('Control+z');
  await expect(paragraph(page, 0)).toHaveText('cat');
  await editor(page).evaluate((element) => {
    element.textContent = 'unrecognized browser mutation';
    element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertFromYank' }));
  });
  await expect(paragraph(page, 0)).toHaveText('cat');
  await expect(page.getByRole('alert')).toContainText('unchanged');
});

test('native fixture, styles, sections and opaque identifiers survive import/export exactly', async ({ page }) => {
  await start(page);
  const native = await fixture();
  native.document.id = 'doc "quoted"]#<id>';
  const first = native.document.sections[0]?.blocks[0]?.content;
  if (!first) throw new Error('Missing fixture paragraph');
  first.id = 'paragraph "quotes"]#<>\\';
  await importNative(page, native, '研究👩🏽‍💻.writer');
  await expect(page.getByRole('textbox', { name: 'Document title' })).toHaveValue('Quarterly research report');
  await expect(editor(page).locator('p')).toHaveCount(3);
  await expect(paragraph(page, 0)).toHaveText('Research progress: Q3');
  await expect(paragraph(page, 0).locator('span').first()).toHaveCSS('font-weight', '700');
  const roundTrip = await exported(page);
  expect(roundTrip).toMatchObject({ ...native });
  // Serde makes unspecified optional fields explicit null; normalize via a
  // second round trip rather than comparing source JSON spelling.
  await importNative(page, roundTrip);
  expect(await exported(page)).toEqual(roundTrip);
  expect(roundTrip.document.id).toBe(native.document.id);
  expect(roundTrip.document.sections[0]?.blocks[0]?.content.id).toBe(first.id);
  expect(roundTrip.document.styles).toHaveLength(2);
  expect(roundTrip.document.sections).toHaveLength(2);
  await select(page, 0, 0, 0, 8);
  await page.getByRole('button', { name: 'Italic', exact: true }).click();
  await expect(paragraph(page, 0).locator('span').first()).toHaveCSS('font-style', 'italic');
});

test('unsupported cross-section edits fail without changing the document', async ({ page }) => {
  await start(page);
  await importNative(page, await fixture());
  await expect(editor(page).locator('p')).toHaveCount(3);
  const before = await exported(page);
  await select(page, 0, 5, 2, 5);
  await page.keyboard.insertText('Rejected replacement');
  await expect(page.getByRole('alert')).toContainText('section');
  await expect(editor(page).locator('p')).toHaveCount(3);
  expect(await exported(page)).toEqual(before);
  await expect(page.getByRole('button', { name: 'Undo', exact: true })).toBeDisabled();
});

test('a structurally empty native document stays empty until the first transaction', async ({ page }) => {
  await start(page);
  const native: NativeFile = { format: 'office-writer', format_version: 1, document: { schema_version: 1, id: 'empty-import', metadata: { title: 'Empty document' }, sections: [], styles: [] } };
  await importNative(page, native);
  await expect(page.getByRole('textbox', { name: 'Document title' })).toHaveValue('Empty document');
  expect((await exported(page)).document.sections).toEqual([]);
  await editor(page).click();
  await editor(page).evaluate((element) => {
    const placeholder = element.querySelector('p');
    if (placeholder) placeholder.textContent = 'uncommitted browser text';
    element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertFromYank' }));
  });
  await expect(paragraph(page, 0)).toHaveText('');
  await expect(page.getByRole('alert')).toContainText('unchanged');
  expect((await exported(page)).document.sections).toEqual([]);
  await editor(page).click();
  await editor(page).evaluate((element) => {
    element.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true, data: '' }));
    const placeholder = element.querySelector('p');
    if (placeholder) placeholder.textContent = 'IME preview';
    element.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: '' }));
  });
  await expect(paragraph(page, 0)).toHaveText('');
  expect((await exported(page)).document.sections).toEqual([]);
  await editor(page).click();
  await page.keyboard.insertText('First 👩🏽‍💻');
  await expect(paragraph(page, 0)).toHaveText('First 👩🏽‍💻');
  expect((await exported(page)).document.sections).toHaveLength(1);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  expect((await exported(page)).document.sections).toEqual([]);
});

test('explicit local saving supports close, reopen, reload and native export', async ({ page }) => {
  await start(page);
  await page.getByRole('textbox', { name: 'Document title' }).fill('Local research — 日本語');
  await editor(page).click();
  await page.keyboard.insertText('Cafe\u0301 and 👩🏽‍💻');
  await select(page, 0, 0, 0, 5);
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  await save(page).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const before = await exported(page);
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Your document is closed' })).toBeVisible();
  await page.getByRole('button', { name: 'Open document', exact: true }).click();
  await page.getByRole('button', { name: /Local research — 日本語/ }).click();
  await expect(paragraph(page, 0)).toHaveText('Cafe\u0301 and 👩🏽‍💻');
  expect(await exported(page)).toEqual(before);
  await page.reload();
  await expect(editor(page)).toBeVisible();
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  await page.getByRole('button', { name: /Local research — 日本語/ }).click();
  expect(await exported(page)).toEqual(before);
});

test('corrupt and future native imports preserve the current document and unsaved guards', async ({ page }) => {
  await start(page);
  await editor(page).click();
  await page.keyboard.type('Keep this work');
  await page.getByRole('button', { name: 'New', exact: true }).click();
  await expect(page.getByRole('dialog', { name: 'Save your changes?' })).toBeVisible();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(paragraph(page, 0)).toHaveText('Keep this work');
  await save(page).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  await importNative(page, '{"format":');
  await expect(page.getByRole('alert')).toContainText('Could not open');
  await expect(paragraph(page, 0)).toHaveText('Keep this work');
  const native = await fixture();
  native.format_version = 999;
  await importNative(page, native);
  await expect(page.getByRole('alert')).toContainText('999');
  await expect(paragraph(page, 0)).toHaveText('Keep this work');
});

test('storage I/O failure retains unsaved work and the previously committed record', async ({ page, context }) => {
  await start(page);
  await page.getByRole('textbox', { name: 'Document title' }).fill('Durable original');
  await editor(page).click();
  await page.keyboard.type('Original');
  await save(page).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  await select(page, 0, 8);
  await page.keyboard.type(' plus unsaved');
  await page.evaluate(() => {
    IDBObjectStore.prototype.put = function (): IDBRequest<IDBValidKey> {
      throw new DOMException('Test quota failure', 'QuotaExceededError');
    };
  });
  await save(page).click();
  await expect(page.getByRole('alert')).toContainText('storage is full');
  await expect(paragraph(page, 0)).toHaveText('Original plus unsaved');
  await expect(page.locator('.save-state')).toHaveText('Unsaved changes');
  const other = await context.newPage();
  await start(other);
  await other.getByRole('button', { name: 'Open', exact: true }).click();
  await other.getByRole('button', { name: /Durable original/ }).click();
  await expect(paragraph(other, 0)).toHaveText('Original');
  await expect(paragraph(page, 0)).toHaveText('Original plus unsaved');
});

test('typing during an IndexedDB save remains dirty and does not alter its byte snapshot', async ({ page, context }) => {
  await start(page);
  await page.getByRole('textbox', { name: 'Document title' }).fill('Save snapshot');
  await editor(page).click();
  await page.keyboard.type('Captured');
  await page.evaluate(() => {
    const original = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (value: unknown): IDBRequest<IDBValidKey> {
      IDBObjectStore.prototype.put = original;
      const request = original.call(this, value);
      // Deliver another browser input while the real save transaction is active,
      // after its byte snapshot was captured and before transaction completion.
      document.querySelector('[data-testid="editor"]')?.dispatchEvent(new InputEvent('beforeinput', {
        bubbles: true, cancelable: true, inputType: 'insertText', data: ' plus later typing',
      }));
      return request;
    };
  });
  await save(page).click();
  await expect(paragraph(page, 0)).toHaveText('Captured plus later typing');
  await expect(page.locator('.save-state')).toHaveText('Unsaved changes');
  await expect(page.getByRole('status')).toContainText('Earlier version saved');
  const other = await context.newPage();
  await start(other);
  await other.getByRole('button', { name: 'Open', exact: true }).click();
  await other.getByRole('button', { name: /Save snapshot/ }).click();
  await expect(paragraph(other, 0)).toHaveText('Captured');
});

test('two tabs cannot overwrite a newer saved version and can save a separate copy', async ({ page, context }) => {
  await start(page);
  await page.getByRole('textbox', { name: 'Document title' }).fill('Shared local document');
  await editor(page).click();
  await page.keyboard.type('Original');
  await save(page).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  const second = await context.newPage();
  await start(second);
  await second.getByRole('button', { name: 'Open', exact: true }).click();
  await second.getByRole('button', { name: /Shared local document/ }).click();
  await expect(paragraph(second, 0)).toHaveText('Original');
  await select(page, 0, 8);
  await page.keyboard.type(' first tab');
  await save(page).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  await select(second, 0, 8);
  await second.keyboard.type(' second tab');
  await save(second).click();
  await expect(second.getByRole('alert')).toContainText('Another saved version');
  await expect(paragraph(second, 0)).toHaveText('Original second tab');
  await second.getByRole('button', { name: 'Save a separate copy' }).click();
  await expect(second.locator('.save-state')).toHaveText('Saved on this device');
  const copy = await exported(second);
  const original = await exported(page);
  expect(copy.document.id).not.toBe(original.document.id);
  await second.getByRole('button', { name: 'Open', exact: true }).click();
  await expect(second.locator('.library-entry')).toHaveCount(2);
});

test('measure Rust-backed typing latency on a realistic ten-thousand-character document', async ({ page }, testInfo) => {
  await start(page);
  await editor(page).click();
  const text = 'Review document integrity, international text, and local save recovery. Preserve formatting and stable identifiers. ';
  const content = Array.from({ length: 50 }, (_, index) => `${index + 1}. ${text}${text}`).join('\n');
  await paste(page, content);
  await expect(editor(page).locator('p')).toHaveCount(50);
  await editor(page).evaluate((element) => {
    const samples: number[] = [];
    const starts = new WeakMap<Event, number>();
    Object.assign(window, { writerTypingSamples: samples });
    element.addEventListener('beforeinput', (event) => starts.set(event, performance.now()), true);
    document.addEventListener('beforeinput', (event) => {
      const start = starts.get(event);
      // The surface's synchronous Rust transaction and DOM reconciliation have
      // completed when this same native event bubbles to the document.
      if (start !== undefined) samples.push(performance.now() - start);
    });
  });
  const typed = 'Responsiveness measurement....';
  await page.keyboard.type(typed);
  await expect(paragraph(page, 49)).toContainText(typed);
  const caret = await page.evaluate(() => {
    const selection = window.getSelection();
    const rect = selection?.rangeCount ? selection.getRangeAt(0).getBoundingClientRect() : null;
    return rect ? { top: rect.top, bottom: rect.bottom, viewportHeight: window.innerHeight } : null;
  });
  expect(caret).not.toBeNull();
  expect(caret?.top).toBeGreaterThanOrEqual(0);
  expect(caret?.bottom).toBeLessThanOrEqual(caret?.viewportHeight ?? 0);
  const samples = await page.evaluate(() => (window as Window & { writerTypingSamples?: number[] }).writerTypingSamples ?? []);
  expect(samples).toHaveLength(typed.length);
  expect(samples.every((value) => Number.isFinite(value) && value >= 0)).toBe(true);
  const sorted = [...samples].sort((left, right) => left - right);
  const result = {
    browser: 'Chromium', charactersBeforeTyping: content.length, paragraphs: 50,
    events: samples.length, p50Ms: sorted[Math.floor(sorted.length * .5)],
    p95Ms: sorted[Math.floor(sorted.length * .95)], maxMs: sorted.at(-1),
    scope: 'Native beforeinput capture through synchronous Rust transaction and DOM reconciliation; not OS input-to-paint latency.',
  };
  await testInfo.attach('typing-latency', { body: JSON.stringify(result, null, 2), contentType: 'application/json' });
  console.log(`Writer typing measurement: ${JSON.stringify(result)}`);
});

test('responsive layout, semantic controls and actual running application screenshots', async ({ page }) => {
  await start(page);
  await page.getByRole('textbox', { name: 'Document title' }).fill('Quarterly planning notes');
  await editor(page).click();
  await paste(page, 'Quarterly planning notes\nThe next review will focus on native document fidelity, dependable editing, and local save recovery. Each change should preserve the existing document structure.\nReview priorities\nConfirm Unicode behavior — Cafe\u0301, 日本語, 👩🏽‍💻 — and verify that formatting survives a complete save and reopen cycle.');
  await select(page, 0, 0, 0, 24);
  await page.getByRole('combobox', { name: 'Font family' }).selectOption('Georgia');
  await page.getByRole('combobox', { name: 'Font size' }).selectOption('24');
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  await select(page, 2, 0, 2, 17);
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  await save(page).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  await select(page, 3, 0);
  await page.getByRole('button', { name: 'Save', exact: true }).focus();
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.screenshot({ path: '../../docs/images/writer-desktop.png', fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeVisible();
  await expect(editor(page)).toHaveAttribute('aria-multiline', 'true');
  await page.getByRole('button', { name: 'Bold', exact: true }).focus();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('button', { name: 'Italic', exact: true })).toBeFocused();
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.screenshot({ path: '../../docs/images/writer-mobile.png', fullPage: true });
});
