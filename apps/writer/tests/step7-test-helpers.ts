import { expect, type Page } from '@playwright/test';

/** Functional document-menu path shared by desktop and narrow-window tests. */
export async function documentAction(page: Page, name: string) {
  const menu = page.locator('details.document-menu');
  if (!(await menu.evaluate((element) => element.hasAttribute('open')))) {
    await menu.locator('summary').click();
  }
  await menu.getByRole('button', { name, exact: true }).click();
}

/** Focus the actual semantic input proxy; pointer behavior is tested on SVG. */
export async function focusEditor(page: Page) {
  const input = page.getByRole('textbox', { name: 'Document content', exact: true });
  await input.focus();
  await expect(input).toBeFocused();
}

/** Wait for the displayed Rust snapshot rather than a browser text-layout pass. */
export async function readyPages(page: Page) {
  const root = page.getByTestId('paged-editor');
  await expect(root).toHaveAttribute('aria-busy', 'false');
  await expect(root.locator('svg[data-layout-page]').first()).toBeVisible();
  return root;
}

/**
 * Translate a Rust SVG glyph origin to a screen point. SVG's supplied transform
 * carries the engine position; this never measures browser-shaped text.
 */
export async function glyphPoint(page: Page, paragraphIndex: number, glyphIndex: number) {
  const paragraphs = page.getByRole('textbox', { name: 'Document content', exact: true }).locator('p');
  const id = await paragraphs.nth(paragraphIndex).getAttribute('data-paragraph-id');
  const root = await readyPages(page);
  return root.locator('g[data-layout-paragraph-id]').evaluateAll((groups, target) => {
    const matching = groups.filter((group) => group.getAttribute('data-layout-paragraph-id') === target.id);
    const glyphs = matching.flatMap((group) => [...group.querySelectorAll('use')]);
    const glyph = glyphs[target.glyphIndex];
    if (!(glyph instanceof SVGGraphicsElement)) throw new Error('Missing actual Rust glyph in the displayed paragraph');
    const matrix = glyph.getScreenCTM();
    if (!matrix) throw new Error('The Rust glyph has no displayed SVG transform');
    const point = new DOMPoint(0, 0).matrixTransform(matrix);
    // Baseline-minus-one is safely inside the line, while staying near its
    // exact Rust glyph-origin x for logical hit testing.
    return { x: point.x + 0.05, y: point.y - 1 };
  }, { id, glyphIndex });
}
