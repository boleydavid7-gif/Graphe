import { readFile } from 'node:fs/promises';
import { expect, test, type Locator, type Page } from '@playwright/test';
import { documentAction, focusEditor, readyPages } from './step7-test-helpers';

// These are real-browser tests against the compiled Rust/WASM application.
// Mobile coverage uses browser touch emulation, not a physical iOS keyboard.
interface NativeRun { readonly text: string; readonly format: Record<string, unknown> }
interface NativeParagraph { readonly id: string; readonly runs: readonly NativeRun[] }
interface NativeCell { readonly id: string; readonly paragraphs: readonly NativeParagraph[] }
interface NativeRow { readonly id: string; readonly cells: readonly NativeCell[] }
interface NativeTable { readonly column_widths?: readonly number[]; readonly id: string; readonly rows: readonly NativeRow[] }
interface NativeFile {
  readonly document: {
    readonly id: string;
    readonly metadata: { readonly title: string };
    readonly sections: readonly {
      readonly blocks: readonly (
        | { readonly kind: 'paragraph'; readonly content: NativeParagraph }
        | { readonly kind: 'table'; readonly content: NativeTable }
      )[];
    }[];
  };
  readonly source?: { readonly format: string; readonly bytes_base64: string };
}

const editor = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const table = (page: Page) => editor(page).locator('table');
const cell = (page: Page, index: number) => table(page).locator('td').nth(index);
const unicode = 'Café e\u0301 👩🏽‍💻 العربية नमस्ते שלום';

async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
}

async function insertTable(page: Page, rows = 2, columns = 2, immediateText?: string) {
  await page.getByRole('button', { name: 'Insert', exact: true }).click();
  await page.getByRole('button', { name: 'Table…', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Insert table', exact: true });
  await expect(dialog).toBeVisible();
  await dialog.getByRole('spinbutton', { name: 'Rows', exact: true }).fill(String(rows));
  await dialog.getByRole('spinbutton', { name: 'Columns', exact: true }).fill(String(columns));
  await dialog.getByRole('button', { name: 'Insert table', exact: true }).click();
  if (immediateText !== undefined) {
    // Do not repair focus or wait for a layout pass before the first key. The
    // actual dialog workflow must return input to the newly created Rust cell.
    await page.keyboard.insertText(immediateText);
    await expect(editor(page)).toBeFocused();
  }
  await expect(dialog).toHaveCount(0);
  await expect(table(page).locator('tr')).toHaveCount(rows);
  await expect(table(page).locator('td')).toHaveCount(rows * columns);
  await readyPages(page);
}

async function structuralAction(page: Page, name: string) {
  await page.getByRole('button', { name: 'Insert', exact: true }).click();
  const command = page.getByRole('button', { name, exact: true });
  if (!(await command.isVisible())) await page.getByLabel('More table commands', { exact: true }).click();
  await command.click();
  await readyPages(page);
}

async function saveLocal(page: Page) {
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device', { timeout: 10_000 });
}

async function nativeFile(page: Page): Promise<NativeFile> {
  const pending = page.waitForEvent('download');
  await documentAction(page, 'Export');
  const disclosure = page.getByRole('dialog', { name: 'Export Writer file', exact: true });
  if (await disclosure.isVisible()) {
    await expect(disclosure).toContainText(/original/i);
    await disclosure.getByRole('button', { name: 'Download Writer file', exact: true }).click();
  }
  const download = await pending;
  expect(download.suggestedFilename()).toMatch(/\.writer$/i);
  const path = await download.path();
  if (!path) throw new Error('The browser did not retain the native table download');
  return JSON.parse(await readFile(path, 'utf8')) as NativeFile;
}

async function externalCopy(page: Page, format: 'docx' | 'odt') {
  await documentAction(page, 'Save As…');
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption(format);
  await expect(dialog.getByRole('button', { name: 'Save copy', exact: true })).toBeEnabled();
  const pending = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Save copy', exact: true }).click();
  const download = await pending;
  const path = await download.path();
  if (!path) throw new Error(`Missing ${format} table download`);
  const bytes = await readFile(path);
  expect(bytes.subarray(0, 2).toString()).toBe('PK');
  return bytes;
}

async function openExternalCopy(page: Page, format: 'docx' | 'odt', bytes: Buffer) {
  await documentAction(page, 'Open document');
  const library = page.getByRole('dialog', { name: 'Open document', exact: true });
  const chosen = page.waitForEvent('filechooser');
  await library.getByRole('button', { name: 'Open file…', exact: true }).click();
  await (await chosen).setFiles({ name: `table.${format}`, mimeType: format === 'docx'
    ? 'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
    : 'application/vnd.oasis.opendocument.text', buffer: bytes });
  await expect(library).toHaveCount(0);
}

function nativeTable(file: NativeFile): NativeTable {
  for (const section of file.document.sections) {
    for (const block of section.blocks) if (block.kind === 'table') return block.content;
  }
  throw new Error('The Rust native document contains no table');
}

function cellTexts(value: NativeTable) {
  return value.rows.map((row) => row.cells.map((entry) => entry.paragraphs.map((paragraph) =>
    paragraph.runs.map((run) => run.text).join('')).join('\n')));
}

/** Native DOM UTF-16 endpoints are translated and checked by the Rust adapter. */
async function selectParagraphs(page: Page, anchor: Locator, anchorOffset: number, focus = anchor, focusOffset = anchorOffset) {
  await readyPages(page);
  const anchorId = await anchor.getAttribute('data-paragraph-id');
  const focusId = await focus.getAttribute('data-paragraph-id');
  if (!anchorId || !focusId) throw new Error('Missing stable table paragraph identity');
  await editor(page).evaluate((element, endpoints) => {
    const paragraphs = [...element.querySelectorAll('p[data-paragraph-id]')];
    function point(id: string, offset: number): { node: Node; offset: number } {
      const paragraph = paragraphs.find((entry) => entry.getAttribute('data-paragraph-id') === id);
      if (!paragraph) throw new Error('Missing selected cell paragraph');
      const walker = document.createTreeWalker(paragraph, NodeFilter.SHOW_TEXT);
      let node = walker.nextNode();
      while (node) {
        const length = node.textContent?.length ?? 0;
        if (offset <= length) return { node, offset };
        offset -= length;
        node = walker.nextNode();
      }
      if (offset !== 0) throw new Error('Invalid table test UTF-16 boundary');
      return { node: paragraph, offset: paragraph.childNodes.length };
    }
    (element as HTMLElement).focus();
    const first = point(endpoints.anchorId, endpoints.anchorOffset);
    const last = point(endpoints.focusId, endpoints.focusOffset);
    window.getSelection()?.setBaseAndExtent(first.node, first.offset, last.node, last.offset);
  }, { anchorId, anchorOffset, focusId, focusOffset });
  await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
}

async function selectCell(page: Page, index: number) {
  const paragraph = cell(page, index).locator('p').first();
  await selectParagraphs(page, paragraph, 0);
}

test('table insertion, Unicode, cell navigation, paragraph breaks and formatting use Rust history', async ({ page }) => {
  const failures: string[] = [];
  page.on('pageerror', (error) => failures.push(error.message));
  await start(page);
  await insertTable(page, 2, 2, unicode);
  await expect(cell(page, 0).locator('p')).toHaveText(unicode);
  await page.keyboard.press('Tab');
  await page.keyboard.insertText('Second cell');
  await expect(cell(page, 1).locator('p')).toHaveText('Second cell');
  await page.keyboard.press('Shift+Tab');
  await expect(page.locator('.table-position')).toHaveText('Row 1, column 1');
  await selectParagraphs(page, cell(page, 0).locator('p'), unicode.length);
  await page.keyboard.press('Enter');
  await page.keyboard.insertText('Another paragraph');
  await expect(cell(page, 0).locator('p')).toHaveCount(2);
  await expect(cell(page, 0).locator('p').last()).toHaveText('Another paragraph');
  await selectParagraphs(page, cell(page, 1).locator('p'), 0, cell(page, 1).locator('p'), 'Second cell'.length);
  await page.getByRole('button', { name: 'Write', exact: true }).click();
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  const formatted = nativeTable(await nativeFile(page));
  expect(cellTexts(formatted)).toEqual([[`${unicode}\nAnother paragraph`, 'Second cell'], ['', '']]);
  expect(formatted.rows[0]?.cells[1]?.paragraphs[0]?.runs.filter((run) => run.text.length)
    .every((run) => run.format.bold === true)).toBe(true);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  const undone = nativeTable(await nativeFile(page));
  expect(undone.rows[0]?.cells[1]?.paragraphs[0]?.runs.every((run) => run.format.bold !== true)).toBe(true);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  expect(nativeTable(await nativeFile(page))).toEqual(formatted);
  await readyPages(page);
  await expect(page.getByTestId('paged-editor').locator('rect[data-table-cell-id]')).toHaveCount(4);
  await page.getByRole('button', { name: 'Dismiss prepared export', exact: true }).click();
  await page.screenshot({ path: '../../docs/images/graphe-tables-desktop.png', fullPage: true });
  expect(failures).toEqual([]);
});

test('row and column commands preserve existing cell identities and undo exact table deletion', async ({ page }) => {
  await start(page);
  await insertTable(page);
  await focusEditor(page);
  await page.keyboard.insertText('Keep this cell');
  const original = nativeTable(await nativeFile(page));
  await structuralAction(page, 'Add row below');
  await expect(table(page).locator('tr')).toHaveCount(3);
  await selectCell(page, 0);
  await structuralAction(page, 'Add column after');
  await expect(table(page).locator('td')).toHaveCount(9);
  const expanded = nativeTable(await nativeFile(page));
  expect(expanded.id).toBe(original.id);
  expect(expanded.rows[0]?.id).toBe(original.rows[0]?.id);
  expect(expanded.rows[0]?.cells[0]).toEqual(original.rows[0]?.cells[0]);
  expect(expanded.rows[0]?.cells[2]).toEqual(original.rows[0]?.cells[1]);
  await selectCell(page, 0);
  await structuralAction(page, 'Add row above');
  await expect(table(page).locator('tr')).toHaveCount(4);
  await selectCell(page, 0);
  await structuralAction(page, 'Add column before');
  await expect(table(page).locator('td')).toHaveCount(16);
  const prefixed = nativeTable(await nativeFile(page));
  expect(prefixed.rows[1]?.cells[1]).toEqual(original.rows[0]?.cells[0]);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  expect(nativeTable(await nativeFile(page))).toEqual(expanded);
  await selectCell(page, 1);
  await structuralAction(page, 'Delete column');
  await expect(table(page).locator('td')).toHaveCount(6);
  await selectCell(page, 2);
  await structuralAction(page, 'Delete row');
  await expect(table(page).locator('tr')).toHaveCount(2);
  expect(nativeTable(await nativeFile(page))).toEqual(original);
  await selectCell(page, 0);
  await structuralAction(page, 'Delete table');
  await expect(table(page)).toHaveCount(0);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(table(page).locator('td')).toHaveCount(4);
  expect(nativeTable(await nativeFile(page))).toEqual(original);
});

test('destructive text replacement across cells fails without changing the table', async ({ page }) => {
  await start(page);
  await insertTable(page);
  await focusEditor(page);
  await page.keyboard.insertText('First');
  await page.keyboard.press('Tab');
  await page.keyboard.insertText('Second');
  const before = await nativeFile(page);
  await selectParagraphs(page, cell(page, 0).locator('p'), 1, cell(page, 1).locator('p'), 3);
  await page.keyboard.insertText('Must not replace cell boundaries');
  await expect(page.getByRole('alert')).toContainText(/cell|table|container|cross/i);
  await expect(table(page).locator('td')).toHaveCount(4);
  await expect(cell(page, 0)).toHaveText('First');
  await expect(cell(page, 1)).toHaveText('Second');
  expect((await nativeFile(page)).document).toEqual(before.document);
});

test('native save, reload and library reopen preserve complete table state and stable identities', async ({ page }) => {
  await start(page);
  await insertTable(page, 3, 2);
  await focusEditor(page);
  await page.keyboard.insertText(unicode);
  await page.keyboard.press('Tab');
  await page.keyboard.insertText('Persisted next cell');
  const title = 'Table persistence regression';
  await page.getByRole('textbox', { name: 'Document title', exact: true }).fill(title);
  await saveLocal(page);
  const before = await nativeFile(page);
  await page.reload();
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await documentAction(page, 'Open document');
  const library = page.getByRole('dialog', { name: 'Open document', exact: true });
  await library.getByRole('button', { name: new RegExp(title) }).click();
  await expect(library).toHaveCount(0);
  await expect(table(page).locator('td')).toHaveCount(6);
  await expect(cell(page, 0)).toHaveText(unicode);
  await expect(cell(page, 1)).toHaveText('Persisted next cell');
  await readyPages(page);
  expect(await nativeFile(page)).toEqual(before);
});

for (const format of ['docx', 'odt'] as const) {
  test(`${format.toUpperCase()} table export, edit and reopen preserve structure and text`, async ({ page }) => {
    await start(page);
    await insertTable(page);
    await focusEditor(page);
    await page.keyboard.insertText(unicode);
    await page.keyboard.press('Tab');
    await page.keyboard.insertText('Formatted cell');
    await selectParagraphs(page, cell(page, 1).locator('p'), 0, cell(page, 1).locator('p'), 'Formatted cell'.length);
    await page.getByRole('button', { name: 'Write', exact: true }).click();
    await page.getByRole('button', { name: 'Bold', exact: true }).click();
    await saveLocal(page);
    const bytes = await externalCopy(page, format);
    await openExternalCopy(page, format, bytes);
    await expect(table(page).locator('td')).toHaveCount(4);
    await expect(cell(page, 0)).toHaveText(unicode);
    await expect(cell(page, 1)).toHaveText('Formatted cell');
    const imported = await nativeFile(page);
    expect(imported.source?.format).toBe(format);
    expect(Buffer.from(imported.source?.bytes_base64 ?? '', 'base64')).toEqual(bytes);
    expect(nativeTable(imported).rows[0]?.cells[1]?.paragraphs[0]?.runs
      .filter((run) => run.text.length).every((run) => run.format.bold === true)).toBe(true);
    await selectCell(page, 1);
    await page.keyboard.press('End');
    await page.keyboard.insertText(' edited');
    await expect(cell(page, 1)).toHaveText('Formatted cell edited');
    await saveLocal(page);
    const currentCopy = await externalCopy(page, format);
    expect(currentCopy).not.toEqual(bytes);
    await openExternalCopy(page, format, currentCopy);
    await expect(table(page).locator('td')).toHaveCount(4);
    await expect(cell(page, 0)).toHaveText(unicode);
    await expect(cell(page, 1)).toHaveText('Formatted cell edited');
    const reopened = await nativeFile(page);
    expect(Buffer.from(reopened.source?.bytes_base64 ?? '', 'base64')).toEqual(currentCopy);
    expect(nativeTable(reopened).rows[0]?.cells[1]?.paragraphs[0]?.runs
      .filter((run) => run.text.length).every((run) => run.format.bold === true)).toBe(true);
  });
}

test('Rust paginates table rows across pages without omitting cells or overlapping columns', async ({ page }) => {
  test.setTimeout(120_000);
  await start(page);
  await insertTable(page, 20, 2);
  await focusEditor(page);
  // Four lines per row force real pagination within the UI's bounded 20-row
  // insertion limit. Each input is processed by Rust, not a generated JS model.
  for (let row = 0; row < 20; row += 1) {
    await page.keyboard.insertText(`Row ${row + 1}: first line`);
    for (const text of ['Second line', 'Third line', 'Fourth line']) {
      await page.keyboard.press('Enter');
      await page.keyboard.insertText(text);
    }
    if (row < 19) {
      await page.keyboard.press('Tab');
      await page.keyboard.press('Tab');
    }
  }
  const root = await readyPages(page);
  expect(await root.locator('svg[data-layout-page]').count()).toBeGreaterThan(1);
  const bounds = await root.locator('rect[data-table-cell-id]').evaluateAll((elements) => elements.map((element) => ({
    id: element.getAttribute('data-table-cell-id'), x: Number(element.getAttribute('x')),
    y: Number(element.getAttribute('y')), width: Number(element.getAttribute('width')),
    height: Number(element.getAttribute('height')), page: element.closest('svg')?.getAttribute('data-layout-page'),
  })));
  expect(bounds).toHaveLength(40);
  expect(new Set(bounds.map((entry) => entry.id)).size).toBe(40);
  expect(bounds.every((entry) => entry.width > 0 && entry.height > 0)).toBe(true);
  for (let row = 0; row < 20; row += 1) {
    const left = bounds[row * 2];
    const right = bounds[row * 2 + 1];
    expect(right?.page).toBe(left?.page);
    expect(right?.y).toBe(left?.y);
    expect(right?.x).toBeGreaterThanOrEqual((left?.x ?? 0) + (left?.width ?? 0) - 1);
  }
  await selectCell(page, 39);
  await page.keyboard.insertText('Final page cell');
  await expect(cell(page, 39)).toHaveText('Final page cell');
  await readyPages(page);
  const finalId = await cell(page, 39).locator('p').getAttribute('data-paragraph-id');
  expect(await root.locator('[data-layout-paragraph-id]').evaluateAll((lines, id) => lines
    .filter((line) => line.getAttribute('data-layout-paragraph-id') === id)
    .some((line) => line.querySelectorAll('use').length > 0), finalId)).toBe(true);
  expect(cellTexts(nativeTable(await nativeFile(page)))[19]?.[1]).toBe('Final page cell');
});

test('synthetic IME composition commits once in a nonfirst cell and blocks structural commands while composing', async ({ page }) => {
  await start(page);
  await insertTable(page);
  await selectCell(page, 3);
  await page.keyboard.insertText('Before ');
  const committed = '日本語 e\u0301 👩🏽‍💻';
  await editor(page).evaluate((element, text) => {
    element.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true, data: '' }));
    const paragraph = element.querySelectorAll('td')[3]?.querySelector('p');
    if (!paragraph) throw new Error('Missing composition cell');
    paragraph.appendChild(document.createTextNode(text));
    element.dispatchEvent(new CompositionEvent('compositionupdate', { bubbles: true, data: text }));
    element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertCompositionText',
      data: text, isComposing: true }));
  }, committed);
  await expect(page.locator('.save-state')).toHaveText('Composing · not saved');
  await expect(page.getByRole('button', { name: 'Add row below', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Add column after', exact: true })).toBeDisabled();
  await editor(page).evaluate((element, text) => {
    element.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: text }));
    // Some browser engines emit a trailing input in the same task. It must not
    // duplicate the Rust composition commit. This is a synthetic event probe,
    // not verification of an operating system's candidate window or keyboard.
    element.dispatchEvent(new InputEvent('beforeinput', { bubbles: true, cancelable: true,
      inputType: 'insertText', data: text }));
    element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: text }));
  }, committed);
  await expect(cell(page, 3)).toHaveText(`Before ${committed}`);
  await expect(cell(page, 0)).toHaveText('');
  await expect(page.getByRole('button', { name: 'Add row below', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(cell(page, 3)).toHaveText('Before ');
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  expect(cellTexts(nativeTable(await nativeFile(page)))).toEqual([['', ''], ['', `Before ${committed}`]]);
});

test('surviving cell paragraph and text DOM nodes remain attached through structural changes and undo', async ({ page }) => {
  await start(page);
  await insertTable(page);
  await selectCell(page, 3);
  await page.keyboard.insertText('Retain this exact text node e\u0301');
  const nodes = await cell(page, 3).locator('p').evaluateHandle((paragraph) => ({
    paragraph, text: paragraph.querySelector('span')?.firstChild,
  }));
  const remainsAttached = () => nodes.evaluate(({ paragraph, text }) => !!text
    && paragraph.isConnected && text.isConnected && paragraph.contains(text)
    && paragraph.querySelector('span')?.firstChild === text);
  expect(await remainsAttached()).toBe(true);
  await selectCell(page, 0);
  await structuralAction(page, 'Add row above');
  expect(await remainsAttached()).toBe(true);
  await structuralAction(page, 'Add column before');
  expect(await remainsAttached()).toBe(true);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await readyPages(page);
  expect(await remainsAttached()).toBe(true);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await readyPages(page);
  expect(await remainsAttached()).toBe(true);
  await expect(table(page).locator('td')).toHaveCount(4);
  await expect(cell(page, 3)).toHaveText('Retain this exact text node e\u0301');
  await nodes.dispose();
});

test('native table, tbody, row and cell container endpoints retain destructive selection safeguards', async ({ page }) => {
  await start(page);
  await insertTable(page);
  await focusEditor(page);
  for (const [index, text] of ['One', 'Two', 'Three', 'Four'].entries()) {
    if (index) await page.keyboard.press('Tab');
    await page.keyboard.insertText(text);
  }
  const before = nativeTable(await nativeFile(page));
  async function selectContainer(selector: string) {
    await readyPages(page);
    await editor(page).evaluate((element, selected) => {
      const container = element.querySelector(selected);
      if (!container) throw new Error('Missing native table selection container');
      (element as HTMLElement).focus();
      window.getSelection()?.setBaseAndExtent(container, 0, container, container.childNodes.length);
    }, selector);
    await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  }
  for (const selector of ['table', 'table tbody', 'table tr']) {
    await selectContainer(selector);
    if (selector.endsWith('tr')) {
      const copied = await editor(page).evaluate((element) => {
        const data = new DataTransfer();
        element.dispatchEvent(new ClipboardEvent('cut', { bubbles: true, cancelable: true, clipboardData: data }));
        return data.getData('text/plain');
      });
      expect(copied).toContain('One');
      expect(copied).toContain('Two');
    } else await page.keyboard.press('Delete');
    await expect(page.getByRole('alert')).toContainText(/cell|table|container|cross/i);
    expect(nativeTable(await nativeFile(page))).toEqual(before);
    const dismiss = page.getByRole('button', { name: 'Dismiss error', exact: true });
    if (await dismiss.isVisible()) await dismiss.click();
  }
  await selectContainer('table td');
  const copied = await editor(page).evaluate((element) => {
    const data = new DataTransfer();
    element.dispatchEvent(new ClipboardEvent('cut', { bubbles: true, cancelable: true, clipboardData: data }));
    return data.getData('text/plain');
  });
  expect(copied).toBe('One');
  await expect(cell(page, 0)).toHaveText('');
  await expect(cell(page, 1)).toHaveText('Two');
  await expect(cell(page, 2)).toHaveText('Three');
  await expect(cell(page, 3)).toHaveText('Four');
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  expect(nativeTable(await nativeFile(page))).toEqual(before);
});

test.describe('mobile table editing', () => {
  test.skip(({ browserName }) => browserName === 'firefox', 'Playwright Firefox does not support mobile device emulation.');
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

  test('tapping a Rust cell focuses input and typing preserves cell boundaries in a narrow viewport', async ({ page }) => {
    await start(page);
    await insertTable(page);
    await page.getByRole('textbox', { name: 'Document title', exact: true }).focus();
    const root = await readyPages(page);
    const displayed = root.locator('rect[data-table-cell-id]').nth(1);
    await displayed.scrollIntoViewIfNeeded();
    const point = await displayed.evaluate((element) => {
      if (!(element instanceof SVGGraphicsElement)) throw new Error('Missing Rust cell geometry');
      const matrix = element.getScreenCTM();
      if (!matrix) throw new Error('Missing SVG cell transform');
      const x = Number(element.getAttribute('x')) + 8 * 64;
      const y = Number(element.getAttribute('y')) + 8 * 64;
      const transformed = new DOMPoint(x, y).matrixTransform(matrix);
      return { x: transformed.x, y: transformed.y };
    });
    await page.touchscreen.tap(point.x, point.y);
    await expect(editor(page)).toBeFocused();
    await page.keyboard.insertText('Touch cell e\u0301 👩🏽‍💻');
    await expect(cell(page, 1)).toHaveText('Touch cell e\u0301 👩🏽‍💻');
    await expect(cell(page, 0)).toHaveText('');
    await saveLocal(page);
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
    await page.screenshot({ path: '../../docs/images/graphe-tables-mobile.png', fullPage: true });
  });
});


test('column resize controls preserve total width, undo, redo and saved identity', async ({ page }) => {
  await start(page);
  await insertTable(page, 2, 2, unicode);
  await structuralAction(page, 'Widen column (12 pt)');
  const resized = nativeTable(await nativeFile(page));
  expect(resized.column_widths).toBeDefined();
  const widths = resized.column_widths!;
  expect(widths[0]! - widths[1]!).toBe(480);
  await structuralAction(page, 'Equal column widths');
  const equal = nativeTable(await nativeFile(page));
  expect(equal.column_widths!.reduce((sum, value) => sum + value, 0)).toBe(widths.reduce((sum, value) => sum + value, 0));
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  expect(nativeTable(await nativeFile(page))).toEqual(resized);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  expect(nativeTable(await nativeFile(page))).toEqual(equal);
  const title = 'Resized table persistence';
  await page.getByRole('textbox', { name: 'Document title', exact: true }).fill(title);
  await saveLocal(page);
  await page.reload();
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await documentAction(page, 'Open document');
  const library = page.getByRole('dialog', { name: 'Open document', exact: true });
  await library.getByRole('button', { name: new RegExp(title) }).click();
  await expect(library).toHaveCount(0);
  await readyPages(page);
  expect(nativeTable(await nativeFile(page))).toEqual(equal);
});
