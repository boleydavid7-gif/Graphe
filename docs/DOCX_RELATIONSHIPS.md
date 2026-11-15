# DOCX ancillary relationship correction

This records the first relationship correction. The owner's subsequent
[formatting-normalization policy](DOCX_NORMALIZATION.md) supersedes the historical
style/settings read-only limits and producer outcomes below. Both corrections
remain in the same review PR; earlier results are retained as audit evidence.

The original semantic graph accepted the main document, paragraph styles and
core properties only. It consequently classified ordinary metadata, font-table,
unused-theme and preference relationships as unsupported document content, then
also reported their targets as unsupported parts. A plain supported body could
therefore become read-only solely because of producer boilerplate.

This correction recognizes specific ancillary roles. It does not make every
settings/theme/font part harmless or relax the supported body-format profile.
The affected user's original file was unavailable during this work; this does
not establish that the reported document is now editable.

## Relationship source and target policy

The source is derived from the relationship part, not a target filename. Package
root means `_rels/.rels`; main means the discovered main document, including a
relocated main part. Targets must be internal and resolve through the existing
validated OPC inventory. Recognition requires an exact known relationship URI,
the correct source role, an allowed content type and, for interpreted XML parts,
the expected root.
Duplicate singleton roles and MIME/root mismatches are errors, not repairs.

| Relationship role | Allowed source | Target interpretation |
| --- | --- | --- |
| `officeDocument` | Package root | Existing validated Word main document |
| Package `metadata/core-properties` | Package root | Title plus recognized descriptive/audit metadata |
| `extended-properties` / Strict `extendedProperties` | Package root | Standard application/count/company metadata |
| `custom-properties` / Strict `customProperties` | Package root | Known typed custom-property metadata |
| Package `metadata/thumbnail` | Package root | Opaque cached preview; never an editable image |
| `styles` | Main document | Existing paragraph-style/default semantic parser |
| `theme` | Main document | Unused theme retained outside the model |
| `fontTable` | Main document | Validated basic font descriptors |
| `settings` | Main document | Named inert preference/session subset only |
| `webSettings` | Main document | Named browser-save preferences only |
| Other types or source roles | None added | Remain unsupported and preserved read-only |

Office relationship prefixes are the Transitional
`http://schemas.openxmlformats.org/officeDocument/2006/relationships/` and Strict
`http://purl.oclc.org/ooxml/officeDocument/relationships/`. Strict metadata uses
camel-case `extendedProperties` and `customProperties`, not the Transitional
hyphenated suffixes. Package core/thumbnail relationships use their exact known
`http://schemas.openxmlformats.org/package/2006/relationships/metadata/` URIs.
The same target can be safe as a root preview and unsupported as a body image.

In both unmodified Microsoft SDK Word fixtures and the LibreOffice output,
`_rels/.rels` points to `docProps/app.xml` (extended properties),
`docProps/core.xml` (core properties), and `word/document.xml` (main content).
Their main relationships point to `styles.xml`, `settings.xml`, `fontTable.xml`
and `theme/theme1.xml`; both Word fixtures also have `webSettings.xml`.
These are the actual targets inspected, not hypothetical filename exceptions.

## Payload acceptance is narrower than relationship recognition

- **Metadata:** Core title remains part of the shared model. Recognized creator,
  dates, revision and other descriptive fields, extended properties and standard
  typed custom values remain in the exact source. Nested document markup or
  unknown property constructs are not silently accepted as ordinary metadata.
  Custom properties do not enable `DOCPROPERTY` or other editable fields.
- **Theme:** The bounded XML must have a known DrawingML theme root. Accepting an
  unused part does not resolve theme fonts, colors, tints or shades. Existing
  body, style and document-default checks still reject active theme-dependent
  properties. Relationships originating in the theme remain subject to the graph.
- **Fonts:** Supported descriptors include name, charset, family, pitch, signature,
  Panose and `notTrueType`. Alternate-name substitution, embedded fonts and unknown
  font semantics remain unsupported. No source font is loaded or executed, and
  Writer's pinned font catalog and substitution diagnostics remain authoritative.
- **Settings:** Empty settings and recognized zoom/view, proofing, revision-save
  IDs, style-pane hints and a small set of display/preview flags may be retained
  as inert preferences. Compatibility rules, default tab stops, hyphenation,
  theme mappings, protection, revision tracking and unknown settings remain
  unsupported. Settings can change layout and behavior; their filename alone
  is never grounds to ignore them.
- **Web settings:** Known flags such as `allowPNG`, `optimizeForBrowser` and basic
  browser-save preferences do not supply editable body content. Framesets,
  frames, unknown attributes and other unsupported constructs remain blocked.
- **Thumbnails:** JPEG, PNG, GIF, TIFF, BMP, EMF/WMF variants and SVG MIME types are
  accepted only for the exact package-root preview role. Payloads are opaque:
  Writer does not render, decode or fetch their contents. Existing package/XML
  safety checks still apply. This does not introduce document image support.

Recognized eight-hex-digit authoring IDs (`w:rsidR`, `w:rsidRDefault`, `w:rsidRPr`, `w:rsidP`,
`w:rsidSect`, `w:rsidDel`) and resolved `mc:Ignorable` declarations are also
recognized as metadata. An ignorable declaration is not
permission to discard an actual extension, unknown formatting or revision node.
Unknown relationships now identify their bounded ID, Type and Target in the
diagnostic, so a remaining refusal can be traced to the actual edge.

This is a bounded compatibility profile, not full OOXML schema validation.
Metadata scalars receive structural/basic value checks rather than every schema
facet (for example, integer subtype ranges and date lexical forms). Opaque unused
themes and previews are retained, not certified or interpreted.

## Original recovery and current-content export

An editable import still requires a complete supported body/style projection and
durable preservation of the exact original before browser editing activates.
The original stays outside document state and undo snapshots; native `.writer`
save/reopen retains it byte-for-byte. Inert source-only metadata is not copied
into a competing document model.

Current-content DOCX export regenerates the supported edited model. It does not
pass through all original settings, metadata, thumbnails or unsupported features.
Save As must continue to disclose conversion losses and recommend a native copy.
Original download is separate and can contain text removed from the edited model;
sharing a source-bearing native file carries the same disclosure.

Unsupported semantic content still yields an exact read-only source without a
partial editable copy. Corrupt packages, ambiguous relationships, wrong typed
roots and malformed supported values fail explicitly. External targets are never
fetched; external chains from recognized ancillary parts remain unsupported.

## Producer fixtures and remaining boundaries

[Fixture provenance](../tests/fixtures/docx/relationships/README.md) distinguishes
synthetic boundary packages from unmodified Microsoft SDK Word-producer originals
and a genuinely generated LibreOffice document. Upstream fixture licensing and
hashes are retained there. Microsoft Word itself was not run in this environment.

The genuine Word/LibreOffice fixtures retain style and settings constructs beyond
the editable profile and are expected to remain read-only for those reasons.
Removing false boilerplate-edge diagnostics does not prove full Word fidelity.
Character/table styles, theme resolution, script-specific formatting, active
layout settings, tables, body images, headers/footers, links and fields retain
their existing limitations. No unused-style pruning or blanket extension bypass
is part of this correction. No new dependency or shared-model change is needed.

## Primary engineering references

- [Microsoft document structure](https://learn.microsoft.com/en-us/office/open-xml/word/structure-of-a-wordprocessingml-document)
  and [official SDK part contracts](https://github.com/dotnet/Open-XML-SDK/tree/main/data/parts).
- [Document settings](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.settings)
  and [theme font languages](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.themefontlanguages?view=openxml-3.0.1).
- [Font alternate names](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.altname?view=openxml-3.0.1)
  explain why font-table data is not universally inert.
- [DOCPROPERTY semantics](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/fc55f49e-2ca4-4366-8e35-bd8b228155a8)
  explain the boundary between custom metadata and active fields.
- [Apache POI relationship constants](https://poi.apache.org/apidocs/dev/constant-values.html)
  provide an established implementation reference for Strict metadata URI spelling.

## Verification (Linux, 2026-11-15)

Commands from the repository root unless prefixed with `apps/writer` below:

| Command | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --workspace` | 536 passed; zero failed/ignored |
| `cargo build --workspace` | Passed |
| `cargo check --locked --workspace --target wasm32-unknown-unknown` | Passed |
| `node apps/writer/tests/layout-check-native.mjs` | Passed; shared geometry/outline golden unchanged |
| `apps/writer`: `npm run typecheck` | Passed |
| `apps/writer`: `npm run lint` | Passed |
| `apps/writer`: `npm test` | 42 passed |
| `apps/writer`: `npm run test:build` | 2 passed |
| `apps/writer`: `npm run test:wasm` | 48 passed against compiled release WASM |
| `apps/writer`: `npm run build` | Passed, including release WASM, TypeScript and Vite |
| `apps/writer`: `npm run test:browser -- --project=chromium` | 78 passed against production assets |
| `apps/writer`: `npm run test:browser -- tests/docx-relationships.spec.ts --project=chromium` | 2 passed against the final release bundle |
| `apps/writer`: `npm run test:browser -- tests/docx-relationships.spec.ts --project=firefox --project=webkit` | Four launch failures: browser executables missing; no interaction assertions executed |

The full Chromium suite preceded the final `rsidRDefault` acceptance rule; the
focused relationship tests passed again on the final bundle, with the positive
browser workflow using Strict authoring metadata. Native tests cover all 36
fixtures, including unchanged Word-producer and LibreOffice files. The release
WASM tests exercise actual native/WASM layout and PDF parity as well as DOCX;
they do not substitute JavaScript importer mocks.

Initial subprocess-based checks were blocked by the execution sandbox and were
rerun with approved process access. An initial WASM test invocation omitted Cargo
from `PATH`; its two native-comparison subprocess failures passed after correcting
the environment. These environmental attempts are not counted as passing tests.

GitHub Actions did not execute steps on the merged baseline
([run 43654606485](https://github.com/boleydavid7-gif/stuff/actions/runs/43654606485));
all four jobs failed before runner allocation. Current PR CI status must be checked
separately; local success is not a claim that CI or Windows/macOS checks passed.
Firefox/WebKit, current Microsoft Word application execution and the unavailable
user document remain unverified. No merge or deployment of this correction is
authorized by its review PR.
