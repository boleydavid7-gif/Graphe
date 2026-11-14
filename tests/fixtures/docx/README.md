# DOCX package fixtures

These deterministic, handcrafted fixtures were generated with Python's standard
ZIP library by `python3 tests/fixtures/docx/generate.py`. No Office engine or
third-party document was embedded. They establish package-inspection and exact
original-byte preservation, not verified Word/LibreOffice interoperability.

- `writer-source-preservation.docx`: Transitional package with paragraphs,
  mixed formatting and Unicode, named styles/docDefaults, a table, field,
  hyperlink, section layout, markup-compatibility choice/fallback, custom XML,
  creator metadata, and a generated one-pixel PNG. Several constructs are not
  representable in the current shared model. Step 5A retains all original bytes
  without creating an editable projection or following its external URL.
- `strict-relocated.docx`: Strict namespace, alternate metadata prefixes, and a
  main part at `documents/text/main.xml`, proving discovery via relationships
  rather than a hard-coded `word/document.xml` path.

ZIP timestamps, order and comments are fixed. The PNG is generated directly from
standard PNG chunks and zlib; it is not downloaded artwork. Regenerating should
preserve semantic fixture contents; compression bytes can vary with Python/zlib
versions. Checked-in binary bytes are the preservation-test reference.
