# DOCX relationship regression fixtures

These fixtures distinguish ordinary producer metadata from unsupported document
content. Passing them establishes this bounded import profile, not general Word
fidelity. Known formatting may be approximated with separate normalization
notices. Unsupported content/security semantics retain their exact original and
stay read-only; malformed packages fail explicitly. No external targets are fetched.

## Genuine producer originals

`microsoft-sdk-hello-world.docx` and `microsoft-sdk-plain.docx` are unmodified
`HelloWorld.docx` and `Plain.docx` from the official
[Open XML SDK test assets](https://github.com/dotnet/Open-XML-SDK/tree/431ab05cf160248cc3885a4a766026d4f8243792/test/DocumentFormat.OpenXml.Tests.Assets/assets/TestFiles),
pinned at commit `431ab05cf160248cc3885a4a766026d4f8243792`. Both identify their
producer as Microsoft Office Word in `docProps/app.xml` (versions 16.0000 and
15.0000 respectively). The upstream MIT license and attribution are retained in
[OPEN_XML_SDK_LICENSE.txt](OPEN_XML_SDK_LICENSE.txt). This is a fixture license,
not a license selection for Writer.

Downloaded from the pinned public upstream, without rewriting the ZIP or XML:

```sh
curl --fail --location --max-filesize 50000 --output tests/fixtures/docx/relationships/microsoft-sdk-hello-world.docx https://raw.githubusercontent.com/dotnet/Open-XML-SDK/431ab05cf160248cc3885a4a766026d4f8243792/test/DocumentFormat.OpenXml.Tests.Assets/assets/TestFiles/HelloWorld.docx
curl --fail --location --max-filesize 50000 --output tests/fixtures/docx/relationships/microsoft-sdk-plain.docx https://raw.githubusercontent.com/dotnet/Open-XML-SDK/431ab05cf160248cc3885a4a766026d4f8243792/test/DocumentFormat.OpenXml.Tests.Assets/assets/TestFiles/Plain.docx
```

Microsoft Word itself was not executed in this environment. These are upstream
Word-producer fixtures, not a claim of a current Word application test.

`libreoffice-basic.docx` was actually generated from this repository's
`tests/fixtures/docx/supported-basic.docx` with LibreOfficeDev 26.8.0.0.alpha0,
revision `2c87e51eeaa2b413ff4ae097b2705eea1995d8e5`:

```sh
mkdir -p /tmp/writer-docx-relationship-libreoffice
soffice -env:UserInstallation=file:///tmp/writer-docx-relationships-profile --headless --convert-to 'docx:Office Open XML Text' --outdir /tmp/writer-docx-relationship-libreoffice tests/fixtures/docx/supported-basic.docx
cp /tmp/writer-docx-relationship-libreoffice/supported-basic.docx tests/fixtures/docx/relationships/libreoffice-basic.docx
```

Generation timestamps can vary. The checked-in bytes below are the actual
generated artifact, without stripping producer settings or styles.

The initial relationship correction left all three genuine files read-only for
unsupported formatting. The owner's subsequent normalization policy requires
editable projections with explicit approximation notices while preserving all
original paragraphs and text. Tests verify native source-byte retention and
reopening exported current content. Ordinary auxiliary parts do not produce
false relationship findings; unrepresented content remains blocked.

## Synthetic boundary fixtures

Run `python3 tests/fixtures/docx/relationships/generate.py` from the repository
root. The script uses only the Python standard library, deterministic ZIP entry
ordering/timestamps, the existing basic document and tiny PNG fixtures, and
Word theme/styles/settings/font/view parts copied from the MIT-licensed SDK
fixtures above. These files are explicitly synthetic, not Word-generated.

`supported-auxiliary-parts.docx` includes a supported default paragraph style,
uniform Arimo script families/12 pt sizes, simple core/extended/custom metadata,
an unused theme, a font descriptor table, inert zoom/proofing/session settings,
and browser-view settings. Native saving retains every original byte, including
metadata not represented in the semantic model. Current-content DOCX export
regenerates the supported model; it is a separate operation from original download.

`supported-authoring-metadata.docx` adds valid eight-hex-digit `w:rsid*` authoring
IDs, including `w:rsidRDefault`, and a bound `mc:Ignorable` declaration. Its negative siblings prove that
unbound prefixes, malformed IDs, actual unknown extension properties and revision
elements remain blocked. `mc:Ignorable` is not a permission to discard markup.

`supported-root-thumbnail.docx` adds an opaque cached package thumbnail.
`unsupported-main-image-same-thumbnail.docx` adds a body image relationship to
that same target and must still be read-only. Supporting a metadata thumbnail
does not implement images in Writer.

`supported-strict-auxiliary-parts.docx` exercises those XML roles and a cached
thumbnail using Strict WordprocessingML, DrawingML and metadata namespaces. It
uses the exact Strict `extendedProperties`/`customProperties` relationship names,
which differ from Transitional `extended-properties`/`custom-properties`.
`supported-strict-authoring-metadata.docx` covers the same authoring IDs in
Strict markup. Three independent Strict theme-reference cases now verify that
known font/color dependencies receive normalization notices while preserving text.

`normalizable-wordweb-profile.docx` combines public SDK Plain style, settings,
font and view parts with nine newly authored paragraphs and seventeen runs.
It exercises theme defaults, authoring IDs, false tracking/caps/noProof flags,
theme color annotations, basic section defaults and mixed Unicode formatting.
No private user content or metadata is copied into this synthetic fixture.
Two independent protected siblings enable document enforcement or revision
tracking and must stay read-only despite the same normalizable formatting.
The warning-flood fixture places actual revision content after 160 language
formatting notices, proving that notice limits cannot suppress content blockers.

Each other negative isolates one unsafe feature: body image, header, footer,
hyperlink, altChunk, external target, embedded font, wrong relationship
source, wrong MIME/root, unknown type namespace, duplicate singleton or an
external relationship originating in an otherwise recognized auxiliary part.
`unsupported-font-embedding.docx` contains an explicit opaque sentinel, not a
real embedded font; it tests rejection before font interpretation.

Historical filenames beginning `unsupported-theme-`/`unsupported-strict-theme-`
and the compatibility/default-tab settings cases are retained unchanged. They
now test editable normalization notices under the approved policy; their names
record the earlier strict profile rather than their current classification.

## Standards and decisions

- [WordprocessingML document structure](https://learn.microsoft.com/en-us/office/open-xml/word/structure-of-a-wordprocessingml-document):
  separate main content, style, font and settings parts.
- [Theme parts](https://learn.microsoft.com/en-us/office/open-xml/general/how-to-replace-the-theme-part-in-a-word-processing-document):
  themes supply fonts, colors and formatting. Approximations must be disclosed.
- [Document settings](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/19b9c0c5-65dd-491c-94d3-1487769154cf):
  settings can affect appearance and behavior; known formatting settings may be
  normalized, while content/security and unknown semantics remain guarded.
- [Official OOXML primer](https://download.microsoft.com/download/e/1/4/e14fb96f-83b8-4a2a-84db-7fa8acbe061a/Office%20Open%20XML%20Part%203%20-%20Primer.pdf):
  relationship roles, themes and script-specific properties require semantic checks.

Recognition requires the exact known relationship URI and source role, an
internal target, matching content type and XML root, and an unambiguous singleton.
All relationship parts remain traversed. Known formatting is projected into the
existing semantic model with bounded notices; originals remain independent.
No filename/suffix allowlist, new document model or additional dependency is
introduced by these corrections.

## Checked-in artifact inventory

SHA-256 identifies the actual bytes tested; it is not a provenance guarantee.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `duplicate-settings-relationship.docx` | 5597 | `66558b2794fdf14c0f4d8e56a8bf959c6d2e4321805298f8ea34f060f4c215d9` |
| `libreoffice-basic.docx` | 5033 | `24b64bd365cc317b8c7a9c73357fccba100fed4989dac4a7bab9d94ea13ea445` |
| `microsoft-sdk-hello-world.docx` | 13171 | `5fcdf446f45ab45906fb1f3728ac706cea78a3b47ea96fb4d365986327f6eb2a` |
| `microsoft-sdk-plain.docx` | 11546 | `6829665375dd098be560b424e2d6c7c31061dd4bca614c39bafcc92a30dc8c58` |
| `mismatched-auxiliary-mime.docx` | 5319 | `a6fdcfe2cb54a1a8d3c939ee837378c2014b708e3b8d9499d70db98f5867ecef` |
| `mismatched-auxiliary-root.docx` | 5272 | `aa264485752f76f95644fc68938418fc2e0128a5781b71e83dd44fc0bfb85f32` |
| `misplaced-main-app-properties.docx` | 5328 | `2da360d4e677e55865bef9cb967b8fa4a3ffe6a83e19070a6ce9210fda19d6d1` |
| `misplaced-root-font-table.docx` | 5314 | `bdfbb3e5972b683b73619cbb9f46f9f60d64311945fb6581a14f52e898aa1bf1` |
| `normalizable-wordweb-profile.docx` | 9383 | `1929030d11761c9ba77f8284146050fba6cac0540b61b293d2af3cf2215c4c7d` |
| `supported-authoring-metadata.docx` | 5423 | `a1cc78c62b40d67b18bd6086c3ac960da85ff14e693f49eff16479578898e927` |
| `supported-auxiliary-parts.docx` | 5311 | `fff8adf7e42aa8019ff2e991608cbcb25d1887180ee08d7a7a3a85db1cb5eac8` |
| `supported-root-thumbnail.docx` | 5536 | `6a819474a8b7e066c4f06f9115d793b3efdbc8a994b00906af263d375644a62d` |
| `supported-strict-authoring-metadata.docx` | 5388 | `2f765359e723aa969366d344b1a491eda4127cf8ec16ca679ffcadd9038064c6` |
| `supported-strict-auxiliary-parts.docx` | 5482 | `be1cd8ef4a987abb8b6cd281afe6343c74be8edda751c5b7e353e8f49c1cfeaf` |
| `unknown-theme-relationship.docx` | 5325 | `7e1e032e97947ad330ea2296398478b316876be63f7bed2a46b72f3df1226340` |
| `unsupported-active-document-protection.docx` | 9416 | `e81314fdf5df3a697d0b6927829caf649d7e8862f17b9bc3ef4eb53fafa51e8b` |
| `unsupported-active-track-revisions.docx` | 9382 | `25bb01dd4691efcf38139d39b7a2b855d3428995e14343d33da6c3902c779a16` |
| `unsupported-altchunk.docx` | 5511 | `eec6f0d34e6bd1142d777f0f2a7174e0bcde086f853e0d02f504570c5252b02f` |
| `unsupported-auxiliary-external-chain.docx` | 5651 | `8dd97b2616c3fadf0c9afdd0a641988f9ee363cf171a4e89f68e828086d0ae90` |
| `unsupported-external-relationship.docx` | 5339 | `f07c9f2abd04c7d09d3dedcde17362fa625202a7b2eb52a3116dfe39811e6a31` |
| `unsupported-font-embedding.docx` | 5907 | `f9b992142673e23258b66ede296ea59df4b7a68dfd2292b2e6fcc545859036b8` |
| `unsupported-footer.docx` | 5550 | `bc0a325ab4e0e77e93db035ba2f600cfb7d7475a8cb3988e511044a21970ae65` |
| `unsupported-header.docx` | 5554 | `8bc3e10623fc0ccc13a2fd104aed84b3e2827bbe6ddbebdddf7f6f3abc5e7ec6` |
| `unsupported-hyperlink.docx` | 5353 | `593b8bdd3249b68d9fbcb2dedb82206bbc9793c114e531829f655e23e8d2287e` |
| `unsupported-ignorable-extension.docx` | 5452 | `cb98c55ff376585d016a75ff768e16a08e8bed65773feb621419f7b0e3ad5fe0` |
| `unsupported-image.docx` | 5512 | `778df456191a6d65d7175d8085078ad3fb07a36651dac904a1c8f98a22678efe` |
| `unsupported-main-image-same-thumbnail.docx` | 5567 | `fcab0aed676cb314ec523625df5769fdd3f08d6a6acf5e47b121be1bc0fd0362` |
| `unsupported-malformed-default-session-id.docx` | 5427 | `788b870f52d6f49053376810469de55591b49691f8ad224a3077055459110c25` |
| `unsupported-malformed-session-id.docx` | 5422 | `125b440777b1aa88cbbbb593bf714e12d75596dd746dc09f9f5c7f71e961d218` |
| `unsupported-normalization-flood.docx` | 5757 | `993524d4f3e60e843a7880a634ceb54139fa6266fec80d95079c9fc3e91e5e7a` |
| `unsupported-revision.docx` | 5449 | `a71bcb165cbd2a94e4cef2bfdfd4b3f7292372b47d9cb661c5c0ddc7e247c578` |
| `unsupported-settings-compat.docx` | 5399 | `7eb53a4c773be4b9a9e1973727f84132a27859871150d39a14cca24ef6f7f09c` |
| `unsupported-settings-default-tabs.docx` | 5358 | `2b3c5db597c4d2455b3f201cca7aaf169f7db8b3beaccb93b947772cd0b18189` |
| `unsupported-strict-theme-color-reference.docx` | 5294 | `9f98907f211e1ee40b6063761f0e4dee653972f2375acf4e43c762d7a450fd89` |
| `unsupported-strict-theme-font-reference.docx` | 5286 | `87060b80cf7ac75ccef22b3aecc8f339384a396b4909f3b30ba16c8397968fb7` |
| `unsupported-strict-theme-style-reference.docx` | 5288 | `84de30342455bf2373cfee4e896fecea66e37b2b0188212eb95c360421e8b724` |
| `unsupported-theme-color-reference.docx` | 5344 | `f7cb5f5771a61c064757db5a5c9541aac30b028c7b1def1a3931d531c3323086` |
| `unsupported-theme-font-reference.docx` | 5336 | `45974793246b162f6b02a8fe8de25e44b1e351a34e08a96f2b1dee84cbec4103` |
| `unsupported-theme-style-reference.docx` | 5345 | `d390b586bba101e35acfd6ce2d5b67a086a3567ad75d7e6bcd17a6d61a3fb4c0` |
| `unsupported-unbound-ignorable.docx` | 5425 | `a4e2f649e1e635530ed1992e30f528830d9b315de439391db6d61e96731d5c73` |
