import { expect, test } from '@playwright/test';
import { focusEditor, readyPages } from './step7-test-helpers';

test('Layout flags toggle independently and change undo history without losing selection', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
  await readyPages(page);
  await focusEditor(page);
  await page.keyboard.insertText('Heading');
  await page.getByRole('button', { name: 'Layout', exact: true }).click();
  const together = page.getByRole('button', { name: 'Keep together', exact: true });
  const next = page.getByRole('button', { name: 'Keep with next', exact: true });
  await expect(together).toHaveAttribute('aria-pressed', 'false');
  await expect(next).toHaveAttribute('aria-pressed', 'false');
  await together.click();
  await expect(together).toHaveAttribute('aria-pressed', 'true');
  await next.click();
  await expect(next).toHaveAttribute('aria-pressed', 'true');
  await together.click();
  await expect(together).toHaveAttribute('aria-pressed', 'false');
  await expect(next).toHaveAttribute('aria-pressed', 'true');
});
