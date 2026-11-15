import { expect, test, type Locator, type Page } from '@playwright/test';
import { focusEditor, readyPages } from './step7-test-helpers';

const input = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const save = (page: Page) => page.getByRole('button', { name: 'Save', exact: true });
const state = (page: Page) => page.locator('.save-state');

async function activate(control: Locator, touch: boolean) {
  if (touch) await control.tap();
  else await control.click();
}

async function begin(page: Page) {
  await page.goto('/');
  await expect(save(page)).toBeEnabled();
  await readyPages(page);
}

async function openLibrary(page: Page, touch: boolean) {
  // Focused mobile chrome intentionally hides the desktop Open shortcut. The
  // document menu remains the supported library route while editing.
  const menu = page.locator('details.document-menu');
  if (!(await menu.evaluate((element) => element.hasAttribute('open')))) {
    await activate(menu.locator('summary'), touch);
  }
  await activate(menu.getByRole('button', { name: 'Open document', exact: true }), touch);
}

async function stored(page: Page) {
  return page.evaluate(async () => {
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('writer-documents', 1);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    try {
      return await new Promise<{ id: string; etag: string; title: string; bytes: number[] }[]>((resolve, reject) => {
        const transaction = database.transaction('documents', 'readonly');
        const request = transaction.objectStore('documents').getAll();
        transaction.oncomplete = () => resolve((request.result as { id: string; etag: string; title: string; bytes: Uint8Array }[])
          .map((record) => ({ id: record.id, etag: record.etag, title: record.title, bytes: [...record.bytes] })));
        transaction.onabort = () => reject(transaction.error);
      });
    } finally { database.close(); }
  });
}

for (const touch of [false, true]) {
  test.describe(touch ? 'audit storage in touch viewport' : 'audit storage on desktop', () => {
    if (touch) test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

    test('a real IndexedDB abort after put success never reports Saved and preserves the committed file', async ({ page }) => {
      await begin(page);
      await page.getByRole('textbox', { name: 'Document title', exact: true }).fill('Commit boundary audit');
      await focusEditor(page);
      await page.keyboard.insertText('Saved e\u0301 👩🏽‍💻 العربية');
      await activate(save(page), touch);
      await expect(state(page)).toHaveText('Saved on this device');
      const original = await stored(page);
      expect(original).toHaveLength(1);

      await page.evaluate(() => {
        const observerState = window as Window & { auditSaveStates?: string[] };
        observerState.auditSaveStates = [];
        const status = document.querySelector('.save-state');
        if (!status) throw new Error('Expected actual save status');
        const observer = new MutationObserver(() => observerState.auditSaveStates?.push(status.textContent ?? ''));
        observer.observe(status, { subtree: true, childList: true, characterData: true });
        const put = IDBObjectStore.prototype.put;
        let abortNext = true;
        IDBObjectStore.prototype.put = function (value: unknown, key?: IDBValidKey) {
          const request = key === undefined ? put.call(this, value) : put.call(this, value, key);
          if (this.name === 'documents' && abortNext) {
            abortNext = false;
            request.addEventListener('success', () => this.transaction.abort());
          }
          return request;
        };
      });
      await focusEditor(page);
      await page.keyboard.press('Control+End');
      await page.keyboard.insertText(' plus unsaved work');
      await activate(save(page), touch);
      await expect(page.getByRole('alert')).toContainText('could not complete the storage operation');
      await expect(state(page)).toContainText('Unsaved');
      await expect(input(page)).toHaveText('Saved e\u0301 👩🏽‍💻 العربية plus unsaved work');
      expect(await stored(page)).toEqual(original);
      expect(await page.evaluate(() => (window as Window & { auditSaveStates?: string[] }).auditSaveStates))
        .not.toContain('Saved on this device');

      await activate(page.getByRole('button', { name: 'Retry save', exact: true }), touch);
      await expect(state(page)).toHaveText('Saved on this device');
      const recovered = await stored(page);
      expect(recovered[0]?.id).toBe(original[0]?.id);
      expect(recovered[0]?.etag).not.toBe(original[0]?.etag);
      expect(recovered[0]?.bytes).not.toEqual(original[0]?.bytes);
      await page.reload();
      await expect(save(page)).toBeEnabled();
      await openLibrary(page, touch);
      const dialog = page.getByRole('dialog', { name: 'Open document', exact: true });
      await activate(dialog.getByRole('button', { name: /Commit boundary audit/ }), touch);
      await expect(dialog).toHaveCount(0);
      await expect(input(page)).toHaveText('Saved e\u0301 👩🏽‍💻 العربية plus unsaved work');
      expect(await stored(page)).toEqual(recovered);
    });

    test('damaged native bytes and mismatched native identities cannot replace the current document', async ({ page }) => {
      await begin(page);
      await page.getByRole('textbox', { name: 'Document title', exact: true }).fill('Healthy current audit');
      await focusEditor(page);
      await page.keyboard.insertText('Current paragraph e\u0301 👩🏽‍💻');
      await activate(save(page), touch);
      await expect(state(page)).toHaveText('Saved on this device');
      const healthy = await stored(page);
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
              const source = request.result[0] as { bytes: Uint8Array };
              store.put({ id: 'audit-corrupt-native', title: 'Corrupt native audit', bytes: new TextEncoder().encode('{corrupt'),
                etag: 'corrupt-token', updatedAt: Date.now() });
              store.put({ id: 'audit-mismatched-native', title: 'Mismatched native audit', bytes: source.bytes,
                etag: 'mismatch-token', updatedAt: Date.now() });
            };
            transaction.oncomplete = () => resolve();
            transaction.onabort = () => reject(transaction.error);
          });
        } finally { database.close(); }
      });
      const before = await stored(page);
      await openLibrary(page, touch);
      const dialog = page.getByRole('dialog', { name: 'Open document', exact: true });
      for (const entry of [/Corrupt native audit/, /Mismatched native audit/]) {
        await activate(dialog.getByRole('button', { name: entry }), touch);
        await expect(dialog.getByRole('alert')).toBeVisible();
        await expect(input(page)).toHaveText('Current paragraph e\u0301 👩🏽‍💻');
        await expect(page.getByRole('textbox', { name: 'Document title', exact: true })).toHaveValue('Healthy current audit');
        expect(await stored(page)).toEqual(before);
      }
      await activate(dialog.getByRole('button', { name: 'Cancel', exact: true }), touch);
      await expect(state(page)).toHaveText('Saved on this device');
      await focusEditor(page);
      await page.keyboard.press('Control+End');
      await page.keyboard.insertText(' still editable');
      await activate(save(page), touch);
      await expect(state(page)).toHaveText('Saved on this device');
      const after = await stored(page);
      expect(after.filter((record) => record.id !== healthy[0]?.id))
        .toEqual(before.filter((record) => record.id !== healthy[0]?.id));
      expect(after.find((record) => record.id === healthy[0]?.id)?.etag).not.toBe(healthy[0]?.etag);
    });
  });
}
