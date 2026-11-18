import { test, expect, type Locator, type Page } from '@playwright/test';
import { documentAction, focusEditor, readyPages } from './step7-test-helpers';

const input = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const status = (page: Page) => page.locator('.save-state');
const save = (page: Page) => page.getByRole('button', { name: 'Save', exact: true });

async function ready(page: Page) {
  await page.goto('/');
  await expect(save(page)).toBeEnabled();
  await readyPages(page);
}

async function activate(control: Locator, touch: boolean) {
  if (touch) await control.tap();
  else await control.click();
}

async function reopen(page: Page, title: string, text: string, touch: boolean) {
  await page.reload();
  await expect(save(page)).toBeEnabled();
  if (touch) await documentAction(page, 'Open document'); else await activate(page.getByRole('button', { name: 'Open', exact: true }), touch);
  const dialog = page.getByRole('dialog', { name: 'Open document', exact: true });
  await activate(dialog.getByRole('button', { name: new RegExp(title) }), touch);
  await expect(dialog).toHaveCount(0);
  await expect(input(page)).toHaveText(text);
  await expect(status(page)).toHaveText('Saved on this device');
}

for (const touch of [false, true]) {
  test.describe(touch ? 'touch viewport local saving' : 'desktop local saving', () => {
    if (touch) test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

    test('a transient library-open failure can be retried without reloading away unsaved text', async ({ page }) => {
      await page.addInitScript(() => {
        const open = IDBFactory.prototype.open;
        IDBFactory.prototype.open = function (name: string, version?: number) {
          if (name === 'writer-documents' && !sessionStorage.getItem('writer-test-startup-failed')) {
            sessionStorage.setItem('writer-test-startup-failed', 'yes');
            throw new DOMException('Transient database startup failure', 'UnknownError');
          }
          return version === undefined ? open.call(this, name) : open.call(this, name, version);
        };
      });
      await ready(page);
      if (touch) await documentAction(page, 'Open document'); else await activate(page.getByRole('button', { name: 'Open', exact: true }), touch);
      await expect(page.getByRole('alert')).toContainText('could not complete the storage operation');
      await page.getByRole('textbox', { name: 'Document title', exact: true }).fill('Recovered startup');
      await focusEditor(page);
      const text = 'Unsaved Café e\u0301 👩🏽‍💻 survives retry.';
      await page.keyboard.insertText(text);
      await activate(save(page), touch);
      await expect(status(page)).toHaveText('Saved on this device');
      await expect(input(page)).toHaveText(text);
      await reopen(page, 'Recovered startup', text, touch);
    });

    test('an unexpected database close reconnects on Save and keeps the current editing session', async ({ page }) => {
      await page.addInitScript(() => {
        const open = IDBFactory.prototype.open;
        IDBFactory.prototype.open = function (name: string, version?: number) {
          const request = version === undefined ? open.call(this, name) : open.call(this, name, version);
          if (name === 'writer-documents') request.addEventListener('success', () => {
            (window as Window & { writerTestConnection?: IDBDatabase }).writerTestConnection = request.result;
          });
          return request;
        };
      });
      await ready(page);
      await page.getByRole('textbox', { name: 'Document title', exact: true }).fill('Reconnected library');
      await focusEditor(page);
      await page.keyboard.insertText('First committed revision');
      await activate(save(page), touch);
      await expect(status(page)).toHaveText('Saved on this device');
      await page.evaluate(() => {
        const database = (window as Window & { writerTestConnection?: IDBDatabase }).writerTestConnection;
        if (!database) throw new Error('Expected a real IndexedDB connection');
        database.close();
        // close() itself does not emit this event. Deliver the browser's
        // abnormal-close signal explicitly so the lifecycle recovery is testable.
        database.dispatchEvent(new Event('close'));
      });
      await focusEditor(page);
      await page.keyboard.press('Control+End');
      await page.keyboard.insertText(' plus current work');
      await activate(save(page), touch);
      await expect(status(page)).toHaveText('Saved on this device');
      const text = 'First committed revision plus current work';
      await expect(input(page)).toHaveText(text);
      await reopen(page, 'Reconnected library', text, touch);
    });
  });
}

test('Save of a new blank document is not swallowed by the final in-flight autosave of its predecessor', async ({ page }) => {
  await ready(page);
  await page.evaluate(() => {
    const state = window as Window & { writerSaveHeld?: number; writerSaveRelease?: number };
    state.writerSaveHeld = 0;
    state.writerSaveRelease = 0;
    const put = IDBObjectStore.prototype.put;
    let sequence = 0;
    IDBObjectStore.prototype.put = function (value: unknown, key?: IDBValidKey) {
      const request = key === undefined ? put.call(this, value) : put.call(this, value, key);
      const record = value as { id?: unknown; bytes?: unknown };
      if (typeof record.id !== 'string' || !(record.bytes instanceof Uint8Array)) return request;
      const attempt = ++sequence;
      if (attempt > 2) return request;
      request.addEventListener('success', () => {
        const keepAlive = () => {
          const pending = this.get(record.id as string);
          pending.onsuccess = () => {
            state.writerSaveHeld = attempt;
            if ((state.writerSaveRelease ?? 0) < attempt) keepAlive();
          };
        };
        keepAlive();
      });
      return request;
    };
  });
  await page.getByRole('textbox', { name: 'Document title', exact: true }).fill('Older autosave');
  await focusEditor(page);
  await page.keyboard.insertText('First');
  await expect.poll(() => page.evaluate(() => (window as Window & { writerSaveHeld?: number }).writerSaveHeld)).toBe(1);
  await page.keyboard.insertText(' plus later typing');
  // Let the actual foreground debounce request its bounded follow-up while
  // the first real IndexedDB transaction is still kept alive.
  await page.waitForTimeout(1000);
  await page.evaluate(() => { (window as Window & { writerSaveRelease?: number }).writerSaveRelease = 1; });
  await expect.poll(() => page.evaluate(() => (window as Window & { writerSaveHeld?: number }).writerSaveHeld)).toBe(2);
  try {
    await page.getByRole('button', { name: 'New', exact: true }).click();
    await page.getByRole('dialog', { name: 'Save your changes?', exact: true })
      .getByRole('button', { name: 'Discard changes', exact: true }).click();
    await expect(input(page)).toHaveText('');
    await expect(status(page)).toHaveText('Not saved yet');
    await save(page).click();
    await expect(save(page)).toBeDisabled();
  } finally {
    await page.evaluate(() => { (window as Window & { writerSaveRelease?: number }).writerSaveRelease = 2; });
  }
  await expect(status(page)).toHaveText('Saved on this device');
  await reopen(page, 'Untitled document', '', false);
});
