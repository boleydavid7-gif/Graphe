import { readFile } from 'node:fs/promises';
import { expect, test, type Page } from '@playwright/test';
import { documentAction, focusEditor, readyPages } from './step7-test-helpers';

type Format = 'writer' | 'docx' | 'odt' | 'rtf' | 'txt';
type Properties = Record<string, unknown>;
interface Run { text: string; format: Properties }
interface Paragraph { id: string; style: string | null; format: Properties; runs: Run[] }
interface Style { id: string; based_on: string | null; paragraph_format: Properties; character_format: Properties }
interface Native {
  document: {
    metadata: { title: string };
    styles: Style[];
    sections: { blocks: { content: Paragraph }[] }[];
  };
  source?: { format: string; filename: string; bytes_base64: string };
}

const editor = (page: Page) => page.getByRole('textbox', { name: 'Document content', exact: true });
const media: Record<Format, string> = {
  writer: 'application/octet-stream', docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  odt: 'application/vnd.oasis.opendocument.text', rtf: 'application/rtf', txt: 'text/plain',
};

/** Test input only: no engine hooks or competing application model. */
function sample() {
  const blocks = [
    { kind: 'paragraph', content: {
      id: 'audit-heading', style: 'audit-heading-style', format: { alignment: 'center' },
      runs: [{ text: 'Quarterly project review — Café e\u0301', format: {} }],
    } },
    ...Array.from({ length: 32 }, (_, index) => ({ kind: 'paragraph', content: {
      id: `audit-body-${index}`, style: 'audit-body-style',
      format: { spacing_after: 160, alignment: index % 2 ? 'left' : 'right' },
      runs: [
        { text: `Review ${index + 1}. The team verified `, format: {} },
        { text: 'dependable saving', format: { bold: true } },
        { text: ', ', format: {} },
        { text: 'thoughtful editing', format: { italic: true } },
        { text: ', and ', format: {} },
        { text: 'document integrity', format: { underline: 'single' } },
        { text: '. Unicode remains intact: Café e\u0301 👩🏽‍💻 العربية नमस्ते שלום. Each paragraph stays in its original order.', format: {} },
      ],
    } })),
  ];
  return Buffer.from(JSON.stringify({
    format: 'office-writer', format_version: 2,
    document: {
      schema_version: 2, id: 'audit-document', metadata: { title: 'Quarterly project review' },
      styles: [
        { id: 'audit-body-style', name: 'Body', based_on: null, paragraph_format: {}, character_format: { font_family: 'Arimo', font_size: 24 } },
        { id: 'audit-heading-style', name: 'Heading', based_on: 'audit-body-style', paragraph_format: { spacing_after: 240 }, character_format: { font_family: 'Tinos', font_size: 36, bold: true } },
      ],
      sections: [{ id: 'audit-section', properties: null, blocks }],
    },
  }));
}

async function openFile(page: Page, format: Format, bytes: Buffer, filename: string) {
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Open document', exact: true });
  const pending = page.waitForEvent('filechooser');
  await dialog.getByRole('button', { name: 'Open file…', exact: true }).click();
  await (await pending).setFiles({ name: filename, mimeType: media[format], buffer: bytes });
  await expect(dialog).toHaveCount(0);
  await readyPages(page);
}

async function copy(page: Page, format: Format, filename: string, expectMeaningfulLoss = false) {
  await documentAction(page, 'Save As…');
  const dialog = page.getByRole('dialog', { name: 'Save As', exact: true });
  await dialog.getByRole('combobox', { name: 'File format', exact: true }).selectOption(format);
  await dialog.getByRole('textbox', { name: 'Filename', exact: true }).fill(filename);
  if (expectMeaningfulLoss) await expect(dialog.getByRole('note', { name: 'Format warning', exact: true })).toBeVisible();
  const pending = page.waitForEvent('download');
  await dialog.getByRole('button', { name: 'Save copy', exact: true }).click();
  const download = await pending;
  const path = await download.path();
  if (!path) throw new Error('Missing browser download');
  expect(await download.failure()).toBeNull();
  expect(download.suggestedFilename()).toBe(filename);
  return readFile(path);
}

const paragraphs = (native: Native) => native.document.sections.flatMap((section) => section.blocks.map((block) => block.content));
const texts = (native: Native) => paragraphs(native).map((paragraph) => paragraph.runs.map((run) => run.text).join(''));

function specified(properties: Properties): Properties {
  return Object.fromEntries(Object.entries(properties).filter(([, value]) => value !== null));
}

/** Compare supported effective values while allowing adapters to flatten styles. */
function supportedFormatting(native: Native) {
  const styles = new Map(native.document.styles.map((style) => [style.id, style]));
  function inherited(id: string | null): { character: Properties; paragraph: Properties } {
    const style = id ? styles.get(id) : undefined;
    if (!style) return { character: {}, paragraph: {} };
    const parent = inherited(style.based_on);
    return { character: { ...parent.character, ...specified(style.character_format) }, paragraph: { ...parent.paragraph, ...specified(style.paragraph_format) } };
  }
  return paragraphs(native).map((paragraph) => {
    const style = inherited(paragraph.style);
    const paragraphFormat = { ...style.paragraph, ...specified(paragraph.format) };
    // Per-scalar records intentionally ignore adapter run partitioning.
    return {
      paragraph: { alignment: paragraphFormat.alignment ?? 'left', spacing_after: paragraphFormat.spacing_after ?? 0 },
      characters: paragraph.runs.flatMap((run) => {
        const format = { ...style.character, ...specified(run.format) };
        return [...run.text].map((text) => ({ text, format: {
          bold: format.bold ?? false, italic: format.italic ?? false, underline: format.underline ?? 'none',
          font_family: format.font_family ?? null, font_size: format.font_size ?? null,
        } }));
      }),
    };
  });
}

for (const format of ['writer', 'docx', 'odt', 'rtf', 'txt'] as const) {
  test(`${format}: realistic multi-page import, edit, native recovery, export and reopen retain content`, async ({ page, browser }) => {
    test.setTimeout(120_000);
    await page.goto('/');
    await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
    await readyPages(page);
    await openFile(page, 'writer', sample(), 'quarterly-source.writer');
    await expect(editor(page).locator('p')).toHaveCount(33);
    expect(await page.locator('svg[data-layout-page]').count()).toBeGreaterThan(1);
    if (format === 'writer') {
      await page.screenshot({ path: '../../docs/images/writer-audit-desktop.png' });
      const previous = page.viewportSize();
      await page.setViewportSize({ width: 390, height: 844 });
      await readyPages(page);
      await page.screenshot({ path: '../../docs/images/writer-audit-mobile.png' });
      if (previous) await page.setViewportSize(previous);
      await readyPages(page);
    }
    const original = await copy(page, format, `quarterly-original.${format}`, format === 'txt');

    // The actual import runs in an independent browser-local library.
    const context = await browser.newContext({ acceptDownloads: true });
    const other = await context.newPage();
    try {
      await other.goto('/');
      await expect(other.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
      await readyPages(other);
      await openFile(other, format, original, `quarterly-original.${format}`);
      await expect(editor(other).locator('p')).toHaveCount(33);
      expect(await other.locator('svg[data-layout-page]').count()).toBeGreaterThan(1);
      const imported = JSON.parse((await copy(other, 'writer', `imported-${format}.writer`)).toString('utf8')) as Native;
      const source = JSON.parse(sample().toString('utf8')) as Native;
      expect(texts(imported)).toEqual(texts(source));
      if (format === 'txt') {
        expect(paragraphs(imported).flatMap((paragraph) => paragraph.runs).every((run) =>
          !run.format.bold && !run.format.italic && (!run.format.underline || run.format.underline === 'none'))).toBe(true);
      } else expect(supportedFormatting(imported)).toEqual(supportedFormatting(source));
      await focusEditor(other);
      await other.keyboard.press('Control+End');
      await other.keyboard.insertText(' — audited edit ✍🏽 e\u0301');
      await other.getByRole('textbox', { name: 'Document title', exact: true }).fill(`Audited ${format} quarterly review`);
      await other.getByRole('button', { name: 'Save', exact: true }).click();
      await expect(other.locator('.save-state')).toHaveText('Saved on this device');
      const beforeBytes = await copy(other, 'writer', `audited-${format}.writer`);
      const before = JSON.parse(beforeBytes.toString('utf8')) as Native;
      const expectedText = texts(JSON.parse(sample().toString('utf8')) as Native);
      expectedText[expectedText.length - 1] += ' — audited edit ✍🏽 e\u0301';
      expect(texts(before)).toEqual(expectedText);
      if (format !== 'writer') {
        expect(before.source?.format).toBe(format);
        expect(Buffer.from(before.source?.bytes_base64 ?? '', 'base64')).toEqual(original);
      }

      await other.reload();
      await expect(other.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
      await other.getByRole('button', { name: 'Open', exact: true }).click();
      await other.getByRole('button', { name: new RegExp(`Audited ${format} quarterly review`) }).click();
      await readyPages(other);
      const recovered = JSON.parse((await copy(other, 'writer', `recovered-${format}.writer`)).toString('utf8')) as Native;
      expect(recovered).toEqual(before);
      expect(await other.locator('svg[data-layout-page]').count()).toBeGreaterThan(1);

      const exported = await copy(other, format, `audited-export.${format}`);
      await expect(other.locator('.save-state')).toHaveText('Saved on this device');
      await openFile(other, format, exported, `audited-export.${format}`);
      const reopened = JSON.parse((await copy(other, 'writer', `reopened-${format}.writer`)).toString('utf8')) as Native;
      expect(texts(reopened)).toEqual(expectedText);
      if (format === 'writer') expect(reopened).toEqual(before);
      else {
        expect(reopened.source?.format).toBe(format);
        expect(Buffer.from(reopened.source?.bytes_base64 ?? '', 'base64')).toEqual(exported);
      }
      expect(supportedFormatting(reopened)).toEqual(supportedFormatting(before));
      await expect(editor(other).locator('p')).toHaveCount(33);
      expect(await other.locator('svg[data-layout-page]').count()).toBeGreaterThan(1);
    } finally { await context.close(); }
  });
}
