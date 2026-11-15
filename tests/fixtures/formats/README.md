# File-format fixtures

These files exercise the declared adapters and preservation policy. A fixture's
presence does not imply every construct in it is editable. Hand-authored fixtures
from the adapter tests are deliberately small; external-producer files below are
not sanitized into the Writer profile.

## Known RTF formatting normalization profile

`rtf-known-formatting.rtf` is a hand-authored regression input containing named
styles, primary and alternate font names, language settings, widow/orphan and
hyphenation preferences, a kerning threshold, mixed bold/italic/underline,
combining Unicode and an emoji. SHA-256:
`aace5f44592c76c5fc114862e22fd146ac9eb989bc0438325cc2ed7d4687cade`.

Its known unsupported formatting is normalized with explicit import notices;
the complete supported content is editable and the exact original remains in
native Writer saves. Converted copies may differ in fallback fonts, proofing,
pagination, hyphenation, kerning and automatic next-paragraph style behavior.
It is not an externally generated LibreOffice or Microsoft Word fixture.

The classification follows Microsoft's [RTF 1.9.1 specification](https://officeprotocoldoc.z19.web.core.windows.net/files/Archive_References/%5BMSFT-RTF%5D.pdf),
linked from its [normative references](https://learn.microsoft.com/en-us/openspecs/exchange_server_protocols/ms-oxrtfex/3402a589-2578-4a15-99ce-04a91545f327).
Alternate font labels are distinct from embedded-font payloads. Direction and
associated-script font controls, actual lists/notes/fields, unknown destinations,
unsupported byte encodings and malformed structures remain guarded; this
fixture does not claim support for those features.

## Independent LibreOffice-produced inputs

### Phase 2 audited ODT resave

`libreoffice-audit-resaved.odt` is the unchanged public synthetic LibreOffice
resave produced during the November 15 stability audit from Writer's exported
`tests/fixtures/native/project-notes.writer`, with the audit's Unicode and mixed
formatting edits. Producer: LibreOfficeDev 26.8.0.0.alpha0, source revision
`2c87e51eeaa2b413ff4ae097b2705eea1995d8e5`. The audit artifact was copied from
`/tmp/writer-audit-independent-formats/lo-roundtrip/odt/audit.odt`; it is not a
private user document and was not sanitized to remove unsupported properties.
SHA-256: `d64d5bbd26c0eb2f5fa765405286a9c71d7ea2204b45556e02f1028e0db1d763`.

The Phase 2 profile recognizes its exact inactive default/outline/note settings,
known application configuration, single-column section and package scaffolding.
Unsupported language/script-font/layout preferences produce explicit notices.
All six Unicode paragraphs, supported formatting, title and page geometry remain
editable, with exact originals retained in native saves. This does not establish
support for all ODT files produced by LibreOffice. The older
`libreoffice-basic.odt` below still contains additional preferences outside this
reviewed profile and remains preserved read-only.

`unsupported-active-note.odt` is a hand-modified copy of the public audited ODT,
with a real footnote and its text added to `content.xml`; the package was rebuilt
without changing the unrelated semantic parts. It proves inactive note settings
do not permit dropping actual note content. It must remain preserved read-only.
SHA-256: `82df43e5c97ada8657f5ac6b7184c9afb70bef2dff4522617d4e665e90096f38`.

### Earlier Step 7.5 inputs

Producer: `LibreOfficeDev 26.8.0.0.alpha0`, source revision
`2c87e51eeaa2b413ff4ae097b2705eea1995d8e5`, installed Linux test tool.
Input: the repository's existing `tests/fixtures/docx/supported-basic.docx`.
No LibreOffice code or runtime dependency is incorporated into Writer.

The exact commands executed were:

```sh
soffice -env:UserInstallation=file:///tmp/writer-step75-libreoffice-profile --headless --convert-to 'rtf:Rich Text Format' --outdir /tmp/writer-step75-external /workspace/stuff/tests/fixtures/docx/supported-basic.docx
soffice -env:UserInstallation=file:///tmp/writer-odt-profile --headless --convert-to odt --outdir /tmp/writer-odt-reference tests/fixtures/docx/supported-basic.docx
```

The resulting files were copied without semantic changes to:

| File | SHA-256 | Expected profile behavior |
| --- | --- | --- |
| `rtf-libreoffice.rtf` | `db2ed350bc60292865808dda244df547420bbe851239cb97fddf175091ba8305` | Preserved read-only: producer defaults/extensions include properties outside the editable model |
| `libreoffice-basic.odt` | `e34f07424cab7500e41c35df268d6c40fe21d3579d6d41b86c5a12701ba0a421` | Preserved read-only: producer defaults/extensions include properties outside the editable model |

Both inputs contain unrepresented properties even though their visible body is
simple. Regression tests must retain the exact original and report unsupported
semantics rather than weaken the import gate to make these fixtures editable.
Successful export consumption by an external application is a separate check
from opening every file that application can generate. See the Step 7.5
verification record for commands and actual results.

## Independent consumer checks of Writer exports

The checked-in verification example generated all six outputs from the same
real `tests/fixtures/native/project-notes.writer` fixture. Run from the repository
root; choose a new output directory because the example refuses overwrites:

```sh
mkdir -p /tmp/writer-step75-repro-export
for writer_format in docx odt rtf txt pdf writer; do
  cargo run --offline --locked -p office-formats --example export_format -- tests/fixtures/native/project-notes.writer "$writer_format" "/tmp/writer-step75-repro-export/project-notes.$writer_format"
done
```

This command actually passed for all six formats. Exported byte counts were
DOCX 2,394; ODT 2,310; RTF 1,158; TXT 422; PDF 80,148; native Writer 8,421 on the
reviewed implementation. The example retains full native original-source state
when `writer` is selected rather than using the document-only exporter.

The independently consumed artifacts were first produced through an isolated
temporary Rust probe and verified byte-for-byte equal to this reproducible
example's external outputs. The exact consumer commands executed were:

```sh
soffice -env:UserInstallation=file:///tmp/writer-step75-readback-profile --headless --convert-to 'txt:Text (encoded):UTF8' --outdir /tmp/writer-step75-external-readback /tmp/writer-step75-exported/writer-export-docx.docx /tmp/writer-step75-exported/writer-export-odt.odt /tmp/writer-step75-exported/writer-export-rtf.rtf /tmp/writer-step75-exported/writer-export-txt.txt
soffice -env:UserInstallation=file:///tmp/writer-step75-readback-fodt-profile --headless --convert-to fodt --outdir /tmp/writer-step75-external-readback /tmp/writer-step75-exported/writer-export-docx.docx /tmp/writer-step75-exported/writer-export-odt.odt /tmp/writer-step75-exported/writer-export-rtf.rtf
soffice -env:UserInstallation=file:///tmp/writer-step75-readback-pdf-profile --headless --convert-to pdf --outdir /tmp/writer-step75-external-readback /tmp/writer-step75-exported/writer-export-docx.docx /tmp/writer-step75-exported/writer-export-odt.odt /tmp/writer-step75-exported/writer-export-rtf.rtf
pdfinfo /tmp/writer-step75-exported/writer-export-pdf.pdf
pdftoppm -singlefile -r 72 -png /tmp/writer-step75-exported/writer-export-pdf.pdf /tmp/writer-step75-external-readback/writer-export-pdf
pdftotext /tmp/writer-step75-exported/writer-export-pdf.pdf /tmp/writer-step75-external-readback/outlined-pdf-text.txt
```

Results: all four editable-format exports produced the exact native text, after
explicitly consuming the reader's leading UTF-8 BOM and one final paragraph LF.
No combining-mark/emoji normalization was applied. LibreOffice flat-ODF styles
retained Tinos 20-point bold heading, Arimo 12-point body and mixed bold emphasis.
PyMuPDF 1.26.6 inspection of LibreOffice's PDFs independently confirmed 20-point
bold serif heading, 12-point normal/bold body spans and `#24211d` text color.
The consumer substituted installed Noto/Liberation fonts, so this is not a claim
of identical font metrics or pagination. Poppler 26.05.0 accepted and rendered
Writer's own outlined PDF; it has no extractable text, as explicitly warned.
