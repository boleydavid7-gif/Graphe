import { expect, test, type Page } from '@playwright/test';
import { focusEditor, readyPages } from './step7-test-helpers';

const editor = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const title = (page: Page) => page.getByRole('textbox', { name: 'Document title', exact: true });

async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
}

async function save(page: Page) {
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
}

async function records(page: Page) {
  return page.evaluate(() => new Promise<{ id: string; etag: string; bytes: number[] }[]>((resolve, reject) => {
    const request = indexedDB.open('writer-documents', 1);
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const database = request.result;
      const transaction = database.transaction('documents', 'readonly');
      const result = transaction.objectStore('documents').getAll();
      let value: { id: string; etag: string; bytes: number[] }[] = [];
      result.onsuccess = () => {
        value = (result.result as { id: string; etag: string; bytes: Uint8Array }[])
          .map((record) => ({ id: record.id, etag: record.etag, bytes: Array.from(record.bytes) }));
      };
      transaction.oncomplete = () => { database.close(); resolve(value); };
      transaction.onabort = () => { database.close(); reject(transaction.error); };
      transaction.onerror = () => { database.close(); reject(transaction.error); };
    };
  }));
}

test('an edit followed by an old-SVG pointer event in the same task cannot move the new Rust selection', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Start here');
  const root = await readyPages(page);
  await expect(root.locator('[data-editor-caret]')).toBeVisible();
  const result = await root.evaluate((element) => {
    const input = document.querySelector('[data-testid="editor"]');
    const svg = element.querySelector('svg[data-layout-page]');
    const cursor = element.querySelector('[data-editor-caret]');
    if (!(input instanceof HTMLElement) || !(svg instanceof SVGSVGElement)
      || !(cursor instanceof SVGGraphicsElement)) throw new Error('Missing actual editor geometry');
    const cursorBounds = cursor.getBoundingClientRect();
    const revision = element.getAttribute('data-layout-revision');
    function selection() {
      const selected = window.getSelection();
      const focus = selected?.focusNode;
      if (!focus) throw new Error('The actual input has no selection');
      let paragraph: Node | null = focus;
      while (paragraph && paragraph.parentNode !== input) paragraph = paragraph.parentNode;
      if (!(paragraph instanceof HTMLParagraphElement)) throw new Error('No selected input paragraph');
      const range = document.createRange();
      range.setStart(paragraph, 0);
      range.setEnd(focus, selected.focusOffset);
      return { paragraphId: paragraph.dataset.paragraphId, utf16: range.cloneContents().textContent?.length ?? 0 };
    }
    input.dispatchEvent(new InputEvent('beforeinput', { bubbles: true, cancelable: true,
      inputType: 'insertText', data: ' — a new layout position 👩🏽‍💻' }));
    const afterEdit = selection();
    // The old SVG is deliberately used before React commits or a layout frame
    // can repaint. A stale hit must stop before even requesting pointer capture.
    svg.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true,
      button: 0, buttons: 1, pointerId: 71, pointerType: 'mouse',
      clientX: cursorBounds.left + cursorBounds.width / 2,
      clientY: cursorBounds.top + cursorBounds.height / 2 }));
    return { revision, afterEdit, afterStaleHit: selection(), text: input.textContent };
  });
  expect(result.afterEdit.utf16).toBe('Start here — a new layout position 👩🏽‍💻'.length);
  expect(result.afterStaleHit).toEqual(result.afterEdit);
  expect(result.text).toBe('Start here — a new layout position 👩🏽‍💻');
  await readyPages(page);
  await expect(root).not.toHaveAttribute('data-layout-revision', result.revision ?? '');
  await expect(editor(page)).toBeFocused();
});

test('a cancelled touch gesture never becomes a tap or steals focus from document controls', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Touch cancellation keeps the document unchanged.');
  const root = await readyPages(page);
  const svg = root.locator('svg[data-layout-page]').first();
  const bounds = await svg.boundingBox();
  if (!bounds) throw new Error('The actual page is not displayed');
  await title(page).focus();
  await svg.dispatchEvent('pointerdown', { button: 0, buttons: 1, pointerId: 72,
    pointerType: 'touch', clientX: bounds.x + bounds.width / 2, clientY: bounds.y + 110 });
  await svg.dispatchEvent('pointercancel', { button: 0, buttons: 0, pointerId: 72,
    pointerType: 'touch', clientX: bounds.x + bounds.width / 2, clientY: bounds.y + 110 });
  await expect(title(page)).toBeFocused();
  await expect(editor(page)).toHaveText('Touch cancellation keeps the document unchanged.');
  await page.waitForTimeout(100);
  await expect(title(page)).toBeFocused();
});

test('title and Find typing retain focus while Rust pages and viewport geometry refresh', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('alpha beta alpha');
  await readyPages(page);
  await title(page).focus();
  await page.keyboard.type('A dependable title', { delay: 45 });
  await expect(title(page)).toHaveValue('A dependable title');
  await expect(title(page)).toBeFocused();
  await readyPages(page);
  await expect(title(page)).toBeFocused();
  await page.getByRole('button', { name: 'Find', exact: true }).click();
  const query = page.getByRole('textbox', { name: 'Find text', exact: true });
  await expect(query).toBeFocused();
  await page.keyboard.type('al');
  await page.evaluate(() => window.dispatchEvent(new Event('resize')));
  await page.waitForTimeout(80);
  // Do not focus the input again: that would conceal a programmatic selection
  // restore stealing focus from this existing control.
  await expect(query).toBeFocused();
  await page.keyboard.type('pha', { delay: 45 });
  await expect(query).toHaveValue('alpha');
  await page.getByRole('button', { name: 'Next match', exact: true }).click();
  await query.focus();
  await page.evaluate(() => window.dispatchEvent(new Event('resize')));
  await page.waitForTimeout(80);
  await expect(query).toBeFocused();
  await page.keyboard.press('Control+a');
  await page.keyboard.type('beta');
  await expect(query).toHaveValue('beta');
  await expect(editor(page)).toHaveText('alpha beta alpha');
});

test('New accepts the first immediate keyboard input without waiting for a deferred focus callback', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Saved previous document');
  await save(page);
  await page.getByRole('button', { name: 'New', exact: true }).click();
  // This deliberately has no expect-to-be-focused or animation-frame delay.
  await page.keyboard.insertText('The first key belongs to the new document.');
  await expect(editor(page)).toHaveText('The first key belongs to the new document.');
  await expect(editor(page).locator('p')).toHaveCount(1);
  await readyPages(page);
});

test('a saved document with active IME work stays unsaved and cancellation preserves exact persisted bytes', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Preserve this exact source.');
  await save(page);
  const before = await records(page);
  expect(before).toHaveLength(1);
  await focusEditor(page);
  await editor(page).evaluate((element) => {
    element.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true, data: '' }));
    element.querySelector('p')?.appendChild(document.createTextNode('未確定'));
    element.dispatchEvent(new CompositionEvent('compositionupdate', { bubbles: true, data: '未確定' }));
    element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertCompositionText',
      data: '未確定', isComposing: true }));
  });
  await expect(page.locator('.save-state')).toHaveText('Composing · not saved');
  await expect(page.locator('.paged-editor-composition')).toHaveText('未確定');
  await page.waitForTimeout(1000);
  expect(await records(page)).toEqual(before);
  await editor(page).evaluate((element) => {
    element.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: '' }));
  });
  await expect(editor(page)).toHaveText('Preserve this exact source.');
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  expect(await records(page)).toEqual(before);
  await page.keyboard.press('Control+z');
  await expect(editor(page)).toHaveText('');
});
