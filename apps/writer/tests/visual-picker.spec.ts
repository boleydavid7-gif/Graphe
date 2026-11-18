import { expect, test } from '@playwright/test';
import { focusEditor, readyPages } from './step7-test-helpers';

test('visual font menu shows real bundled typefaces with substitute labels', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.goto('/');
  await readyPages(page);
  await focusEditor(page);
  await page.keyboard.insertText('Type preview sample');
  await page.getByRole('button', { name: 'Browse fonts with previews', exact: true }).click();
  const gallery = page.getByRole('dialog', { name: 'Font previews', exact: true });
  await expect(gallery).toBeVisible();
  const tinos = gallery.getByRole('button', { name: /Tinos \(Times New Roman\)/ });
  await expect(tinos).toBeVisible();
  await expect(tinos.locator('.writer-choice-preview')).toHaveCSS('font-family', /Tinos/);
  await expect(gallery.getByRole('button', { name: /Arimo \(Arial\)/ })).toBeVisible();
  await tinos.click();
  await expect(gallery).toHaveCount(0);
  await expect(page.locator('#font-family')).toHaveValue('Tinos');
});

test('mobile heading previews apply a persistent style and populate Outline', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await readyPages(page);
  await focusEditor(page);
  await page.keyboard.insertText('Overview');
  await page.getByRole('button', { name: 'Browse styles with previews', exact: true }).click();
  const gallery = page.getByRole('dialog', { name: 'Style previews', exact: true });
  await expect(gallery).toBeVisible();
  await expect(gallery.getByRole('button', { name: /Heading 1/ })
    .locator('.writer-choice-preview')).toHaveCSS('font-size', '18px');
  await gallery.getByRole('button', { name: /Heading 1/ }).click();
  await expect(gallery).toHaveCount(0);
  await expect(page.locator('#paragraph-style')).not.toHaveValue('null');
  await page.getByRole('navigation', { name: 'Document tools', exact: true })
    .getByRole('button', { name: 'Outline' }).click();
  await expect(page.getByRole('complementary', { name: 'Document outline', exact: true })
    .getByRole('button', { name: /Overview/ })).toBeVisible();
});

test('visual choice dialog closes with Escape without changing document formatting', async ({ page }) => {
  await page.goto('/');
  await readyPages(page);
  const before = await page.locator('#font-family').inputValue();
  await page.getByRole('button', { name: 'Browse fonts with previews', exact: true }).click();
  await page.getByRole('dialog', { name: 'Font previews', exact: true }).press('Escape');
  await expect(page.getByRole('dialog', { name: 'Font previews', exact: true })).toHaveCount(0);
  await expect(page.locator('#font-family')).toHaveValue(before);
});
