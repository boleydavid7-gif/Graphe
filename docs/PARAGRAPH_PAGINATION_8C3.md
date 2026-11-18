# Graphe 8C.3 — Paragraph pagination

Development branch `dev/graphe-step8c3-paragraph-pagination`. Not merged or deployed.

## Scope
- Layout → **Keep together** moves all lines of a body paragraph to the next page before placing any line, if the paragraph can fit on a fresh page.
- Layout → **Keep with next** moves a body paragraph to the next page if needed to keep it and the *first line* of its immediately following body paragraph together. Next-page section/page breaks and non-paragraph blocks are boundaries.
- The existing Rust layout engine shapes the next paragraph at most once using a one-element cache. No CSS/browser pagination.
- New explicit `keep_together` / `keep_with_next` paragraph/style properties use revision-guarded Rust transactions, undo/redo, serialization and a strictly migrated native schema/envelope **7**. Format-6 and older files may not smuggle the fields under old labels.
- DOCX uses `w:keepLines` and `w:keepNext`; ODT uses `fo:keep-together` and `fo:keep-with-next`; RTF uses `\\keep` and `\\keepn`. TXT formatting-loss assessment includes these controls. PDF uses Rust pagination.

## Limits
- Paragraphs larger than one full page remain splittable and produce a diagnostic rather than an infinite page-creation loop.
- Keep-with-next covers the next paragraph's first line, not chained groups of several headings or table blocks. Widow/orphan control is a separate milestone.
- Empty pages, headers/footers, continuous section breaks, table-row splitting, and exact Word pagination parity are not added. Native and external format handling must be verified independently.
- Verification required before merge: workspace cargo fmt/clippy/tests/build, TypeScript typecheck, Vite build, compiled WASM and Chromium tests, mobile keyboard/manual checks. Cloudflare compile and UI smoke tests are not substitutes for a full code/security audit.

## Step 8C.4 — default two-line widow/orphan protection

Rust pagination now prevents a single paragraph line from being stranded at the bottom or top of a page whenever the paragraph's shaped line boxes can satisfy a two-line minimum. Short two- or three-line paragraphs move together when they fit on a fresh page. Longer paragraphs reserve two starting lines and move the final two lines together when a legal break is possible. The existing Keep together and Keep with next controls remain authoritative; Keep with next reserves two lines of its following paragraph where the pair fits.

This is an **automatic layout default**, not a per-paragraph toggle. The native document schema, editor commands and file-format adapters do not change. Very tall or unusual lines still use the bounded existing layout fallback; exact Microsoft Word/LibreOffice line placement is not promised. Table-cell text continues under the existing table-row policy. Validate the Rust pagination tests and Cloudflare build before treating it as fully tested.
