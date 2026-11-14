# Native Writer format fixtures

`writer-v1.writer` is an Office Engine-owned sample of native format version 1.
It wraps the existing `writer-sample-v1.json` model fixture without changing any
supported model values. It includes two sections, several paragraphs and text
runs, named style inheritance, mixed formatting, combining marks, emoji, and RTL
text.

This fixture records a supported native input format. JSON whitespace, object key
ordering, and omitted optional formatting fields need not match the serializer's
deterministic output. Tests compare complete decoded documents and verify stable
serialized output separately. The `.writer` extension and specification remain
provisional before public release.
