import { expect, test, type Frame, type Page } from '@playwright/test';

// The trusted host is fulfilled locally. Its iframe loads the actual production
// Writer preview, engine and IndexedDB; this is not a live ProairetOS/device test.
const host = 'https://proairetos.com/__writer_merge_regression';
// Both HTTPS origins are intercepted locally; no live requests are sent.
const writer = 'https://writer.proairetos.com';
const tokens = {
  wall: '#211d19', page: '#302b25', card: '#383129', inset: '#443a30',
  raised: '#504333', mica: '#352d26', ink: '#f3eee5', ink2: '#c9bfae',
  lamp: '#edc669', onLamp: '#282117',
};
const text = 'Embedded Café e\u0301 👩🏽‍💻';
const content = (frame: Frame) => frame.getByRole('textbox', { name: 'Document content', exact: true });
interface HostFilesWindow extends Window {
  writerFilesReady?: unknown[];
  writerFileDialogs?: { requestId: string; name: string; format: string }[];
  writerFileSaves?: { origin: string; requestId: string; name: string; mime: string; bytes: number[] }[];
}

async function openHost(page: Page): Promise<Frame> {
  // Serve the unchanged production preview under an intercepted HTTPS origin.
  // HTTPS host -> HTTP loopback iframes are blocked by modern local-network
  // protections; keep those protections and model actual HTTPS embedding.
  await page.route(`${writer}/**`, async (route) => {
    const url = new URL(route.request().url());
    const response = await route.fetch({ url: `http://127.0.0.1:4173${url.pathname}${url.search}` });
    await route.fulfill({ response });
  });
  await page.route(host, (route) => route.fulfill({ contentType: 'text/html', body: `<!doctype html>
    <html><head><meta name="viewport" content="width=device-width, initial-scale=1">
    <style>html,body{margin:0;height:100%;overflow:hidden}iframe{display:block;border:0;width:100%;height:100%}
    dialog{box-sizing:border-box;max-width:calc(100vw - 24px);width:320px}dialog label{display:block;margin:12px 0}
    dialog input,dialog select{box-sizing:border-box;width:100%}dialog button{min-height:44px}</style></head>
    <body><iframe title="Writer integration test" referrerpolicy="origin" src="${writer}/"></iframe>
    <dialog id="files-dialog" aria-labelledby="files-title"><h2 id="files-title">ProairetOS Save copy</h2>
      <label>Filename<input id="files-name"></label><label>File format<select id="files-format">
        <option value="writer">Native Writer document (.writer)</option></select></label>
      <button id="files-save" type="button">Save copy</button></dialog>
    <script>
    window.writerReady=[];window.writerFilesReady=[];window.writerFileDialogs=[];window.writerFileSaves=[];
    let activeCopy=null;
    const child=document.querySelector('iframe');const dialog=document.getElementById('files-dialog');
    addEventListener('message',event=>{
      if(event.source!==child.contentWindow||event.origin!==${JSON.stringify(writer)})return;
      const data=event.data;
      if(data?.type==='writer:lamplight-ready')window.writerReady.push({origin:event.origin,data});
      if(data?.version!==1)return;
      if(data.type==='writer:files-ready')window.writerFilesReady.push(data);
      if(data.type==='writer:files-dialog-open'){
        activeCopy=data;window.writerFileDialogs.push({requestId:data.requestId,name:data.name,format:data.format});
        document.getElementById('files-name').value=data.name;
        document.getElementById('files-format').value=data.format;dialog.showModal();
      }
      if(data.type==='writer:files-save-request'&&activeCopy&&data.requestId===activeCopy.requestId&&data.data instanceof ArrayBuffer){
        window.writerFileSaves.push({origin:event.origin,requestId:data.requestId,name:data.name,mime:data.mime,
          bytes:Array.from(new Uint8Array(data.data))});
        child.contentWindow.postMessage({type:'proairetos:files-save-result',version:1,requestId:data.requestId,
          ok:true,fileId:'embedded_native_copy'},${JSON.stringify(writer)});
        activeCopy=null;dialog.close();
      }
    });
    document.getElementById('files-save').addEventListener('click',()=>{
      if(!activeCopy)return;
      child.contentWindow.postMessage({type:'proairetos:files-export-request',version:1,requestId:activeCopy.requestId,
        name:document.getElementById('files-name').value,format:document.getElementById('files-format').value},${JSON.stringify(writer)});
    });
    </script></body></html>` }));
  await page.goto(host);
  await expect.poll(() => page.frames().filter((frame) => frame.url() === `${writer}/`).length).toBe(1);
  const frame = page.frames().find((candidate) => candidate.url() === `${writer}/`)!;
  await expect(frame.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await expect(frame.getByTestId('paged-editor')).toHaveAttribute('aria-busy', 'false');
  await expect(frame.locator('svg[data-layout-page]').first()).toBeVisible();
  return frame;
}

async function theme(page: Page, mode = 'dark', palette: Record<string, string> = tokens) {
  await page.evaluate(({ mode, palette, target }) => {
    document.querySelector('iframe')!.contentWindow!.postMessage({
      type: 'proairetos:lamplight', version: 1, app: 'writer', mode, tokens: palette,
    }, target);
  }, { mode, palette, target: writer });
}

async function menuAction(frame: Frame, name: string) {
  const menu = frame.locator('details.document-menu');
  if (!await menu.evaluate((element) => element.hasAttribute('open'))) await menu.locator('summary').tap();
  await menu.getByRole('button', { name, exact: true }).tap();
}

/** Read committed bytes without interpreting or changing the Rust document. */
async function nativeRecords(frame: Frame) {
  return frame.evaluate(async () => {
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('writer-documents', 1);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    try {
      return await new Promise<{ id: string; etag: string; bytes: number[] }[]>((resolve, reject) => {
        const transaction = database.transaction('documents', 'readonly');
        const request = transaction.objectStore('documents').getAll();
        transaction.oncomplete = () => resolve((request.result as { id: string; etag: string; bytes: Uint8Array }[])
          .map((record) => ({ id: record.id, etag: record.etag, bytes: [...record.bytes] })));
        transaction.onabort = () => reject(transaction.error);
      });
    } finally { database.close(); }
  });
}

test('standalone Writer keeps its identity and fallback palette', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await expect(page.locator('.top-bar .identity')).toBeVisible();
  expect(await page.locator('html').getAttribute('data-writer-appearance')).toBeNull();
});

test('trusted parent theme updates chrome without changing paper or Rust page geometry', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  const frame = await openHost(page);
  await expect.poll(() => page.evaluate(() => (window as Window & { writerReady?: unknown[] }).writerReady?.length)).toBe(1);
  const paper = frame.locator('.paged-editor-paper').first();
  const before = await paper.evaluate((element) => ({ background: getComputedStyle(element).backgroundColor,
    width: element.querySelector('svg')!.getAttribute('data-layout-width'), height: element.querySelector('svg')!.getAttribute('data-layout-height') }));
  // Synthetic negative events exercise both guards; the positive message below
  // crosses the actual parent/iframe boundary with the browser's real origin.
  await frame.evaluate((palette) => {
    const data = { type: 'proairetos:lamplight', version: 1, app: 'writer', mode: 'dark', tokens: palette };
    window.dispatchEvent(new MessageEvent('message', { source: window.parent, origin: 'https://untrusted.invalid', data }));
    window.dispatchEvent(new MessageEvent('message', { source: window, origin: 'https://proairetos.com', data }));
  }, tokens);
  expect(await frame.locator('html').getAttribute('data-writer-appearance')).toBeNull();
  await theme(page, 'dark', { ...tokens, lamp: 'invalid' });
  expect(await frame.locator('html').getAttribute('data-writer-appearance')).toBeNull();
  await theme(page);
  await expect(frame.locator('html')).toHaveAttribute('data-writer-appearance', 'dark');
  await expect(frame.locator('.top-bar .identity')).toBeHidden();
  await expect(frame.locator('.top-bar')).toHaveCSS('background-color', 'rgb(56, 49, 41)');
  await theme(page, 'light', { ...tokens, card: '#eee8dc' });
  await expect(frame.locator('html')).toHaveAttribute('data-writer-appearance', 'light');
  await expect(frame.locator('.top-bar')).toHaveCSS('background-color', 'rgb(238, 232, 220)');
  expect(await paper.evaluate((element) => ({ background: getComputedStyle(element).backgroundColor,
    width: element.querySelector('svg')!.getAttribute('data-layout-width'), height: element.querySelector('svg')!.getAttribute('data-layout-height') }))).toEqual(before);
  expect(errors).toEqual([]);
});

test.describe('trusted embedded mobile Writer', () => {
  test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

  test('trusted paper taps, focus chrome, saved reopening and native Save copy remain functional', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    let frame = await openHost(page);
    await theme(page);
    await expect(frame.locator('html')).toHaveAttribute('data-writer-appearance', 'dark');
    await expect.poll(() => page.evaluate(() => (window as HostFilesWindow).writerFilesReady?.length ?? 0)).toBeGreaterThan(0);
    const title = frame.getByRole('textbox', { name: 'Document title', exact: true });
    await expect(title).toHaveCSS('font-size', '16px');
    await title.fill('Embedded merge regression');
    await frame.locator('svg[data-layout-page]').first().tap({ position: { x: 90, y: 90 } });
    await expect(content(frame)).toBeFocused();
    await expect(frame.locator('.status-bar')).toBeVisible();
    await expect(frame.locator('.mobile-document-actions')).toHaveCount(0);
    await page.keyboard.insertText(text);
    await expect(content(frame)).toHaveText(text);
    await expect(frame.locator('.mobile-word-count')).toHaveCount(0);
    await expect(frame.locator('.status-bar .word-count')).toBeVisible();
    await expect.poll(async () => Number((await frame.locator('.status-bar .word-count').textContent())?.split(' ')[0])).toBeGreaterThan(1);
    await expect(frame.getByTestId('paged-editor')).toHaveAttribute('aria-busy', 'false');
    await title.tap();
    await expect(frame.locator('.status-bar')).toBeVisible();
    await expect(frame.locator('.mobile-document-actions')).toHaveCount(0);
    await expect(frame.locator('.document-menu > summary')).toBeVisible();
    await frame.getByRole('button', { name: 'Save', exact: true }).tap();
    await expect.poll(async () => ({
      status: await frame.locator('.mobile-save-state').textContent(),
      alerts: await frame.getByRole('alert').allTextContents(),
    })).toEqual({ status: 'Saved on this device', alerts: [] });
    const saved = await nativeRecords(frame);
    expect(saved).toHaveLength(1);
    await menuAction(frame, 'Save As…');
    // Connected Save As deliberately belongs to the host Files UI. This local
    // host stub uses the real postMessage protocol and waits for Rust bytes.
    const dialog = page.getByRole('dialog', { name: 'ProairetOS Save copy', exact: true });
    await expect(dialog).toBeVisible();
    await expect(frame.getByRole('dialog', { name: 'Save As', exact: true })).toHaveCount(0);
    await expect(dialog.getByRole('combobox', { name: 'File format', exact: true })).toHaveValue('writer');
    await dialog.getByRole('textbox', { name: 'Filename', exact: true }).fill('Embedded copy.writer');
    const requested = await page.evaluate(() => (window as HostFilesWindow).writerFileDialogs?.at(-1));
    expect(requested?.requestId).toMatch(/^[a-zA-Z0-9_-]{8,80}$/);
    if (!requested) throw new Error('The embedding host did not receive a save-dialog request');
    // Wrong-origin and wrong-source messages must not export any document.
    await frame.evaluate((requestId) => {
      const data = { type: 'proairetos:files-export-request', version: 1, requestId,
        name: 'Rejected copy.writer', format: 'writer' };
      window.dispatchEvent(new MessageEvent('message', { source: window.parent, origin: 'https://untrusted.invalid', data }));
      window.dispatchEvent(new MessageEvent('message', { source: window, origin: 'https://proairetos.com', data }));
    }, requested.requestId);
    expect(await page.evaluate(() => (window as HostFilesWindow).writerFileSaves)).toEqual([]);
    const copy = dialog.getByRole('button', { name: 'Save copy', exact: true });
    await expect(copy).toBeInViewport();
    await copy.tap();
    await expect.poll(() => page.evaluate(() => (window as HostFilesWindow).writerFileSaves?.length)).toBe(1);
    const received = await page.evaluate(() => (window as HostFilesWindow).writerFileSaves?.[0]);
    expect(received).toMatchObject({ origin: writer, requestId: requested.requestId,
      name: 'Embedded copy.writer', mime: 'application/json', bytes: saved[0]?.bytes });
    const native = JSON.parse(Buffer.from(received?.bytes ?? []).toString('utf8')) as {
      document: { id: string; metadata: { title: string }; sections: { blocks: { content: { runs: { text: string }[] } }[] }[] };
    };
    expect(native.document.id).toBe(saved[0]?.id);
    expect(native.document.metadata.title).toBe('Embedded merge regression');
    expect(native.document.sections.flatMap((section) => section.blocks)
      .flatMap((block) => block.content.runs).map((run) => run.text).join('')).toBe(text);
    await expect(dialog).not.toBeVisible();
    await expect(frame.locator('.status-message')).toHaveText('Saved in ProairetOS Files.');
    await expect(content(frame)).toHaveText(text);
    expect(await nativeRecords(frame)).toEqual(saved);
    await page.reload();
    frame = page.frames().find((candidate) => candidate.url() === `${writer}/`)!;
    await expect(frame.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
    await theme(page);
    await menuAction(frame, 'Open document');
    const library = frame.getByRole('dialog', { name: 'Open document', exact: true });
    await library.getByRole('button', { name: /Embedded merge regression/ }).tap();
    await expect(library).toHaveCount(0);
    await expect(content(frame)).toHaveText(text);
    expect(await nativeRecords(frame)).toEqual(saved);
    expect(errors).toEqual([]);
  });

  test('long-press selection and handle hover keep Rust endpoints and transparent touch targets', async ({ page, browserName }) => {
    test.skip(browserName !== 'chromium', 'Trusted multi-event touch injection uses Chromium CDP.');
    const frame = await openHost(page);
    await theme(page);
    await frame.locator('svg[data-layout-page]').first().tap({ position: { x: 90, y: 90 } });
    await page.keyboard.insertText(text);
    await expect(frame.getByTestId('paged-editor')).toHaveAttribute('aria-busy', 'false');
    const glyph = await frame.locator('svg[data-layout-page] g[data-layout-paragraph-id] use').nth(2).boundingBox();
    if (!glyph) throw new Error('The Rust glyph is not displayed');
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [
      { x: glyph.x + glyph.width / 2, y: glyph.y + glyph.height / 2, radiusX: 2, radiusY: 2 },
    ] });
    await page.waitForTimeout(650);
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    const anchor = frame.getByTestId('touch-selection-handle-anchor');
    const focus = frame.getByTestId('touch-selection-handle-focus');
    await expect(anchor).toBeVisible();
    await expect(focus).toBeVisible();
    const endpoints = [await anchor.getAttribute('data-utf16'), await focus.getAttribute('data-utf16')];
    expect(endpoints).toEqual(['0', '8']);
    await anchor.hover();
    await expect(anchor).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
    expect([await anchor.getAttribute('data-utf16'), await focus.getAttribute('data-utf16')]).toEqual(endpoints);
    await frame.getByRole('button', { name: 'Bold', exact: true }).tap();
    await expect(frame.getByRole('button', { name: 'Bold', exact: true })).toHaveAttribute('aria-pressed', 'true');
    await expect(content(frame)).toHaveText(text);
    expect([await anchor.getAttribute('data-utf16'), await focus.getAttribute('data-utf16')]).toEqual(endpoints);
    await frame.getByRole('button', { name: 'Undo', exact: true }).tap();
    await expect(frame.getByRole('button', { name: 'Bold', exact: true })).toHaveAttribute('aria-pressed', 'false');
    await expect(content(frame)).toHaveText(text);
    await cdp.detach();
  });
});
