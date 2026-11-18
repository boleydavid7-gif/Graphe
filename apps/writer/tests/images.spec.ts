import { readFile } from 'node:fs/promises';
import { expect, test, type Locator, type Page } from '@playwright/test';
import { documentAction, focusEditor, readyPages } from './step7-test-helpers';

// Real-browser workflows use the compiled Rust/WASM app and actual file pickers.
// Touch/viewport emulation below does not establish physical iOS keyboard behavior.
interface NativeImage {
  readonly id: string;
  readonly asset: { readonly format: 'png' | 'jpeg'; readonly bytes_base64: string };
  readonly width: number;
  readonly height: number;
  readonly alt_text: string;
  readonly alignment: 'left' | 'center' | 'right';
}
interface NativeParagraph { readonly id: string; readonly runs: readonly { readonly text: string }[] }
interface NativeFile {
  readonly document: {
    readonly id: string;
    readonly sections: readonly { readonly blocks: readonly (
      | { readonly kind: 'paragraph'; readonly content: NativeParagraph }
      | { readonly kind: 'image'; readonly content: NativeImage }
      | { readonly kind: 'table'; readonly content: unknown }
    )[] }[];
  };
  readonly source?: { readonly format: string; readonly bytes_base64: string };
}
type CopyFormat = 'writer' | 'docx' | 'odt' | 'pdf';
const fixtures = new URL('../../../tests/fixtures/images/', import.meta.url);
const editor = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const renderedImages = (page: Page) => page.getByTestId('paged-editor').locator('image[data-document-image-id]');
const unicode = 'Café e\u0301 👩🏽‍💻 العربية नमस्ते שלום';

async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
}

async function chooseImage(page: Page, bytes: Buffer, name = 'quadrants.png', mimeType = 'image/png') {
  await page.getByRole('button', { name: 'Insert', exact: true }).click();
  const pending = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Image…', exact: true }).click();
  await (await pending).setFiles({ name, mimeType, buffer: bytes });
}

async function insertPng(page: Page) {
  const bytes = await readFile(new URL('quadrants.png', fixtures));
  await chooseImage(page, bytes);
  await expect(renderedImages(page)).toHaveCount(1);
  await readyPages(page);
  const id = await renderedImages(page).getAttribute('data-document-image-id');
  if (!id) throw new Error('The displayed image has no stable Rust identity');
  await expect(page.getByTestId('paged-editor')).toHaveAttribute('data-selected-image-id', id);
  return { bytes, id };
}

async function copyFile(page: Page, format: CopyFormat): Promise<Buffer> {
  await documentAction(page, 'Save As…');
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption(format);
  const save = dialog.getByRole('button', { name: 'Save copy', exact: true });
  await expect(save).toBeEnabled();
  const pending = page.waitForEvent('download');
  await save.click();
  const download = await pending;
  expect(download.suggestedFilename()).toMatch(new RegExp(`\\.${format}$`, 'i'));
  const path = await download.path();
  if (!path) throw new Error(`No completed ${format} download was available`);
  await expect(dialog).toHaveCount(0);
  return readFile(path);
}

async function nativeFile(page: Page): Promise<NativeFile> {
  return JSON.parse((await copyFile(page, 'writer')).toString('utf8')) as NativeFile;
}

function images(file: NativeFile) {
  return file.document.sections.flatMap((section) => section.blocks.flatMap((block) =>
    block.kind === 'image' ? [block.content] : []));
}

function paragraphs(file: NativeFile) {
  return file.document.sections.flatMap((section) => section.blocks.flatMap((block) =>
    block.kind === 'paragraph' ? [block.content.runs.map((run) => run.text).join('')] : []));
}

async function properties(page: Page) {
  await page.getByRole('button', { name: 'Insert', exact: true }).click();
  await page.getByRole('button', { name: 'Image properties…', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Image properties', exact: true });
  await expect(dialog).toBeVisible();
  return dialog;
}

async function saveLocal(page: Page) {
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.save-state')).toHaveText('Saved on this device', { timeout: 10_000 });
}

async function presentationPixels(page: Page) {
  return renderedImages(page).evaluate(async (element) => {
    const src = element.getAttribute('href');
    if (!src) throw new Error('No Rust-generated image presentation URL');
    const image = new Image();
    image.src = src;
    await image.decode();
    const canvas = document.createElement('canvas');
    canvas.width = image.naturalWidth;
    canvas.height = image.naturalHeight;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('No test pixel reader');
    context.drawImage(image, 0, 0);
    return { width: image.naturalWidth, height: image.naturalHeight,
      pixel: [...context.getImageData(4, 4, 1, 1).data] };
  });
}

async function openFile(page: Page, format: 'writer' | 'docx' | 'odt', bytes: Buffer, requirePages = true) {
  await documentAction(page, 'Open document');
  const dialog = page.getByRole('dialog', { name: 'Open document', exact: true });
  const pending = page.waitForEvent('filechooser');
  await dialog.getByRole('button', { name: 'Open file…', exact: true }).click();
  await (await pending).setFiles({ name: `image-roundtrip.${format}`, buffer: bytes,
    mimeType: format === 'docx' ? 'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
      : format === 'odt' ? 'application/vnd.oasis.opendocument.text' : 'application/json' });
  await expect(dialog).toHaveCount(0);
  if (requirePages) {
    await expect(renderedImages(page)).toHaveCount(1);
    await readyPages(page);
  }
}

/** Set exact DOM UTF-16 endpoints; the application translates/validates in Rust. */
async function selectParagraphs(page: Page, anchor: Locator, offset: number,
  focus: Locator = anchor, endOffset = offset) {
  const anchorId = await anchor.getAttribute('data-paragraph-id');
  const focusId = await focus.getAttribute('data-paragraph-id');
  if (!anchorId || !focusId) throw new Error('Missing stable paragraph identity');
  await editor(page).evaluate((element, endpoints) => {
    const entries = [...element.querySelectorAll('p[data-paragraph-id]')];
    function point(id: string, remaining: number): { node: Node; offset: number } {
      const paragraph = entries.find((entry) => entry.getAttribute('data-paragraph-id') === id);
      if (!paragraph) throw new Error('The selected paragraph is absent');
      const walker = document.createTreeWalker(paragraph, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const length = node.textContent?.length ?? 0;
        if (remaining <= length) return { node, offset: remaining };
        remaining -= length;
      }
      if (remaining !== 0) throw new Error('Test endpoint exceeds paragraph UTF-16 length');
      return { node: paragraph, offset: paragraph.childNodes.length };
    }
    (element as HTMLElement).focus();
    const first = point(endpoints.anchorId, endpoints.offset);
    const last = point(endpoints.focusId, endpoints.endOffset);
    window.getSelection()?.setBaseAndExtent(first.node, first.offset, last.node, last.offset);
  }, { anchorId, focusId, offset, endOffset });
  await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
}

test('PNG insertion, image properties and keyboard deletion use atomic Rust history', async ({ page }) => {
  const failures: string[] = [];
  page.on('pageerror', (error) => failures.push(error.message));
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText(`Before ${unicode}`);
  const { bytes, id } = await insertPng(page);
  const original = images(await nativeFile(page))[0];
  expect(original?.id).toBe(id);
  expect(Buffer.from(original?.asset.bytes_base64 ?? '', 'base64')).toEqual(bytes);
  const dialog = await properties(page);
  await expect(dialog.getByRole('checkbox', { name: 'Lock aspect ratio', exact: true })).toBeChecked();
  await dialog.getByRole('spinbutton', { name: 'Width (pt)', exact: true }).fill('180');
  await dialog.getByRole('textbox', { name: 'Alt text', exact: true }).fill('Four quadrants — e\u0301 👩🏽‍💻');
  await dialog.getByRole('combobox', { name: 'Alignment', exact: true }).selectOption('center');
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  const updated = images(await nativeFile(page))[0];
  expect(updated).toEqual({ ...original, width: 3600, height: 1800,
    alt_text: 'Four quadrants — e\u0301 👩🏽‍💻', alignment: 'center' });
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  expect(images(await nativeFile(page))).toEqual([original]);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  expect(images(await nativeFile(page))).toEqual([updated]);
  await renderedImages(page).click();
  await page.keyboard.press('Delete');
  await expect(renderedImages(page)).toHaveCount(0);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(renderedImages(page)).toHaveCount(1);
  expect(images(await nativeFile(page))).toEqual([updated]);
  expect(paragraphs(await nativeFile(page))).toContain(`Before ${unicode}`);
  expect(failures).toEqual([]);
});

test('JPEG EXIF orientation is applied by Rust once, with canonical pixels rendered in the browser', async ({ page }) => {
  await start(page);
  const original = await readFile(new URL('phone-orientation-6.jpg', fixtures));
  await chooseImage(page, original, 'phone-orientation-6.jpg', 'image/jpeg');
  await expect(renderedImages(page)).toHaveCount(1);
  await readyPages(page);
  const dimensions = await presentationPixels(page);
  expect(dimensions.width).toBe(60);
  expect(dimensions.height).toBe(120);
  expect(dimensions.pixel[2]).toBeGreaterThan(160);
  expect(dimensions.pixel[0]).toBeLessThan(80);
  const saved = images(await nativeFile(page))[0];
  expect(saved?.asset.format).toBe('jpeg');
  expect(Buffer.from(saved?.asset.bytes_base64 ?? '', 'base64')).toEqual(original);
  expect(saved?.height).toBe((saved?.width ?? 0) * 2);
  const copy = await copyFile(page, 'docx');
  await openFile(page, 'docx', copy);
  await readyPages(page);
  expect(await presentationPixels(page)).toEqual(dimensions);
  const reopened = images(await nativeFile(page))[0];
  expect(reopened?.width).toBe(saved?.width);
  expect(reopened?.height).toBe(saved?.height);
});

test('text on both sides of an image survives typing and rejected cross-image replacement', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Before image');
  await insertPng(page);
  await page.getByRole('button', { name: 'Back to text', exact: true }).click();
  await expect(editor(page)).toBeFocused();
  await page.keyboard.insertText(`After ${unicode}`);
  const text = editor(page).locator('p[data-paragraph-id]');
  await expect(text.first()).toHaveText('Before image');
  await expect(text.last()).toHaveText(`After ${unicode}`);
  const before = await nativeFile(page);
  await selectParagraphs(page, text.first(), 3, text.last(), 4);
  await page.keyboard.insertText('Must not erase an image');
  await expect(page.getByRole('alert')).toContainText(/image|boundary|container|cross/i);
  await expect(renderedImages(page)).toHaveCount(1);
  expect((await nativeFile(page)).document).toEqual(before.document);
  await selectParagraphs(page, text.last(), 0);
  await page.keyboard.press('Backspace');
  await expect(renderedImages(page)).toHaveCount(1);
  expect((await nativeFile(page)).document).toEqual(before.document);
});

test('image selection prevents text clipboard cut and paste from modifying a stale paragraph selection', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText(`Preserve ${unicode}`);
  await insertPng(page);
  const before = await nativeFile(page);
  await renderedImages(page).click();
  for (const type of ['copy', 'cut', 'paste']) {
    const intercepted = await page.getByTestId('paged-editor').evaluate((element, eventType) => {
      const clipboardData = new DataTransfer();
      clipboardData.setData('text/plain', 'Must not replace previous text');
      const event = new ClipboardEvent(eventType, { bubbles: true, cancelable: true, clipboardData });
      element.dispatchEvent(event);
      return event.defaultPrevented;
    }, type);
    expect(intercepted).toBe(true);
    await expect(page.getByRole('alert')).toContainText(/image|text|clipboard/i);
    expect((await nativeFile(page)).document).toEqual(before.document);
    await renderedImages(page).click();
  }
});

test('local save, reload, native export and reopen retain original image bytes and stable identity', async ({ page }) => {
  await start(page);
  await insertPng(page);
  const title = 'Image native persistence regression';
  await page.getByRole('textbox', { name: 'Document title', exact: true }).fill(title);
  await saveLocal(page);
  const before = await nativeFile(page);
  await page.reload();
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await documentAction(page, 'Open document');
  const library = page.getByRole('dialog', { name: 'Open document', exact: true });
  await library.getByRole('button', { name: new RegExp(title) }).click();
  await expect(library).toHaveCount(0);
  await expect(renderedImages(page)).toHaveCount(1);
  await readyPages(page);
  expect(await nativeFile(page)).toEqual(before);
  const bytes = await copyFile(page, 'writer');
  await openFile(page, 'writer', bytes);
  expect(await nativeFile(page)).toEqual(before);
});

test('image pagination renders complete geometry after long mixed-Unicode text without overlapping page bounds', async ({ page }) => {
  test.setTimeout(90_000);
  await start(page);
  await focusEditor(page);
  const text = Array.from({ length: 40 }, (_, index) =>
    `Paragraph ${index + 1}: ${unicode}. A longer line checks wrapping before the next image.`).join('\n');
  await page.keyboard.insertText(text);
  await insertPng(page);
  const root = await readyPages(page);
  expect(await root.locator('svg[data-layout-page]').count()).toBeGreaterThan(1);
  const bounds = await renderedImages(page).evaluate((element) => {
    if (!(element instanceof SVGGraphicsElement)) throw new Error('The image has no SVG geometry');
    const svg = element.ownerSVGElement;
    if (!svg) throw new Error('The Rust image has no containing page');
    const box = element.getBBox();
    const page = svg.viewBox.baseVal;
    return { x: box.x, y: box.y, width: box.width, height: box.height,
      pageWidth: page.width, pageHeight: page.height };
  });
  expect(bounds.width).toBeGreaterThan(0);
  expect(bounds.height).toBeGreaterThan(0);
  expect(bounds.x).toBeGreaterThanOrEqual(0);
  expect(bounds.y).toBeGreaterThanOrEqual(0);
  expect(bounds.x + bounds.width).toBeLessThanOrEqual(bounds.pageWidth);
  expect(bounds.y + bounds.height).toBeLessThanOrEqual(bounds.pageHeight);
  const native = await nativeFile(page);
  expect(paragraphs(native).join('\n')).toContain(text);
  expect(images(native)).toHaveLength(1);
});

for (const format of ['docx', 'odt'] as const) {
  test(`${format.toUpperCase()} image copy reopens editable and preserves media, size and text order`, async ({ page }) => {
    await start(page);
    await focusEditor(page);
    await page.keyboard.insertText(`Before ${unicode}`);
    const { bytes } = await insertPng(page);
    const dialog = await properties(page);
    await dialog.getByRole('textbox', { name: 'Alt text', exact: true }).fill('A preserved picture');
    await dialog.getByRole('combobox', { name: 'Alignment', exact: true }).selectOption('right');
    await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
    await page.getByRole('button', { name: 'Back to text', exact: true }).click();
    await page.keyboard.insertText('After image');
    const baseline = images(await nativeFile(page))[0];
    const exported = await copyFile(page, format);
    await openFile(page, format, exported);
    const imported = await nativeFile(page);
    const current = images(imported)[0];
    expect(current).toEqual({ ...baseline, id: current?.id });
    expect(Buffer.from(current?.asset.bytes_base64 ?? '', 'base64')).toEqual(bytes);
    expect(paragraphs(imported)).toEqual([`Before ${unicode}`, 'After image']);
    expect(Buffer.from(imported.source?.bytes_base64 ?? '', 'base64')).toEqual(exported);
    await selectParagraphs(page, editor(page).locator('p[data-paragraph-id]').last(), 'After image'.length);
    await page.keyboard.insertText(' edited');
    await saveLocal(page);
    await openFile(page, format, await copyFile(page, format));
    expect(paragraphs(await nativeFile(page))).toContain('After image edited');
    expect(Buffer.from(images(await nativeFile(page))[0]?.asset.bytes_base64 ?? '', 'base64')).toEqual(bytes);
  });
}

test('PDF export contains image objects, and formats without image support disclose or block loss', async ({ page }) => {
  await start(page);
  await insertPng(page);
  const pdf = await copyFile(page, 'pdf');
  expect(pdf.subarray(0, 5).toString()).toBe('%PDF-');
  expect(pdf.toString('latin1')).toMatch(/\/Subtype\s*\/Image/);
  await documentAction(page, 'Save As…');
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption('txt');
  const save = dialog.getByRole('button', { name: 'Save copy', exact: true });
  if (await save.isEnabled()) await expect(dialog).toContainText(/image|picture/i);
  else await expect(dialog).toContainText(/image|picture|unsupported/i);
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(renderedImages(page)).toHaveCount(1);
});

test('SVG, corrupt and oversized chosen files fail without changing saved content or undo history', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText(`Keep this ${unicode}`);
  const before = await nativeFile(page);
  const attempts = [
    { name: 'active.svg', mimeType: 'image/svg+xml', bytes: Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"><script>throw 1</script></svg>') },
    { name: 'broken.png', mimeType: 'image/png', bytes: Buffer.from([137, 80, 78, 71, 13, 10, 26, 10, 1, 2, 3]) },
    { name: 'repeated-ifd.png', mimeType: 'image/png', bytes: await readFile(new URL('repeated-ifd.png', fixtures)) },
    { name: 'too-large.png', mimeType: 'image/png', bytes: Buffer.alloc(2 * 1024 * 1024 + 1) },
  ];
  for (const entry of attempts) {
    await chooseImage(page, entry.bytes, entry.name, entry.mimeType);
    await expect(page.getByRole('alert')).toContainText(/PNG|JPEG|image|MiB|large|format/i);
    await expect(renderedImages(page)).toHaveCount(0);
    expect((await nativeFile(page)).document).toEqual(before.document);
  }
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(editor(page)).not.toContainText('Keep this');
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  expect((await nativeFile(page)).document).toEqual(before.document);
});

test('an image file read that completes after a newer edit cannot insert into the changed revision', async ({ page }) => {
  await page.addInitScript(() => {
    const original = File.prototype.arrayBuffer;
    File.prototype.arrayBuffer = function arrayBuffer() {
      if (this.name !== 'delayed.png') return original.call(this);
      const readChosen = original.bind(this);
      return new Promise<ArrayBuffer>((resolve, reject) => {
        (window as Window & { releaseImageRead?: () => void }).releaseImageRead = () => {
          readChosen().then(resolve, reject);
        };
      });
    };
  });
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText('Current document survives stale file work');
  await chooseImage(page, await readFile(new URL('quadrants.png', fixtures)), 'delayed.png');
  await expect.poll(() => page.evaluate(() => typeof (window as Window & {
    releaseImageRead?: () => void;
  }).releaseImageRead)).toBe('function');
  await focusEditor(page);
  await page.keyboard.insertText(' — newer edit');
  await expect(editor(page)).toContainText('newer edit');
  await page.evaluate(() => (window as Window & { releaseImageRead?: () => void }).releaseImageRead?.());
  await expect(page.getByRole('alert')).toContainText(/stale|changed|earlier|revision/i);
  await expect(renderedImages(page)).toHaveCount(0);
  expect(paragraphs(await nativeFile(page))).toContain('Current document survives stale file work — newer edit');
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(editor(page)).toHaveText('Current document survives stale file work');
});

test('inserting an image with approximated color metadata discloses the adjustment immediately', async ({ page }) => {
  await start(page);
  const original = await readFile(new URL('color-approx.png', fixtures));
  await chooseImage(page, original, 'color-approx.png');
  await expect(renderedImages(page)).toHaveCount(1);
  await expect(page.locator('.status-message')).toContainText(/approximated colors|approximated.*precision/i);
  const dialog = await properties(page);
  await expect(dialog.getByRole('note')).toContainText(/approximates.*color|precision/i);
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  const persisted = images(await nativeFile(page))[0];
  expect(Buffer.from(persisted?.asset.bytes_base64 ?? '', 'base64')).toEqual(original);
});

test('an oversized native image can be selected and repaired from text fallback without losing content', async ({ page }) => {
  await start(page);
  await focusEditor(page);
  await page.keyboard.insertText(`Preserve text during recovery ${unicode}`);
  const { bytes } = await insertPng(page);
  const before = await nativeFile(page);
  // A native image may have structurally valid dimensions that do not fit its
  // page. This intentionally constructed file tests recovery, not JS editing.
  const oversized: NativeFile = { ...before, document: { ...before.document,
    sections: before.document.sections.map((section) => ({ ...section,
      blocks: section.blocks.map((block) => block.kind === 'image'
        ? { ...block, content: { ...block.content, width: 20_000, height: 20_000 } } : block),
    })),
  } };
  await openFile(page, 'writer', Buffer.from(JSON.stringify(oversized), 'utf8'), false);
  await page.getByRole('button', { name: 'Continue in text view', exact: true }).click();
  await expect(editor(page)).toContainText(`Preserve text during recovery ${unicode}`);
  await page.getByRole('button', { name: 'Select image: Document image', exact: true }).click();
  await expect(editor(page)).not.toBeFocused();
  const dialog = await properties(page);
  await dialog.getByRole('spinbutton', { name: 'Width (pt)', exact: true }).fill('180');
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await page.getByRole('button', { name: 'Try pages again', exact: true }).click();
  await readyPages(page);
  await expect(renderedImages(page)).toHaveCount(1);
  const repaired = await nativeFile(page);
  expect(paragraphs(repaired)).toEqual(paragraphs(before));
  expect(images(repaired)[0]?.id).toBe(images(before)[0]?.id);
  expect(images(repaired)[0]?.width).toBe(3600);
  expect(Buffer.from(images(repaired)[0]?.asset.bytes_base64 ?? '', 'base64')).toEqual(bytes);
});

test.describe('mobile image interaction', () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

  test('trusted touch selects an image without opening text input; Back to text restores editing', async ({ page }) => {
    await start(page);
    await insertPng(page);
    await page.getByRole('textbox', { name: 'Document title', exact: true }).focus();
    await renderedImages(page).tap();
    await expect(editor(page)).not.toBeFocused();
    const id = await renderedImages(page).getAttribute('data-document-image-id');
    await expect(page.getByTestId('paged-editor')).toHaveAttribute('data-selected-image-id', id ?? '');
    const dialog = await properties(page);
    await dialog.getByRole('textbox', { name: 'Alt text', exact: true }).fill('Mobile image');
    await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
    await page.getByRole('button', { name: 'Back to text', exact: true }).click();
    await expect(editor(page)).toBeFocused();
    await page.keyboard.insertText(`Mobile ${unicode}`);
    await expect(editor(page)).toContainText(`Mobile ${unicode}`);
    await saveLocal(page);
    const pdf = await copyFile(page, 'pdf');
    expect(pdf.subarray(0, 5).toString()).toBe('%PDF-');
    await expect(editor(page)).toContainText(`Mobile ${unicode}`);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  });
});


test('desktop resize handles and dragging use one Rust image edit each, retaining native pixels', async ({ page }) => {
  await start(page);
  const { bytes } = await insertPng(page);
  const baseline = images(await nativeFile(page))[0]!;
  const handle = page.locator('[data-image-resize-handle="se"]').first();
  await expect(handle).toBeVisible();
  const grip = await handle.boundingBox();
  if (!grip) throw new Error('Resize handle has no visible bounds');
  const x = grip.x + grip.width / 2;
  const y = grip.y + grip.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 65, y + 32, { steps: 8 });
  await page.mouse.up();
  await readyPages(page);
  const resized = images(await nativeFile(page))[0]!;
  expect(resized.width).toBeGreaterThan(baseline.width);
  expect(resized.height).toBeGreaterThan(baseline.height);
  expect(Math.abs(resized.width / resized.height - baseline.width / baseline.height)).toBeLessThan(0.01);
  expect(Buffer.from(resized.asset.bytes_base64, 'base64')).toEqual(bytes);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  expect(images(await nativeFile(page))).toEqual([baseline]);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  expect(images(await nativeFile(page))).toEqual([resized]);

  const rect = await renderedImages(page).boundingBox();
  const paper = await page.locator('svg[data-layout-page]').first().boundingBox();
  if (!rect || !paper) throw new Error('No page or image bounds for dragging');
  const startX = rect.x + rect.width / 2;
  const startY = rect.y + rect.height / 2;
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(startX + Math.max(150, (paper.width - rect.width) / 2), startY, { steps: 10 });
  await page.mouse.up();
  await readyPages(page);
  const moved = images(await nativeFile(page))[0]!;
  expect(moved.alignment).not.toBe(resized.alignment);
  expect(moved.id).toBe(resized.id);
  expect(moved.asset).toEqual(resized.asset);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  expect(images(await nativeFile(page))).toEqual([resized]);
});


test.describe('real browser touch image gestures', () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });
  test('two-finger pinch on an image resizes it through Rust without changing its original pixels', async ({ page, browserName }) => {
    test.skip(browserName !== 'chromium', 'Requires Chromium CDP multi-touch input');
    await start(page);
    const { bytes } = await insertPng(page);
    const before = images(await nativeFile(page))[0]!;
    const imageBox = await renderedImages(page).boundingBox();
    if (!imageBox) throw new Error('No image visible for multi-touch');
    const x = imageBox.x + imageBox.width / 2;
    const y = imageBox.y + imageBox.height / 2;
    const separation = Math.max(14, imageBox.width / 7);
    const client = await page.context().newCDPSession(page);
    const points = (delta: number) => [
      { x: x - separation - delta, y, id: 0 },
      { x: x + separation + delta, y, id: 1 },
    ];
    await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: points(0) });
    await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: points(12) });
    await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: points(22) });
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await readyPages(page);
    const after = images(await nativeFile(page))[0]!;
    expect(after.width).toBeGreaterThan(before.width);
    expect(after.height).toBeGreaterThan(before.height);
    expect(Buffer.from(after.asset.bytes_base64, 'base64')).toEqual(bytes);
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    expect(images(await nativeFile(page))).toEqual([before]);
  });
});
