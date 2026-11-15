import { readFile } from 'node:fs/promises';
import { expect, test, type Download, type Locator, type Page } from '@playwright/test';
import { focusEditor, readyPages } from './step7-test-helpers';

type Format = 'docx' | 'odt' | 'rtf';
type Properties = Record<string, unknown>;
interface Run { text: string; format: Properties }
interface Paragraph { style: string | null; runs: Run[]; format: Properties }
interface Style { id: string; based_on: string | null; paragraph_format: Properties; character_format: Properties }
interface NativeFile {
  document: { id: string; metadata: { title: string }; styles: Style[]; sections: { blocks: { content: Paragraph }[] }[] };
  source?: { format: string; filename: string; bytes_base64: string };
}

const officeText = [
  'Project notes',
  'The next review will focus on dependable editing, accurate document layout, and reliable local saving. Writer keeps the document engine in Rust.',
  'Review priorities',
  'Confirm Unicode behavior — Café, e\u0301, 👩🏽‍💻 — and preserve formatting across native save and reopen cycles.',
  'Next steps',
  'Compare the saved document with the current revision, check keyboard navigation, and retain an exported backup. Audit edit e\u0301 👩🏽‍💻 العربية नमस्ते שלום.',
];
const fixtures = [
  { format: 'docx', path: '../../../tests/fixtures/docx/relationships/phase2-libreoffice-unicode.docx', expected: officeText },
  { format: 'odt', path: '../../../tests/fixtures/formats/libreoffice-audit-resaved.odt', expected: officeText },
  { format: 'rtf', path: '../../../tests/fixtures/formats/rtf-known-formatting.rtf', expected: [
    'Compatibility notes', 'Writer keeps logical Unicode e\u0301 and 😀 with bold, italic and underlined text.',
  ] },
] as const;
const media: Record<Format | 'writer', string> = {
  writer: 'application/json', docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  odt: 'application/vnd.oasis.opendocument.text', rtf: 'application/rtf',
};
const input = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const opening = (page: Page) => page.getByRole('dialog', { name: 'Open document', exact: true });
const notice = (page: Page) => page.getByRole('complementary', { name: 'Import formatting notice', exact: true });
const paragraphs = (file: NativeFile) => file.document.sections.flatMap((section) => section.blocks.map((block) => block.content));
const texts = (file: NativeFile) => paragraphs(file).map((paragraph) => paragraph.runs.map((run) => run.text).join(''));

async function activate(control: Locator, touch: boolean) {
  if (touch) await control.tap(); else await control.click();
}

async function command(page: Page, name: string, touch: boolean) {
  const menu = page.locator('details.document-menu');
  if (!await menu.evaluate((element) => element.hasAttribute('open'))) await activate(menu.locator('summary'), touch);
  await activate(menu.getByRole('button', { name, exact: true }), touch);
}

async function start(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
}

async function chooseFile(page: Page, format: Format, bytes: Buffer, filename: string, touch: boolean) {
  await command(page, 'Open document', touch);
  const choosing = page.waitForEvent('filechooser');
  await activate(opening(page).getByRole('button', { name: 'Open file…', exact: true }), touch);
  await (await choosing).setFiles({ name: filename, mimeType: media[format], buffer: bytes });
}

async function importEditable(page: Page, format: Format, bytes: Buffer, filename: string, touch: boolean) {
  await chooseFile(page, format, bytes, filename, touch);
  await expect(opening(page)).toHaveCount(0);
  await expect(input(page)).toHaveAttribute('contenteditable', 'true');
  await expect(page.locator('.save-state')).toHaveText('Saved on this device');
  await readyPages(page);
}

async function downloadBytes(download: Download) {
  expect(await download.failure()).toBeNull();
  const path = await download.path();
  if (!path) throw new Error('Missing actual browser download');
  return readFile(path);
}

async function copy(page: Page, format: Format | 'writer', touch: boolean, meaningful = false) {
  await command(page, 'Save As…', touch);
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption(format);
  await dialog.getByRole('textbox', { name: 'Filename', exact: true }).fill(`Phase 2 copy.${format}`);
  await expect(dialog.getByRole('checkbox')).toHaveCount(0);
  const warning = dialog.getByRole('note', { name: 'Format warning', exact: true });
  if (meaningful) {
    await expect(warning).toContainText(/original formatting.*different/i);
    expect((await warning.innerText()).length).toBeLessThan(500);
  } else if (format === 'writer') await expect(warning).toHaveCount(0);
  const pending = page.waitForEvent('download');
  await activate(dialog.getByRole('button', { name: 'Save copy', exact: true }), touch);
  const download = await pending;
  expect(download.suggestedFilename()).toBe(`Phase 2 copy.${format}`);
  await expect(dialog).toHaveCount(0);
  return downloadBytes(download);
}

function expectFormatting(file: NativeFile) {
  const specified = (properties: Properties): Properties => Object.fromEntries(Object.entries(properties).filter(([, value]) => value !== null));
  const styles = new Map(file.document.styles.map((style) => [style.id, style]));
  function inherited(id: string | null): { character: Properties; paragraph: Properties } {
    const style = id ? styles.get(id) : undefined;
    if (!style) return { character: {}, paragraph: {} };
    const parent = inherited(style.based_on);
    return { character: { ...parent.character, ...specified(style.character_format) }, paragraph: { ...parent.paragraph, ...specified(style.paragraph_format) } };
  }
  let count = 0;
  // Adapters may flatten or retain inheritance and elide redundant direct
  // values. Assert effective formatting, as in the existing integrity matrix.
  for (const paragraph of paragraphs(file)) {
    const style = inherited(paragraph.style);
    for (const run of paragraph.runs.filter((run) => run.text.length)) {
      expect({ ...style.character, ...specified(run.format) }).toMatchObject({
        bold: true, italic: true, underline: 'single', font_family: 'Tinos', font_size: 28,
      });
      count += 1;
    }
    expect({ ...style.paragraph, ...specified(paragraph.format) }).toMatchObject({ alignment: 'right' });
  }
  expect(count).toBeGreaterThan(0);
}

for (const touch of [false, true]) {
  test.describe(touch ? 'Phase 2 compatibility on touch' : 'Phase 2 compatibility on desktop', () => {
    if (touch) test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

    for (const item of fixtures) {
      test(`${item.format}: normalized import, Rust edits, native recovery and external reopen retain supported content`, async ({ page }) => {
        test.setTimeout(120_000);
        const errors: string[] = [];
        page.on('pageerror', (error) => errors.push(error.message));
        const source = await readFile(new URL(item.path, import.meta.url));
        await start(page);
        await importEditable(page, item.format, source, `Original.${item.format}`, touch);
        await expect(notice(page)).toBeVisible();
        await expect(input(page).locator('p')).toHaveText([...item.expected]);
        const originalNative = JSON.parse((await copy(page, 'writer', touch)).toString('utf8')) as NativeFile;
        expect(texts(originalNative)).toEqual(item.expected);
        expect(originalNative.source?.format).toBe(item.format);
        expect(Buffer.from(originalNative.source?.bytes_base64 ?? '', 'base64')).toEqual(source);

        const marker = ` — edited ${item.format.toUpperCase()} e\u0301 👩🏽‍💻 العربية नमस्ते שלום`;
        await focusEditor(page);
        await page.keyboard.press('Control+End');
        await page.keyboard.insertText(marker);
        await page.keyboard.press('Control+Home');
        await page.keyboard.press('Control+Shift+End');
        const ribbon = page.locator('#writer-formatting');
        for (const name of ['Bold', 'Italic', 'Underline']) {
          const control = ribbon.getByRole('button', { name, exact: true });
          if (await control.getAttribute('aria-pressed') !== 'true') await activate(control, touch);
          await expect(control).toHaveAttribute('aria-pressed', 'true');
        }
        if (touch) await activate(page.getByRole('button', { name: 'More formatting', exact: true }), true);
        await ribbon.getByRole('combobox', { name: 'Font family', exact: true }).selectOption('Tinos');
        await ribbon.getByRole('combobox', { name: 'Font size', exact: true }).selectOption('14');
        await activate(ribbon.getByRole('button', { name: 'Align right', exact: true }), touch);
        const title = `Phase 2 ${touch ? 'touch' : 'desktop'} ${item.format} recovery`;
        await page.getByRole('textbox', { name: 'Document title', exact: true }).fill(title);
        await activate(page.getByRole('button', { name: 'Save', exact: true }), touch);
        await expect(page.locator('.save-state')).toHaveText('Saved on this device');
        const beforeBytes = await copy(page, 'writer', touch);
        const before = JSON.parse(beforeBytes.toString('utf8')) as NativeFile;
        const expected = [...item.expected];
        expected[expected.length - 1] += marker;
        expect(texts(before)).toEqual(expected);
        expect(before.document.id).toBe(originalNative.document.id);
        expect(before.source).toEqual(originalNative.source);
        expectFormatting(before);

        await page.reload();
        await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
        await command(page, 'Open document', touch);
        await activate(opening(page).getByRole('button', { name: new RegExp(title) }), touch);
        await expect(opening(page)).toHaveCount(0);
        await readyPages(page);
        await expect(notice(page)).toBeVisible();
        expect(await copy(page, 'writer', touch)).toEqual(beforeBytes);
        const exported = await copy(page, item.format, touch, true);
        expect(exported).not.toEqual(source);
        expect(await copy(page, 'writer', touch)).toEqual(beforeBytes);
        await expect(page.locator('.save-state')).toHaveText('Saved on this device');
        await importEditable(page, item.format, exported, `Edited.${item.format}`, touch);
        const reopened = JSON.parse((await copy(page, 'writer', touch)).toString('utf8')) as NativeFile;
        expect(texts(reopened)).toEqual(expected);
        expect(Buffer.from(reopened.source?.bytes_base64 ?? '', 'base64')).toEqual(exported);
        expectFormatting(reopened);
        expect(errors).toEqual([]);
      });
    }

    test('unsupported content still preserves the current normalized document and exact recovery files', async ({ page }) => {
      test.setTimeout(120_000);
      const source = await readFile(new URL(fixtures[0].path, import.meta.url));
      await start(page);
      await importEditable(page, 'docx', source, 'Current original.docx', touch);
      await focusEditor(page);
      await page.keyboard.press('Control+End');
      await page.keyboard.insertText(' — retain current work e\u0301 👩🏽‍💻');
      await activate(page.getByRole('button', { name: 'Save', exact: true }), touch);
      await expect(page.locator('.save-state')).toHaveText('Saved on this device');
      const before = await copy(page, 'writer', touch);
      const cases: { format: Format; path: string }[] = [
        { format: 'docx', path: '../../../tests/fixtures/docx/relationships/unsupported-image.docx' },
        { format: 'odt', path: '../../../tests/fixtures/formats/unsupported-active-note.odt' },
        { format: 'rtf', path: '../../../tests/fixtures/formats/rtf-libreoffice.rtf' },
      ];
      for (const item of cases) {
        const bytes = await readFile(new URL(item.path, import.meta.url));
        await chooseFile(page, item.format, bytes, `Unsupported.${item.format}`, touch);
        const preserved = page.getByRole('dialog', { name: `${item.format.toUpperCase()} preserved read-only`, exact: true });
        await expect(preserved).toBeVisible();
        await expect(preserved).toContainText('Your current document is unchanged');
        expect(await preserved.locator('.compatibility-issues li').count()).toBeGreaterThan(0);
        const downloading = page.waitForEvent('download');
        await activate(preserved.getByRole('button', { name: `Download original ${item.format.toUpperCase()}`, exact: true }), touch);
        expect(await downloadBytes(await downloading)).toEqual(bytes);
        await activate(preserved.getByRole('button', { name: 'Close', exact: true }), touch);
        expect(await copy(page, 'writer', touch)).toEqual(before);
      }
      await focusEditor(page);
      await page.keyboard.press('Control+z');
      await expect(input(page).locator('p')).toHaveText([...officeText]);
      await expect(notice(page)).toBeVisible();
    });
  });
}
