# Phase 2, work area 2: external document compatibility

This follows completed mobile selection (`2bccd2c`) on
`dev/writer-stability-phase2`. No merge, deployment, core schema change or
additional format is authorized by this work.

## Confirmed causes and boundaries

The November audit used LibreOfficeDev 26.8.0.0.alpha0 as an independent consumer
of public synthetic Writer exports. Its resaves exposed restrictions which
self-roundtrips did not reveal. The exact original packages remain the recovery
source; a complete supported text projection is required before editing.

DOCX `w:rtl` is active run formatting, not harmless metadata. Microsoft specifies
complex-script appearance and selected directional overrides. Writer can retain
logical Unicode text and use natural bidi, but cannot represent every forced
direction distinction. Valid run RTL therefore receives a disclosed formatting
approximation; direct complex-script font, bold, italic and size are preferred.
Inherited/mixed-script appearance may differ. No direction controls are inserted
into text. Paragraph bidi, hidden content, protection, revisions, numbering,
unknown properties and malformed structures remain guarded.

ODT producer defaults, configuration and unused declarations need classification
before they enter the blocking report. Known inactive settings can be recognized
without inventing content. Known unsupported formatting needs explicit notices.
Unknown settings, active notes/numbering, protection, linked or unsupported body
content must remain read-only. An ODF section's `style:editable` is not itself a
document protection flag; actual protection remains restrictive.

RTF language/proofing values, alternate font preferences, widow/hyphenation and
kerning controls affect formatting. Validate and disclose their approximation;
do not treat arbitrary control words as ignorable. Associated script fonts,
active direction, list/note destinations and encoding still need their existing
guards. In particular an undefined font reference must not become valid merely
because another producer emits it.

## Notice and export contract

Non-blocking notices have their own 128-entry budget, separate from unsafe-content
findings. A runtime meaningful-formatting summary is set before truncation.
Never post-filter a bounded blocker list: that could hide a later real content or
protection finding. This does not add fields to the persisted native document.

Full-file copy assessment reparses the retained original through the same adapter,
checks its MIME/format and complete editable outcome, and reports meaningful
formatting changes for external destinations. DOCX retains its source-aware copy
strategy. ODT/RTF copies regenerate supported content and formatting; native files
retain exact original bytes. Native reopening reproduces import notices. Exporting
a copy must not alter original bytes, current content, selection, history or the
local save association. Existing revision-bound confirmation remains required.

## Research

- [Microsoft OOXML RightToLeftText](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.righttolefttext?view=openxml-3.0.1)
  and [Word interoperability notes](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/aef3c9a6-5d6c-434b-90b7-85e761fd8e62):
  RTL selects complex-script appearance and is not equivalent to reordering text.
- [Unicode UAX #9](https://www.unicode.org/reports/tr9/): logical text storage and
  higher-level direction protocols differ from visual ordering.
- [ODF 1.3, part 3](https://docs.oasis-open.org/office/OpenDocument/v1.3/os/part3-schema/OpenDocument-v1.3-os-part3-schema.html),
  especially settings, outline/note configuration and section editability;
  [LibreOffice DocumentSettings](https://api.libreoffice.org/docs/idl/ref/servicecom_1_1sun_1_1star_1_1text_1_1DocumentSettings.html).
- [Microsoft RTF 1.9.1 specification](https://officeprotocoldoc.z19.web.core.windows.net/files/Archive_References/%5BMSFT-RTF%5D.pdf),
  linked by [Microsoft normative references](https://learn.microsoft.com/en-us/openspecs/exchange_server_protocols/ms-oxrtfex/3402a589-2578-4a15-99ce-04a91545f327).

Existing low-level ZIP/XML/RTF libraries are sufficient. No dependency, licensing
selection, document-model rewrite or new source-preservation container is needed.

## Implementation and regression files

- `office-docx/src/properties.rs` and `tests/rtl_normalization.rs`: valid active
  run RTL, order-independent direct complex-script appearance, inheritance/false
  overrides, logical Unicode preservation, exact originals and strict guards.
  Six new Rust regressions include the public actual LibreOffice resave.
- `office-formats/src/odt.rs`, `odt/properties.rs` and `odt/compatibility.rs`:
  exact inactive defaults, outline/note/sequence declarations, known typed
  application settings, one-column unprotected section styles and inspected
  auxiliary parts. Match role, manifest MIME and payload, not filenames alone.
  Known script/language/layout preferences receive explicit notices. Thirteen
  new ODT regressions cover the actual resave and guard boundaries.
- `office-formats/src/rtf.rs` and `tests/rtf.rs`: validated language IDs, exact
  alternate font labels, widow/hyphenation/line-number/kerning preferences and
  valid next-style references. Seven new regressions retain unsupported script,
  font, destination, encoding and content guards.
- `office-formats/src/lib.rs` and `tests/assessment.rs`: independent notices,
  pre-truncation summary and source-aware cross-format warnings. Five new tests
  include warning saturation, native reopening, invalid original metadata and
  ordinary warning-free sources. The export verification example now uses the
  complete-file assessment API too.
- `apps/writer/tests/phase2-compatibility-wasm.test.mjs` and
  `phase2-compatibility.spec.ts`: actual compiled engine and desktop/touch UI
  import, editing, formatting, native persistence, external copying/reopening
  and rejected-import preservation. `package.json` only adds the compiled test
  to the existing script; dependencies and lockfiles are unchanged.
- Public synthetic fixtures and provenance live in `tests/fixtures/docx/relationships`
  and `tests/fixtures/formats`. No private user document was added or published.

Review also corrected malformed ODT text being ignored in property/page-layout/
master-page containers, misplaced property children, unknown settings containers,
unresolved script-font references and unsigned acceptance of a signed ODF integer.
Each correction has a regression; ZIP/XML/CRC/DTD and resource checks remain.

## Executed verification

Frontend commands run from `apps/writer`, Cargo commands from the repository root,
with the documented stable Rust/WASM toolchain available on PATH.

| Command | Frozen-build result |
| --- | --- |
| `npm ci` | Pass, locked dependencies installed. |
| `cargo fmt --all -- --check` | Pass. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass. |
| `cargo test --workspace` | 650 passed, zero failed/ignored. Includes 31 new compatibility regressions. |
| `cargo build --workspace` | Pass. |
| `cargo check --locked --workspace --target wasm32-unknown-unknown` | Pass. |
| `npm run typecheck` | Pass. |
| `npm run lint` | Pass, zero warnings. |
| `npm test` | 61 passed, zero failed/skipped. |
| `npm run test:wasm` | 65 passed, zero failed/skipped; includes seven new compatibility cases. |
| `npm run test:build` | 2 passed, zero failed/skipped. |
| `npm run build` | Pass; Rust/WASM, TypeScript and production Vite assets. |
| `npm run test:browser -- tests/phase2-compatibility.spec.ts --project=chromium --output=/tmp/writer-phase2-compat-browser-final-results --reporter=list` | 8 passed, zero failed/skipped, desktop and touch. Actual production bundle and IndexedDB are used. |

These eight browser cases are targeted compatibility coverage, not a claim that
all 179 current Chromium cases have run at this stage. The preceding mobile
work area ran the then-complete 171-case suite; the overall final run follows the
remaining work areas. Targeted adapter/ABI cases overlap the suite totals above
and are not added again. Intermediate ODT tests incorrectly required equal
external run splits and direct underline values rather than inherited effective
formatting. They now compare text and effective formatting while exact native
structure remains required. Research or unexecuted checks are not test passes.

Independent final consumer probes passed using LibreOfficeDev
26.8.0.0.alpha0, commit `2c87e51eeaa2b413ff4ae097b2705eea1995d8e5`.
It consumed the three Rust-edited exports into fresh ODT outputs. Exact complete
Unicode paragraphs matched (DOCX six, ODT six, narrow RTF two), and the edited
Latin marker retained bold, italic, underline, Tinos and 14pt. The generator also
checked exact originals, native identity/bytes/reopening, warning confirmation and
external reopening; the complex LibreOffice RTF remained safely rejected.

Executed independent commands, all exit 0:

```sh
node /tmp/writer-phase2-independent-generate.mjs /workspace/stuff/tests/fixtures/formats/rtf-known-formatting.rtf
soffice -env:UserInstallation=file:///tmp/writer-phase2-independent/profile-final-docx-20261115 --headless --convert-to odt --outdir /tmp/writer-phase2-independent/lo-final-docx /tmp/writer-phase2-independent/phase2-docx.docx
soffice -env:UserInstallation=file:///tmp/writer-phase2-independent/profile-final-odt-20261115 --headless --convert-to odt --outdir /tmp/writer-phase2-independent/lo-final-odt /tmp/writer-phase2-independent/phase2-odt.odt
soffice -env:UserInstallation=file:///tmp/writer-phase2-independent/profile-final-rtf-20261115 --headless --convert-to odt --outdir /tmp/writer-phase2-independent/lo-final-rtf /tmp/writer-phase2-independent/phase2-rtf.rtf
python3 /tmp/writer-phase2-independent-verify-final.py
```

The temporary probe scripts/artifacts contain only public synthetic fixtures.
Their logs are `/tmp/writer-phase2-independent-generate-final.log`,
`writer-phase2-independent-soffice-final-{docx,odt,rtf}.log` and
`writer-phase2-independent-verify-final.log`. These are independent probes, not
additional suite passes, pixel comparisons or general interoperability proof.

## Remaining limits and risks

This enables the actual audited LibreOffice DOCX/ODT resaves and a deliberately
bounded known-formatting RTF fixture. It does not make arbitrary external documents
editable. The older `libreoffice-basic.odt` still contains unsupported hyphenation,
font, page-layout and settings variants. The complex LibreOffice RTF resave still
contains associated script fonts, active RTL and unsupported destinations; one
older fixture also references an undefined font. Those sources remain read-only.
Authored unsupported ODT metadata, headings with outline semantics, active notes,
lists, tables, headers and linked content remain guarded.

Formatting normalization is intentionally approximate, including inherited/mixed
script direction, typography preferences and some producer layout settings.
Notices may conservatively include preferences in style definitions. This is not
Word pagination equality or a full OOXML/ODF/RTF schema implementation. Native
files retain exact originals; regenerated ODT/RTF copies contain supported current
content and formatting with a brief warning for meaningful approximations.

No physical iOS/Android/PWA, Microsoft Word runtime, Windows/macOS host or available
Firefox/WebKit result is established by these Chromium/LibreOffice checks. Hosted
CI restoration remains a later work area. No merge, deployment or integration
protocol change has occurred.
