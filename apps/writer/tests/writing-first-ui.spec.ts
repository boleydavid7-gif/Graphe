import { expect, test } from '@playwright/test';
import { readyPages, focusEditor } from './step7-test-helpers';

// Browser viewport coverage; physical keyboard and native touch still need device checks.
test.describe('Graphe responsive Writing First UI', () => {
  test('mobile ribbon stays compact and bottom tools open real editor controls', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/');
    await readyPages(page);
    const ribbon = page.locator('.formatting-bar');
    await expect(ribbon).toHaveCSS('height', '52px');
    const tools = page.getByRole('navigation', { name: 'Document tools', exact: true });
    await expect(tools).toBeVisible();

    await tools.getByRole('button', { name: 'Outline' }).click();
    const outline = page.getByRole('complementary', { name: 'Document outline', exact: true });
    await expect(outline).toBeVisible();
    await outline.getByRole('button', { name: 'Close document outline' }).click();
    await expect(outline).toHaveCount(0);

    await tools.getByRole('button', { name: 'Format' }).click();
    const inspector = page.getByRole('complementary', { name: 'Formatting inspector', exact: true });
    await expect(inspector).toBeVisible();
    await inspector.getByRole('button', { name: 'Close formatting inspector' }).click();
    await expect(inspector).toHaveCount(0);

    const more = page.getByRole('button', { name: 'Show text formatting', exact: true });
    await more.click();
    await expect(ribbon).toHaveCSS('height', '52px');
    await expect(page.getByRole('combobox', { name: 'Font family', exact: true })).toBeVisible();
  });

  test('a new document can add a heading and navigate to it from Outline', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/');
    await readyPages(page);
    await focusEditor(page);
    await page.keyboard.insertText('Overview');
    const styles = page.getByRole('combobox', { name: 'Paragraph style', exact: true });
    await expect(styles.locator('option')).toContainText(['Paragraph', 'Heading 1', 'Heading 2', 'Heading 3']);
    await styles.selectOption(JSON.stringify('graphe-heading:1'));
    await expect(styles).not.toHaveValue(JSON.stringify('graphe-heading:1'));
    await page.getByRole('navigation', { name: 'Document tools', exact: true })
      .getByRole('button', { name: 'Outline' }).click();
    await expect(page.getByRole('complementary', { name: 'Document outline', exact: true })
      .getByRole('button', { name: /Overview/ })).toBeVisible();
  });

  test('tablet inspector is reachable and dismissible', async ({ page }) => {
    await page.setViewportSize({ width: 800, height: 900 });
    await page.goto('/');
    await readyPages(page);
    const toggle = page.getByRole('button', { name: 'Details', exact: true });
    await expect(toggle).toBeVisible();
    await toggle.click();
    const inspector = page.getByRole('complementary', { name: 'Formatting inspector', exact: true });
    await expect(inspector).toBeVisible();
    await inspector.getByRole('button', { name: 'Close formatting inspector' }).click();
    await expect(inspector).toHaveCount(0);
  });
});
