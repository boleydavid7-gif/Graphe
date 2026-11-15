import { expect, test, type Locator, type Page } from '@playwright/test';
import { focusEditor, readyPages } from './step7-test-helpers';

const content = 'Keep the current Unicode document e\u0301 👩🏽‍💻 العربية.';
const input = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const save = (page: Page) => page.getByRole('button', { name: 'Save', exact: true });
const dialog = (page: Page) => page.getByRole('dialog', { name: 'Open document', exact: true });

async function activate(control: Locator, touch: boolean) {
  if (touch) await control.tap(); else await control.click();
}

async function openLibrary(page: Page, touch: boolean) {
  const menu = page.locator('details.document-menu');
  if (!await menu.evaluate((element) => element.hasAttribute('open'))) await activate(menu.locator('summary'), touch);
  await activate(menu.getByRole('button', { name: 'Open document', exact: true }), touch);
}

async function begin(page: Page, touch: boolean) {
  await page.goto('/');
  await expect(save(page)).toBeEnabled();
  await readyPages(page);
  await page.getByRole('textbox', { name: 'Document title', exact: true }).fill('Current library document');
  await focusEditor(page);
  await page.keyboard.insertText(content);
  await activate(save(page), touch);
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
}

async function records(page: Page) {
  return page.evaluate(async () => {
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('writer-documents', 1);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    try {
      return await new Promise<{ id: string; title: string; etag: string; updatedAt: number; bytes: number[] | string }[]>((resolve, reject) => {
        const transaction = database.transaction('documents', 'readonly');
        const request = transaction.objectStore('documents').getAll();
        transaction.oncomplete = () => resolve((request.result as { id: string; title: string; etag: string; updatedAt: number; bytes: Uint8Array | string }[])
          .map((record) => ({ id: record.id, title: record.title, etag: record.etag, updatedAt: record.updatedAt,
            bytes: record.bytes instanceof Uint8Array ? [...record.bytes] : record.bytes })));
        transaction.onabort = () => reject(transaction.error);
      });
    } finally { database.close(); }
  });
}

async function seedLargeBodiesAndDamage(page: Page) {
  await page.evaluate(async () => {
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('writer-documents', 1);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    try {
      await new Promise<void>((resolve, reject) => {
        const transaction = database.transaction('documents', 'readwrite');
        const store = transaction.objectStore('documents');
        const request = store.getAll();
        request.onsuccess = () => {
          const current = request.result[0] as { bytes: Uint8Array };
          // Test fixtures start from a real engine-produced native file. Only
          // independent fixture identity/title and harmless JSON whitespace
          // differ; no JavaScript editing or application model is introduced.
          const template = JSON.parse(new TextDecoder().decode(current.bytes)) as {
            document: { id: string; metadata: { title: string } };
          };
          for (const [id, title, time] of [
            ['library-z-tied', 'Large Z', 20], ['library-a-tied', 'Large A', 20], ['library-older', 'Older large body', 10],
          ] as const) {
            const fixture = structuredClone(template);
            fixture.document.id = id;
            fixture.document.metadata.title = title;
            const encoded = new TextEncoder().encode(JSON.stringify(fixture));
            const bytes = new Uint8Array(256 * 1024).fill(32);
            bytes.set(encoded);
            store.put({ id, title, bytes, etag: `${id}-token`, updatedAt: time });
          }
          store.put({ id: 'library-corrupt', title: 'Damaged library envelope', bytes: 'not byte data', etag: 'damaged-token', updatedAt: 30 });
        };
        transaction.oncomplete = () => resolve();
        transaction.onabort = () => reject(transaction.error);
      });
    } finally { database.close(); }
  });
}

for (const touch of [false, true]) {
  test.describe(touch ? 'Phase 2 library in touch viewport' : 'Phase 2 library on desktop', () => {
    if (touch) test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

    test('metadata listing retains large bodies, etags, deterministic order and healthy access beside damage', async ({ page }) => {
      test.setTimeout(60_000);
      const errors: string[] = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await begin(page, touch);
      await seedLargeBodiesAndDamage(page);
      const before = await records(page);
      await openLibrary(page, touch);
      await expect(dialog(page)).toBeVisible();
      await expect(dialog(page).locator('.library-entry > span')).toHaveText([
        'Current library document', 'Large A', 'Large Z', 'Older large body',
      ]);
      await expect(dialog(page).getByRole('alert')).toContainText('could not be listed');
      await expect(input(page)).toHaveText(content);
      expect(await records(page)).toEqual(before);

      await activate(dialog(page).getByRole('button', { name: /Large A/ }), touch);
      await expect(dialog(page)).toHaveCount(0);
      await readyPages(page);
      await expect(input(page)).toHaveText(content);
      await expect(page.getByRole('textbox', { name: 'Document title', exact: true })).toHaveValue('Large A');
      expect(await records(page)).toEqual(before);
      await focusEditor(page);
      await page.keyboard.press('Control+End');
      await page.keyboard.insertText(' Still editable after listing.');
      await activate(save(page), touch);
      await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      const after = await records(page);
      expect(after.find((record) => record.id === 'library-a-tied')?.etag)
        .not.toBe(before.find((record) => record.id === 'library-a-tied')?.etag);
      expect(after.filter((record) => record.id !== 'library-a-tied'))
        .toEqual(before.filter((record) => record.id !== 'library-a-tied'));
      expect(errors).toEqual([]);
    });

    test('abort after the final real cursor result publishes no partial library and retry preserves current work', async ({ page }) => {
      test.setTimeout(60_000);
      const errors: string[] = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await begin(page, touch);
      await seedLargeBodiesAndDamage(page);
      const before = await records(page);
      await page.evaluate(() => {
        const open = IDBObjectStore.prototype.openCursor;
        IDBObjectStore.prototype.openCursor = function (query?: IDBValidKey | IDBKeyRange | null, direction?: IDBCursorDirection) {
          const request = open.call(this, query, direction);
          if (this.name === 'documents' && this.transaction.mode === 'readonly') {
            let atEnd = false;
            request.addEventListener('success', () => {
              // continue() changes this request back to pending. Read result
              // before Writer advances it; the later listener uses this flag.
              atEnd = request.result === null;
            });
            // Attach after Writer's onsuccess handler. End-of-cursor is still
            // request success, not a committed read transaction.
            queueMicrotask(() => request.addEventListener('success', () => {
              if (atEnd) {
                IDBObjectStore.prototype.openCursor = open;
                this.transaction.abort();
              }
            }));
          }
          return request;
        };
      });
      await openLibrary(page, touch);
      await expect(page.getByRole('alert')).toContainText('could not complete the storage operation');
      await expect(dialog(page)).toHaveCount(0);
      await expect(input(page)).toHaveText(content);
      await expect(page.getByRole('textbox', { name: 'Document title', exact: true })).toHaveValue('Current library document');
      await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      expect(await records(page)).toEqual(before);

      await openLibrary(page, touch);
      await expect(dialog(page).locator('.library-entry')).toHaveCount(4);
      await activate(dialog(page).getByRole('button', { name: 'Cancel', exact: true }), touch);
      await focusEditor(page);
      await page.keyboard.press('Control+End');
      await page.keyboard.insertText(' Retained after retry.');
      await activate(save(page), touch);
      await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      const after = await records(page);
      expect(after.filter((record) => record.id.startsWith('library-')))
        .toEqual(before.filter((record) => record.id.startsWith('library-')));
      expect(errors).toEqual([]);
    });
  });
}
