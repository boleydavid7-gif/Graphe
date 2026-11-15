# Writer engineering research

Research date: **2026-11-14**. The initial record preceded Step 4.5; later
reviews are appended with their milestone context. Earlier limitations record
what was true at that stage. The conversion review below supersedes 5A-only
conversion statements. Research alone is not a compatibility or completion claim.

## DOCX relationship review (2026-11-15)

The owner subsequently authorized [safe formatting normalization](DOCX_NORMALIZATION.md).
The supplied private Word-for-the-web file exposed 82 metadata/formatting-only
blockers despite having ordinary paragraph content and disabled tracking.
Research separates gallery/unused-style data and known appearance preferences
from actual document content, revision records and protection. The accepted
adapter policy uses explicit validated normalizations and visible notices,
preserving exact originals without changing the core/native schema. Independent
review covers character-style precedence, active numbering inheritance, report
caps, import atomicity and allocation amplification. Sources, evidence, tests and
limitations are recorded in the linked report; the private text is not checked in.

The focused [relationship investigation](DOCX_RELATIONSHIPS.md) compares ordinary
Word and LibreOffice package relationships with the previous semantic graph
whitelist. It rejected extended/custom metadata, themes, font tables and settings
without inspecting their role or content. This caused false read-only findings.

Primary evidence includes Microsoft's WordprocessingML structure, theme/settings
documentation and pinned official Open XML SDK fixtures; see the report for links
and fixture provenance. No new parser, dependency or shared model was needed.
The correction checks exact URI/source role, content type, root, singleton rules
and a named inert payload subset. Every child relationship is still inspected.
Active settings, unresolved theme formatting and embedded fonts remain guarded.
Unknown extension markup is not discarded under `mc:Ignorable`.

Independent code review found no critical integrity hole in this scoped policy.
This is not complete OOXML schema validation: metadata scalars receive bounded
structural checks, and unused themes/thumbnails remain opaque original data.
Important remaining profile limits are recorded rather than addressed by a
style/theme rewrite. Native source retention, failed-import atomicity and
regenerated-export disclosure have dedicated native/WASM/browser regressions.
The user's specific DOCX was unavailable; actual Word execution is also
unavailable. Unmodified Word-producer fixtures and an actual LibreOffice-generated
file establish the reported relationship behavior, not general Word fidelity.

## Questions, evidence, and accepted approach

| Question | Sources and approaches considered | Step 4.5 choice and tradeoff |
| --- | --- | --- |
| Browser editing surface and input | [Input Events 2][input], [MDN beforeinput][beforeinput], [EditContext][editcontext], [ProseMirror view][pm-view] | Semantic `contenteditable` renders Rust snapshots. Cancel supported `beforeinput` edits and issue Rust transactions; reconcile or restore unhandled `input` changes. Browser navigation remains useful. Canvas/hidden textarea requires substantially more selection, accessibility, and IME infrastructure. EditContext has limited availability. Deprecated `execCommand` and another editor engine are excluded. |
| Rust/WebAssembly boundary | [wasm-bindgen deployment][wasm-deploy], [CLI][wasm-cli], [Vite assets][vite-assets], [Vite WASM][vite-wasm] | Add one small binding crate; keep `office-core` browser-independent. Export commands, snapshots, selections, and structured errors. Build WASM before Vite and verify production asset loading. Binding DTOs are transport, not a second model. |
| Unicode and selection mapping | [DOM CharacterData][dom], [Selection direction][selection], [Unicode 17 UAX 29][unicode17], [latest UAX 29][unicode-latest] | DOM text offsets are UTF-16 code units; element offsets index child nodes. Map through stable paragraph IDs and Rust conversions across all runs. Preserve anchor/focus direction. Rust's pinned Unicode 17 segmentation is authoritative; Unicode 18 exists but is not an incidental upgrade. Never use JavaScript length as a grapheme count or a competing browser segmenter. |
| IME and international input | [Input Events composition ordering][composition-order], [MDN compositionend][compositionend], [isComposing][iscomposing] | Permit transient composition DOM while preserving the saved Rust origin/selection; do not reconcile its active nodes mid-composition. Commit the final replacement once through Rust. Cancellation or empty completion must not become an accidental deletion. Reconcile final browser event ordering without committing twice. |
| Mixed formatting and text runs | [OOXML paragraphs][ooxml-paragraphs], [OOXML bold toggle][ooxml-bold], [LibreOffice Writer core][libreoffice] | Render inherited styles plus direct properties using a Rust-defined effective display policy. Retain optional source properties and existing run boundaries. Browser CSS is a prototype display adapter, not OOXML toggle evaluation or platform-independent layout. |
| Undo/redo consistency | [ProseMirror state/transactions][pm-state], [deprecated execCommand][execcommand], existing `DocumentSession` | All mutations commit through Rust; intercept browser history commands. One composition or multiline paste is one transaction. Keep existing bounded exact snapshots; no JavaScript undo stack. Whole-document copies are understood costs, not grounds for an unapproved rewrite. |
| Browser-local storage | [IndexedDB 3][idb], [transaction completion][idb-complete], [storage quotas][quotas] | Save exact native `.writer` bytes in IndexedDB. Compare the expected record token and put in one read/write transaction; resolve only on transaction completion. Explicit save/open first. Preserve the live document and dirty state on failure; clear dirty only if the saved revision still matches. Browser storage can be evicted and export remains important. |
| Rendering and updates | [ProseMirror view][pm-view], [LibreOffice architecture][libreoffice], [ONLYOFFICE architecture][onlyoffice] | Reconcile changed paragraph/run presentation while keeping Rust authoritative. Retain unchanged DOM where practical and avoid unnecessary selection resets. Measure typing before introducing caches, persistent trees, workers, or custom layout. Existing Office architectures inform boundaries; their complete engines are not embedded. |
| Accessibility and keyboard use | [ARIA textbox][textbox], [APG toolbar][toolbar], [keyboard interface][keyboard], [WCAG focus][focus], [status messages][status] | Labeled multiline textbox, semantic controls, visible focus, predictable Tab behavior, selection-aware formatting states, polite save status and actionable errors. A toolbar role requires its keyboard pattern; a labeled group of ordinary controls is preferable to incomplete toolbar semantics. No blanket application role. |

`getTargetRanges()` supplies browser edit targets where available, but returns no
ranges for history commands; it does not replace engine validation. Pointer and
native keyboard navigation update the Rust selection through mapped DOM ranges.
Restore the DOM selection only when needed, retaining anchor/focus direction.
If browser endpoints fall inside an engine grapheme, the adapter must apply its
documented affinity policy through Rust rather than silently weakening the core
strict coordinate conversions.

Plain-text paste reads `text/plain`, prevents native rich HTML insertion, and
uses Rust paragraph operations for line breaks. CR/LF policy belongs to this
explicit command; the existing low-level text operation continues preserving
literal UTF-8. Native IDs remain opaque: look up DOM elements in a map, never
interpolate imported IDs into CSS selectors. Session `u64` revision numbers cross
the JavaScript boundary as decimal strings to avoid number precision loss.

## Retrospective review before implementation

Reviewed the schema-1 model and validator, `coordinates.rs`, `position.rs`,
transactions, editing, history, native serialization/filesystem persistence,
public APIs, fixtures, and relevant tests against the sources above. Stable IDs,
paragraph-wide graphemes, strict decoding, validated atomic publication, and
separate byte/filesystem persistence are suitable foundations. Passing tests
alone does not establish full Word compatibility or browser correctness.

**Critical existing defects:** none identified by this review. No correction to
old code is claimed here. Critical integration risks require implementation and
regression coverage: UTF-16/grapheme confusion; new graphemes formed across an
insertion/deletion boundary; stale selections; DOM reconciliation during IME;
duplicate composition commits; competing browser history; unchecked imported
IDs; lossy revision transport; and storage completion/conflict handling.

**Accepted targeted additions:** paragraph splitting is missing and is necessary
for Enter and multiline paste. Add it through the existing transaction boundary,
preserving identities and source formatting deliberately. Compute post-edit
caret affinity in Rust because inserted text can merge adjacent graphemes;
adding JavaScript string length to an old offset is incorrect. Define effective
display formatting in Rust without replacing the optional source properties.
Opening a valid document with no paragraphs must not silently add content;
materialize an initial paragraph only through a deliberate edit transaction.
Keep cross-section text replacement explicitly unsupported until semantics exist.

**Important future changes requiring a separate decision/approval:** document
copies, full validation, and full snapshots can limit large-document performance;
measure before structural sharing, incremental validation, or history redesign.
OOXML style toggles, themes, defaults, unsupported-element preservation, and
precise import/export mappings remain incomplete. Native persistence's documented
filesystem guarantees remain platform-dependent; browser storage does not inherit
native rename/durability guarantees. None is a reason to rewrite working APIs in
this milestone. Prototype input/import resource bounds must be documented and
tested; they are application limits, not silent native-format changes.

**Optional:** EditContext, rich HTML clipboard import, mapped/rebased positions,
typing-history coalescing, coordinate caches, custom visual bidi movement, and
advanced typography/layout. These are not implemented merely because researched.

## Dependencies, licensing, and deployment boundary

Evaluate mature low-level libraries first; do not embed ProseMirror, LibreOffice,
ONLYOFFICE, or another complete editor. ProseMirror's transaction/view design is
studied as engineering evidence. ONLYOFFICE's SDK is [AGPL-3.0][onlyoffice-license]
and is studied only; no source is copied or linked.

Proposed interface/build dependencies use established upstream projects:
[React MIT][react-license], [Vite MIT][vite-license],
[TypeScript Apache-2.0][ts-license], [Playwright Apache-2.0][pw-license], and
[wasm-bindgen MIT OR Apache-2.0][wasm-license]. Review exact resolved versions,
transitive licenses, maintenance, security advisories, and target support when
adding lockfiles. These licenses do not select this project's pending license.
Use host [cryptographic UUID][uuid] namespaces plus a checked Rust ID counter;
this does not require a random library in the core model. Match the
`wasm-bindgen` CLI version to the locked binding dependency. Start with JSON
snapshots and the native IndexedDB API rather than additional bridge/storage
wrappers.

Cloudflare preparation is an inert example only: `cloudflare.config.ts.example`
uses `cf/config` for an assets-only Worker, SPA fallback, no Worker entrypoint,
no explicit `assets.directory`, no domain attachment, and disabled workers.dev
and preview URLs. No CLI installation, live settings, deployment, DNS, or access
restriction changes are authorized by implementation. See [Static Assets][cf-assets]
and the current [Cloudflare configuration][cf-config] documentation before use.

## Verification requirements and remaining limits

Add Rust regressions for paragraph splits, inherited display formatting, opaque
IDs, revision transport, and caret affinity across combining/emoji boundaries.
Real-browser tests must exercise WASM initialization and production assets,
typing/deletion, native arrows and directional selections, formatting, undo/redo,
multiline paste, failed edits, native bytes, IndexedDB conflicts/failures, and
corrupt imports. Test unhandled/noncancellable `input` recovery explicitly.

Chromium [IME protocol events][cdp-ime] can exercise composition event paths;
synthetic composition is not proof of real operating-system candidate-window,
dictation, or every-language IME support. Record manual IME testing limits.
Automated semantic/keyboard checks do not certify NVDA, JAWS, VoiceOver, touch, or
advanced rich-text accessibility; manual assistive-technology review remains.
Browser font metrics are not professional page layout or Word pagination.
Performance measurements and actual verification results belong in the milestone
report; this pre-implementation record makes no unperformed success claims.

## Implementation review and regression corrections

The original engine review found no confirmed critical defect. Review of the new
integration subsequently found and corrected these issues before delivery:

| Classification | Finding | Targeted correction and regression |
| --- | --- | --- |
| Critical | A missing browser selection could materialize a paragraph over existing section content. | Materialization refuses any document containing paragraphs. Tests cover multiple paragraphs and an empty first section followed by content. |
| Critical | Native arrows can land inside a grapheme split across formatted spans. Always snapping forward traps backward movement; always expanding ranges traps Shift-selection contraction. | Rust accepts explicit leading/trailing focus affinity for navigation. DOM logical movement supplies direction; Rust remains the only segmenter. Native arrows and shrinking directed selections are browser regressions. |
| Critical | Forced recovery reused an empty-document placeholder containing transient browser text, leaving visible text absent from the engine. | Forced recovery restores the exact empty placeholder and canonical spans/styles. Browser tests cover unhandled input and canceled composition; exported empty structure remains unchanged. |
| Important, corrected | Typing during an asynchronous open could be discarded; a successful save of an earlier snapshot could mark newer work clean. | Block editing during replacement, recheck session/revision before publication, and clear dirty only for the captured saved revision. In-flight save behavior is tested in Chromium. |
| Important, corrected | Corrupt storage timestamps could crash the document-library render; record identity could disagree with its native bytes. | Validate the JavaScript Date range; Rust checks expected stored document ID before replacing a session. Unit/native regressions preserve the original work on failure. |
| Important, corrected | Deep style chains, many empty runs/sections, or newline-heavy paste can exhaust the prototype despite a byte cap. | Apply documented adapter limits to import, candidates, and export; preflight paragraph and text growth before staging paste. Core format/schema rules remain unchanged. |

The browser limits are 5 MiB of native bytes, 200,000 UTF-16 units, 2,000
paragraphs, 2,000 sections, 20,000 runs, 128 styles, and style depth 32. Retain 20
undo snapshots. Limits are explicit rejection boundaries, never truncation.
Collapsed typing formatting is tied to its Rust selection; a supplied new DOM
location cannot accidentally reuse an old caret's formatting. Forced DOM recovery
also drops transient formatting instead of adopting it as document state.

No unresolved critical finding remains after the regression gate. Important
future recommendations still require a separate architectural decision: measure
and budget large-document history before structural sharing/incremental
validation; define complete Word style and unsupported-content semantics before
DOCX import. Optional rich clipboard input and advanced accessibility remain
deferred. See [verification](VERIFICATION.md) and [dependency review](DEPENDENCIES.md)
for actual checks, measurements, resolved libraries, and security review.

## Pre-merge follow-up

The independent pre-merge review found a critical clipboard fidelity defect
missed by the initial suite: DOM-selected paragraphs copied two newlines, and
the Rust plaintext-paste policy interpreted them as an extra paragraph. The
correction reads exact selected text through core Unicode coordinates in Rust,
with one structural LF, and performs cut only after successful clipboard writing.
Native, direct compiled-WASM, and actual Ctrl+C/X/V regressions cover this.

Important targeted corrections isolate damaged library records with explicit
warnings, put save failures inside the active unsaved modal, and improve measured
label/focus contrast. No new dependency or major architectural layer was needed.
The expanded tests cover full native-fixture saving, fresh browser-process
restart, startup failure/retry, physical mouse selection, unsaved actions and
reload cancellation. A separate Node/V8 suite runs actual release WASM bindings;
it does not substitute for browser testing or claim a wasm-bindgen-test harness.

See [PRE_MERGE_REVIEW.md](PRE_MERGE_REVIEW.md) for every requirement, actual rerun
results, and remaining untested platform/IME/accessibility/durability scopes.
Clean browser restart is verified separately from crash or power-loss recovery;
injected quota failures are not a test of real storage exhaustion.

## Hosted build toolchain correction

The supplied Cloudflare build log reaches `npm run build` then fails with
`spawnSync wasm-bindgen ENOENT`; dependency installation succeeds on Node 24.
Official build-image documentation does not promise Rust tooling. An explicit
build-only bootstrap installs stable Rust, its WASM target, and the exact
wasm-bindgen CLI using an official prebuilt asset and pinned SHA-256, then invokes
the existing build in the same environment. This avoids repeatedly compiling
the binding CLI's large dependency graph and avoids ineffective npm prebuild
PATH exports. Existing local builds and the independent core are unchanged.

The cold environment test passed without any Rust/WASM tools on PATH or installed
in the test Cargo/Rustup homes. Two orchestration regressions cover checksum
rejection before executable installation and correct environment propagation.
[CLOUDFLARE_BUILD.md](CLOUDFLARE_BUILD.md) records sources, exact setup, and limits.
Live Worker/build reads are denied by the current connection, and Access reports
it is not enabled; no production/preview endpoint is assumed private from an
inert configuration example or an outbound-proxy 403. No deployment is claimed.

## Step 5A — DOCX adapter and preservation review

The user authorized Step 5 and selected **preserve unsupported originals before
editing**, then required a format-specific adapter around the shared model.
[DOCX.md](DOCX.md) records the questions, primary sources, dependency evaluation,
limits and independently testable import/export sequence. Decisions 029–030
record the accepted boundary; no additional formats are implemented.

Reviewed the existing model/validator, Unicode positions, transactions, editing,
history, schema/native serialization, native storage, browser bindings and tests
before adding code. All baseline engine checks passed (191 tests including
doctests), as did existing TypeScript, lint, storage, compiled-WASM and hosted
build tests. No confirmed critical defect in those existing foundations blocks
read-only package inspection.

- **Critical adapter corrections, addressed with regressions:** bound ZIP entry
  indexing before library allocation; reject ambiguous end-record signatures in
  comments and ZIP64 local headers; verify CRC/observed expansion, header and
  streamed data-descriptor consistency, including descriptor overlaps. Bound XML
  attributes/namespaces/raw DOM capacity markers before
  parsing and MIME name lengths before repeated inventory copies. Reject invalid
  scalar references and invalid namespace bindings that
  the selected DOM library otherwise accepts. Correct the Strict root
  `officeDocument` relationship URI and resolve canonical package-absolute
  internal targets. These are corrections in the new adapter, not changes to
  native document semantics.
- **Important, later semantic-import gates:** core sections cannot yet retain
  page properties/headers; Word defaults/themes/script fonts and style toggles
  need explicit mappings; tables, fields, lists, revisions, links and unknown
  markup cannot be projected losslessly. Native IDs, empty containers, CR and
  XML-illegal controls also require export policies. Do not weaken core
  validation or embed DOCX structures to bypass these constraints. Editable
  projection/source association, persistence and edit-aware export need a
  reviewed design before enabling edits. The namespace-family result is not
  OOXML schema conformance. External URI inventory is not a fetch-security API.
- **Optional:** later fuzzing/corpus expansion and streaming optimizations after
  measuring this bounded profile. No speculative registry or format hierarchy.

The new byte API retains the complete immutable original, including unsupported
semantic parts; it neither edits nor reconstructs a document. Handcrafted
Transitional and relocated Strict fixtures exercise this contract. Word and
LibreOffice interoperability, broader encodings/URI profiles, full normative
PDF review and live browser DOCX integration remain untested. See
[DOCX.md](DOCX.md) for advisory-check limitations and
[DOCX_VERIFICATION.md](DOCX_VERIFICATION.md) for actual commands/results.

The corrected Strict URI is independently documented by
[Microsoft's ISO 29500 implementation note](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/32be961f-d71a-4812-913f-b675c79aa88a)
and [LibreOffice's package detector](https://docs.libreoffice.org/oox/html/filterdetect_8cxx_source.html).

## DOCX conversion completion before Step 6

Inspection of main `a504e2f` found package inspection/preservation only: no semantic
DOCX import/export, no imported page geometry, and schema-1 section properties
were empty. The owner explicitly chose to complete this prerequisite before
Step 6. Work is on `dev/docx-conversion`; no merge, deployment or typography
implementation is authorized by this work. The accepted scope and primary
standards sources are in [DOCX_CONVERSION.md](DOCX_CONVERSION.md).

Reviewed the model, strict loaders, transactions/history, DOCX package APIs,
WASM presentation and browser import/storage flow. The chosen implementation
adds generic schema-2 geometry/page-break-before and an explicit schema-1
migration. Native format 2 retains a generic immutable original outside the
editable model/history. Existing ZIP/XML, Serde, filesystem and IndexedDB
infrastructure is reused; a maintained base64 codec supplies bounded source
encoding. Decisions 031–033 record the accepted changes.

- **Critical corrections:** source-bearing native input must not pass through a
  document-only loader and discard its original. Browser imports use a detached
  candidate and persist the complete file before activation; storage failures or
  stale completion cannot replace live work. Resource checks cover both reported
  input length and actual retained bytes after custom ownership conversion.
- **Critical semantic corrections:** independent LibreOffice 26.8 reference
  checks exposed differences between Word text elements and structural breaks/
  tabs, including literal CR/LF/TAB and ambiguous edge whitespace. Such source
  spellings now remain preserved read-only instead of producing an approximation.
  Export uses the tested explicit controls/character-reference policy. Supported
  style toggles are translated contextually, separate from direct Boolean values.
- **Important profile limits:** script-specific font families, sizes and bold/
  italic values must fit the single native property set. Themes, unsupported
  properties/metadata, tables, fields, images, links, revisions and other omitted
  semantics prevent editable import. Native IDs, exact run partitions and future
  style-editing intent are not universal DOCX round-trip guarantees. Negative
  Word margin-overlap semantics remain unsupported.
- **Important later work:** accurate fonts, text measurement, shaping, bidi,
  line wrapping and pagination remain Step 6. Existing CSS display and fixed/
  minimum line-height projection are not authoritative layout. Optional broader
  interoperability/corpus/fuzzing work must retain the same preservation rules.

Supported imports retain original bytes through native save/reopen and edits.
Unsupported sources remain available in the read-only dialog but are not saved
in the local library. Source-bearing native exports disclose that their backup
may include deleted text; current-content Word export and original download are
separate actions. LibreOffice reference checks cover only their tested subset;
Microsoft Word application interoperability and Word pagination are untested.
Final executed commands/results, including WASM/browser regressions, belong in
[DOCX_CONVERSION_VERIFICATION.md](DOCX_CONVERSION_VERIFICATION.md).

## Retained Step 6 typography research

Established Rust stacks were evaluated for shaping/metrics, font fallback,
Unicode line breaking/bidi and native/WASM compatibility. Parley 0.13 with
HarfRust/Skrifa is a promising low-level candidate with explicitly registered,
pinned font bytes and no host-font discovery. Rustybuzz/ttf-parser were rejected
because current RustSec records mark them unmaintained. The isolated
`/tmp/writer-layout-probe` ran four native tests and a WASM compile; that is not
an implemented engine, browser-runtime parity evidence or Word-pagination test.
No typography dependency or font asset has been added. Reassess the final stack,
font licensing/availability, cluster-to-position mapping, checked units/rounding
and browser glyph rendering when Step 6 resumes.

Primary sources: [Parley 0.13](https://github.com/linebender/parley/tree/v0.13.0),
[HarfRust](https://github.com/harfbuzz/harfrust/blob/main/HARFBUZZ.md),
[Rustybuzz advisory](https://rustsec.org/advisories/RUSTSEC-2026-0206.html),
[ttf-parser advisory](https://rustsec.org/advisories/RUSTSEC-2026-0192.html).

## Step 6 — authoritative typography and page geometry

After the DOCX prerequisite in PR #10, Step 6 resumed on
`dev/typography-layout-engine` from `db975fb`. The following review precedes the
new layout subsystem. It supersedes the earlier candidate-only status; actual
execution results belong in the milestone verification record.

| Question and primary evidence | Accepted strategy and limits |
| --- | --- |
| Shaping, metrics and glyphs: [Parley 0.13](https://github.com/linebender/parley/tree/32a137efbb228ad0b1c6a46b979dfb55d4584eae), [HarfRust 0.15](https://github.com/harfbuzz/harfrust/tree/71d965130ea93905f81b8f03a03d7ecc794fbc47), [Skrifa 0.49](https://github.com/googlefonts/fontations/tree/47a301615fd95acf0da9fb7dbcd0ea7c7df36943/skrifa), [OpenType processing](https://learn.microsoft.com/en-us/typography/opentype/spec/ttochap1) | Reuse maintained low-level shaping/layout and font-reading libraries in `office-layout`. Resolve styles before shaping; retain actual glyph advances, offsets, selected faces and variations. Characters, glyphs, runs and cursor graphemes are distinct. Font synthesis requires an explicit implementation or diagnostic. |
| Established alternatives: [COSMIC Text 0.19 manifest](https://github.com/pop-os/cosmic-text/blob/main/Cargo.toml), [LibreOffice frames](https://docs.libreoffice.org/sw/html/classSwTextFrame.html), [Knuth–Plass, 1981](https://doi.org/10.1002/spe.4380111102) | COSMIC Text is a credible maintained alternative and now uses HarfRust; its buffer/editor, discovery and raster stack adds overlap with our existing boundaries. Direct HarfRust would leave substantially more layout/fallback work. Parley fits positioned glyphs and custom page geometry. Begin with incremental line fitting and page flow; paragraph-wide optimization, hyphenation and mature Office pagination are later work. No complete editor/Office engine is embedded. |
| Unicode boundaries: [UAX #9](https://www.unicode.org/reports/tr9/tr9-52.html), [UAX #14](https://www.unicode.org/reports/tr14/tr14-57.html), [UAX #29](https://www.unicode.org/reports/tr29/tr29-49.html), [ICU4X 2.3 segmenter](https://github.com/unicode-org/icu4x/blob/a14f2dad852be26bad277ae704abf27a15cbfed1/components/segmenter/README.md) | The current standards are Unicode 18; core positions deliberately retain Unicode 17. ICU4X 2.3 graphemes also use 17, while default line-break constructors used by Parley use 15.1. Record these versions instead of claiming latest-Unicode conformance. Map geometry to core paragraph IDs and explicit byte/grapheme/UTF-16 boundaries. Bidi visual order never rewrites logical text. Dictionary-based breaking needs a deliberate feature choice. |
| Paragraph/line spacing: [OOXML spacing](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.spacingbetweenlines?view=openxml-3.0.1), [line measurements](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.spacingbetweenlines.line?view=openxml-3.0.1) | Adjacent before/after space uses a maximum, not their sum. Exact height stays fixed; minimum height expands for natural metrics. Keep ink bounds separate from advance/line boxes. Auto spacing uses the stored 1/240-line multiplier. The initial baseline/default policy is explicit; these rules do not establish Word pagination parity. Contextual spacing and compatibility settings remain outside the imported profile. |
| Cross-target geometry: [WebAssembly numerics](https://webassembly.github.io/spec/core/exec/numerics.html), pinned library feature manifests | Disable host-font discovery and platform math features; use the same font bytes and `libm` path on native/WASM. Preserve model twips/half-points and convert only at the layout boundary. Publish checked integer 1/64-point geometry, rounding nearest with ties away from zero; outline coordinates use 0.001 font units and transform scales nine decimals. Reject nonfinite/excessive values and compare actual native/WASM results. Arithmetic choices alone do not prove universal determinism. |
| Browser display: [SVG paths](https://www.w3.org/TR/SVG2/paths.html), [CSS Text](https://www.w3.org/TR/2026/CRD-css-text-3-20260814/) | Rust supplies positioned glyph outlines; React paints paths rather than asking browser text APIs to reshape, wrap or paginate. Geometry can agree while raster antialiasing differs. A read-only page preview verifies this boundary; replacing the editable surface requires later selection, IME and accessibility work. |

### Retrospective findings and correctness gates

Reviewed Steps 1–5: model/schema migration, IDs and paragraph-wide coordinates,
atomic transactions/history, source-preserving native/DOCX conversion, Rust view
inheritance, and current browser rendering/input. No new critical defect in
those foundations was confirmed by this review. The following risks must be
handled in the new layout implementation, with regression coverage:

- **Critical gates:** layout cannot mutate documents, normalize Unicode, discard
  source bytes, reuse stale geometry for edits, or publish partial results after
  a resource/measurement failure. Layout state stays outside files and history.
  Ligatures, combining sequences across runs, emoji and bidi need explicit
  mappings to existing cursor boundaries rather than glyph-count offsets.
- **Important:** resolve inherited character/paragraph fields, including
  individual indentation components and page-break-before. The browser's
  `AtLeast` projection as fixed CSS height and `overflow-wrap:anywhere` do not
  define engine layout. Extreme values can be valid model data but exceed this
  layout profile; return structured errors without clamping the model.
- **Important:** a literal LF inside a run remains a soft break in the same
  paragraph and retains its offset. Tabs, CR and other controls need explicit
  layout policies; do not reuse the browser paste normalization as a layout
  transformation. Physical left/right alignment is distinct from bidi direction.
- **Important compatibility limit:** Writer's unspecified layout defaults are
  1.5 lines and 8 pt after a paragraph. OOXML's documented fallback when spacing
  is absent throughout the style hierarchy adds no before/after spacing. No
  explicit property is discarded and the original remains retained, but these
  defaults are not Word-equivalent. Before claiming that parity, the DOCX adapter
  should deliberately materialize its known source defaults, with regression
  tests and a review of empty/style semantics. Layout must not acquire a DOCX
  special case, and this milestone does not retroactively change PR #10.
- **Important:** pinned fallback and missing-glyph diagnostics must remain
  observable without replacing stored font names. The catalog is finite; CJK,
  color emoji and all script/style combinations are not implied by Unicode text
  storage or successful shaping of a few sample strings.
- **Important for Step 7:** caret candidates describe logical-neighbor edges.
  After a mandatory soft break, leading/downstream affinity belongs on the next
  line; a preceding-neighbor edge may also appear on the prior line. These are
  valid core offsets, not a completed keyboard-navigation or hit-test policy.
  Resolve mandatory-break affinity before using candidates as edit destinations.
- **Optional later work:** broader Unicode/Office corpora, optimal line breaking,
  hyphenation, widow/orphan/keep rules, advanced sections and incremental
  invalidation beyond measured needs. No speculative model rewrite or new
  format adapter is needed for this milestone.

The accepted font catalog uses unmodified Arimo, Tinos, Noto Sans Arabic,
Devanagari, Hebrew and monochrome Noto Emoji. Official notices at pinned
revisions permit embedding/bundling under OFL 1.1 with their copyright/license
texts retained. No separate Reserved Font Name declaration appears in the
selected notices; modifications/subsetting still need a fresh license review.
Production assets must expose notices for fonts embedded in WASM. Provenance,
versions and SHA-256 values are recorded in the catalog manifest; proprietary
fonts and host discovery are excluded. See [DEPENDENCIES.md](DEPENDENCIES.md).

Research evidence includes the earlier four isolated native probes and WASM
compile, installed exact-version sources/manifests, official standards and
upstream implementation/conformance notes. Direct docs.rs requests were
unavailable through the web tool; pinned upstream sources and installed registry
sources were inspected instead. HarfRust documents remaining Arabic fallback
shaping failures with fonts lacking required OpenType substitutions. We have
not run its complete upstream conformance suite. Native/browser parity,
accessibility and performance claims must cite executed project tests rather
than these research probes. No Microsoft Word pagination comparison is implied.

### Step 6 implementation review corrections

Independent integration review confirmed and corrected two new correctness
defects: a valid hanging indent made the first line wider than Parley's declared
overall maximum and caused a panic; the maximum now covers every per-line width
without clamping the indent. Font underline offsets also needed their upward
font coordinates converted to downward page coordinates; underlines now appear
below the baseline for both bundled Latin families. Both have regression tests.

The bounded profile explicitly rejects tabs until tab stops are implemented,
unsupported control characters, excessive model measurements and positive line
heights below output resolution. CR/LF/CRLF remain source-preserving soft breaks.
Exact-height boxes keep their requested advance and report ink overflow rather
than implementing Word's undersized-line clipping. Minimum height can expand.
Suppressed spacing at page boundaries is diagnosed. Synthetic italic is explicit;
unavailable genuine weight and missing glyphs remain diagnosed limitations.
Outline keys include the actual pinned face and normalized variation coordinates;
font-unit outlines can be shared across sizes without confusing regular/bold
variants. Underline thickness has an explicit minimum of one output unit.
Diagnostics retain at most 127 unique findings plus a truncation marker so a
bounded warning list cannot silently appear complete.

`cargo test --offline -p office-layout --test layout_audit -- --nocapture` passed
all ten independent regressions after those fixes. They exercise variation/face
outline caching, different font sizes, cross-run graphemes, bidi affinities,
trailing soft breaks, mandatory-break downstream affinity, negative indents,
paragraph spacing, page continuation,
section geometry, resource/control failures and native original-source reopen.
This targeted result does not stand in for the final native/WASM/browser checks.

Real-browser review then found two preview focus races: deferred editor focus
could lose immediate typing on return, and revision-keyed preview focus could
steal title input during typing. Focus now changes only on explicit preview mode
transitions through a React layout effect. Regressions use immediate keyboard
input and delayed per-character title typing; they do not mask the races by
waiting for the old deferred callback. Shift+Enter now uses the existing Rust
transaction model to store a soft LF; DOM-only terminal break markers preserve
its exact text and offsets. See [the final verification](LAYOUT_VERIFICATION.md)
for executed full-suite results, native/WASM parity and measured costs.

## Step 7 — lightweight paginated editing and local autosave

The implementation starts from merged main `86f2ad8`. The document, transaction,
history, native-file/DOCX and typography boundaries from Steps 1–6 remain in use.
The owner supplied the approved [Lamplight visual reference](design/writer-lamplight-reference.png)
after the initial repository search. Visual fidelity must be assessed against
that reference and screenshots of the running application, not inferred from
the earlier Step 4.5/6 interface. No new editor framework, file format or cloud
service is justified by this milestone.

| Question and authoritative evidence | Accepted strategy and tradeoffs |
| --- | --- |
| Input and IME: [MDN beforeinput](https://developer.mozilla.org/en-US/docs/Web/API/Element/beforeinput_event), [Input Events Level 2, October 2026 draft](https://www.w3.org/TR/2026/WD-input-events-2-20261015/), [EditContext](https://developer.mozilla.org/en-US/docs/Web/API/EditContext) | Retain the controlled semantic input/composition adapter already exercised in the prototype. It is an input projection of Rust state; visible pages, caret, selection and visual navigation come from Rust layout. Handle both cancellable `beforeinput` and recovery `input`, since real browser/OS paths can omit or bypass cancellation. Preserve an active composition buffer and commit its final replacement once. A new general editor or a required EditContext dependency would add complexity and exclude browsers where that API remains unavailable. |
| Pointer selection and coordinate mapping: [Pointer capture](https://developer.mozilla.org/en-US/docs/Web/API/Element/setPointerCapture), [Selection API](https://www.w3.org/TR/selection-api/) | Convert viewport coordinates only into the Rust page coordinate system. Rust hit testing, caret geometry, visual navigation and selection rectangles choose positions using existing IDs and core Unicode boundaries. Retain explicit leading/trailing affinity and a preferred x for vertical movement. Pointer capture keeps a drag coherent across page boundaries; cancellation must release it. A CSS/input-proxy caret must not become a second visual layout policy. |
| Clipboard: [ClipboardEvent](https://developer.mozilla.org/en-US/docs/Web/API/ClipboardEvent) and existing Rust clipboard semantics | Keep Rust's structural plain-text extraction and one-transaction paste. Cut removes text only after clipboard writing succeeds. Browser HTML is neither the committed model nor an implicit rich-format importer. This deliberately does not add rich clipboard fidelity. |
| Accessibility and keyboard: [ARIA multiline textbox](https://developer.mozilla.org/en-US/docs/Web/Accessibility/ARIA/Reference/Roles/textbox_role), [WAI-ARIA toolbar pattern](https://www.w3.org/WAI/ARIA/apg/patterns/toolbar/) | Keep a labeled semantic input target with predictable focus and native text exposure. SVG paths alone are not an accessible editor. Geometry-driven navigation must not interfere with IME or toolbar controls. A `toolbar` role entails its keyboard pattern; a simpler labeled group with normal native controls is preferable to an incomplete composite widget. Manual screen-reader, OS IME, dictation and mobile selection-handle behavior still need explicit testing beyond automated events. |
| Autosave and write ordering: [IndexedDB transactions](https://developer.mozilla.org/en-US/docs/Web/API/IDBTransaction), [transaction completion](https://developer.mozilla.org/en-US/docs/Web/API/IDBTransaction/complete_event), existing `WriterStorage` etags | One small save controller schedules debounced autosave and explicit Save through the same write lane. Capture immutable native bytes, document/session/revision and expected etag when an attempt begins. A successful earlier write advances that binding's etag even when later edits remain dirty; queued work then captures the latest revision and token. Success means transaction completion. No schema, local database version or JavaScript document model changes are needed. |
| Page lifecycle and unsaved work: [visibilitychange](https://developer.mozilla.org/en-US/docs/Web/API/Document/visibilitychange_event), [beforeunload](https://developer.mozilla.org/en-US/docs/Web/API/Window/beforeunload_event) | Flush pending committed work when the page becomes hidden as a best effort. `beforeunload` is a warning aid, especially unreliable on mobile, not a place to promise async save completion. Active composition suspends autosave and counts as uncommitted work even if the last Rust revision was already saved. Quota/conflict/I/O failures retain in-memory work, a persistent unsaved status and an explicit retry/export path. |

### Retrospective findings and implementation gates

Reviewed core model/version migration, stable IDs/Unicode coordinates, atomic
editing/history, full native files/original preservation, DOCX profile gates,
layout snapshots/carets and existing browser input/IndexedDB flows. No new
critical defect in those completed foundations was confirmed by the initial
review. Step 7 must retain their invariants rather than replacing them.

- **Critical gates:** a paginated interaction cannot apply stale geometry to
  another session/revision or translate a glyph count into a text offset. Bidi
  ranges can produce separate visual rectangles. Mandatory-break downstream
  affinity and wrap affinity need distinct handling; duplicate visual candidates
  are not distinct logical insertion destinations. Failures must not publish a
  partial edit, change source bytes or add layout/navigation entries to history.
- **Critical save gates:** autosave and manual Save cannot race independently
  against the same old etag, clear dirty state for an earlier revision, or let an
  obsolete document/session update the active status/association. Reopening the
  same document ID still changes the save epoch. A conflict pauses writes rather
  than overwriting another tab; a quota/error cannot silently become a saved
  status after the next successful typing command.
- **Important, existing integration gap:** the initial `beforeunload` guard used
  committed dirty state only. A clean document can have an active, visible IME
  buffer that is not yet a Rust transaction. The new save/status wiring must
  suspend saving and warn about that buffer without pretending to persist it.
  Browser termination can still discard uncommitted input; no lifecycle event
  provides a universal durability guarantee.
- **Important:** a semantic input projection needs the Rust caret origin for
  composition placement, while native browser selection changes must not undo
  geometry-driven navigation. Keep the input focusable/accessible rather than
  applying `hidden` or `display:none`. Avoid unconditional focus restoration
  after every render or asynchronous completion; Step 6 already found such
  races during title typing and preview transitions.
- **Important limits retained:** tabs/unsupported layout controls, finite font
  coverage, exact-box clipping differences, independent Writer spacing defaults
  and bounded full-layout snapshots remain explicit. A fallback presentation, if
  offered, must preserve the document and disclose unavailable pagination rather
  than manufacture CSS page counts. DOCX originals and unsupported-content
  diagnostics remain unchanged by the interface work.
- **Optional future work:** richer mobile selection handles, advanced visual
  navigation, rich clipboard formats and measured incremental rendering/layout.
  These do not justify tables/images, reviewing, PDF, cloud features or another
  document model in Step 7.

The small Lamplight command surface uses the approved warm brown/charcoal,
restrained gold and white-page direction. Commands remain tied to actual engine
capabilities; a static icon is not evidence that a feature works. Browser tests
must exercise real paginated typing, selection, caret movement, clipboard,
undo/redo, persistence and failure recovery. Chromium, Firefox and WebKit results
must be reported separately and only after execution. Synthetic IME is useful
regression coverage, not OS-level or accessibility certification.

The approved design's "Paragraph style where available" control reuses the
existing Rust assignment operation for named styles already in the document.
It does not add style creation/editing, templates, a style model or a second
inheritance policy. Direct overrides remain explicit. Advanced style/template
tools remain future work.

### Step 7 implementation review

Independent native review found an LTR-only assumption in horizontal line
transitions. A wrapped RTL paragraph needs its line progression derived from
shaped logical endpoint placements, rather than always pairing Right with the
next physical line. The correction retains explicit bidi/wrap affinity. Five
independent quality regressions passed with strict scoped Clippy: complete RTL
multi-page walks and pointer-selected wrap edges, disjoint bidi selection with
reversed direction, CRLF downstream geometry across a page, and invalid/extreme
coordinate requests. The bridge retains native geometry beside its serialized
response, avoiding a full glyph-JSON parse per pointer/navigation request; failed
bounded interaction transport restores selection, typing and visual state.

Surface review also identified three browser integration risks before final
verification: a canceled touch must not be treated as a tap; displayed-pointer
coordinates must carry their originating snapshot through the client/native
revision guard, including same-task edits before React commits; and background
geometry refresh must not restore a DOM selection into an unfocused editor.
The [Selection API focus behavior](https://developer.mozilla.org/en-US/docs/Web/API/Selection_API#selection_and_input_focus)
makes the last risk observable across browsers. The corresponding code paths
were corrected. All five focused regression cases passed in actual Chromium:
same-task stale-pointer rejection, pointer cancellation, title/Find focus during
refresh, the first key immediately after New, and an initially clean document's
IME save suspension/cancellation with unchanged persisted bytes. These results
do not certify other browsers or manual OS composition/assistive technology.

Autosave review found that a pure trailing debounce could defer every write
indefinitely during continuous typing. The shared write lane now combines a
750 ms inactivity timer with a 5-second foreground deadline, suspending both
during composition and retaining its pause-on-error rule. These are scheduling
targets, not guarantees against background timer throttling or process death.

Further foundation review confirmed a **critical ordinary-typing budget defect**:
the core created a new run for every collapsed insertion. A deliberately failing
pre-fix regression committed 1,000 individual keystrokes and found 1,000 runs
instead of one; the browser's 20,000-run safety cap could consequently reject an
ordinary writing session. The targeted correction extends only a nonempty
retained left prefix with exactly equal direct properties (including `Option`
presence). It does not merge existing neighbors, absorb the right suffix,
resolve styles or alter noncollapsed replacement/no-op policy. Independent diff
review confirmed the restricted branch; Decision 040 records the contract.
All eight focused typing regressions passed after the correction, including the
1,000-keystroke case producing one run and retaining undo/redo. The independent
quality reviewer reran those eight successfully. The ensuing full Rust gate
passed 442 tests with formatting, strict workspace Clippy and build checks.

A separate production performance sample used 11,840 characters in 50 paragraphs
and 30 sequential trusted keyboard events, awaiting each visible geometry update.
Before the targeted presentation optimization, `beforeinput` to committed
revision/ready DOM mutation measured median 94.0 ms, p95 132.9 ms, maximum
136.6 ms; the next animation-frame callback measured 120.6/162.2/171.8 ms.
An isolated rerun after exact memo comparisons of unchanged Rust glyph output
measured committed DOM geometry at 90.6/109.0/113.3 ms and the next animation
frame at 124.8/144.4/151.8 ms (median/p95/maximum). The observed geometry p95
fell about 18%; its median remained similar and the next-frame median increased.
One 30-event run per version is not statistical proof of a general speedup.
These warm-font, machine-specific figures measure neither OS input latency nor
actual painted pixels. Roughly 0.1-second full-layout/JSON latency on this
11.8k-character sample remains an **important** performance limitation for the
planned audit. No worker/cache architecture or browser text-layout substitute
was introduced to conceal it. Future measurements must retain the same scenario.

The planned comprehensive technical/commercial licensing audit follows Step 7.
This milestone preserves existing notices and the pending project-license
decision; it does not claim that broader audit has been performed. Final test
results, screenshots and unresolved limitations belong in the milestone report.

## Step 7.5 — local file-format adapters

The owner's explicit Step 7.5 instruction authorizes DOCX/ODT/RTF/TXT opening,
editing and saving, PDF export, native Writer retention and merging after the
verification gate. It authorizes this bounded feature milestone after Step 7;
it does not claim that the planned comprehensive technical/security/commercial
audit has occurred or select a project software license.

The initial clean `dab4db7` baseline passed formatting, strict workspace Clippy,
442 Rust tests, workspace build and the locked `wasm32-unknown-unknown` check
before implementation. The existing document, transactions, Unicode positions,
full native file/source records, DOCX adapter and layout engine were inspected.
No model/native/database schema change or browser document engine is needed.

| Question and primary evidence | Choice and tradeoff |
| --- | --- |
| Format boundaries and preservation | A concrete `office-formats` facade dispatches additional adapters and reuses `office-docx`. Format structures and parser dependencies remain outside `office-core`. `WriterFile` already retains a format-neutral original independently from the editable document/history; no new original-history or generic plugin framework is needed. |
| ODT package/schema: [OASIS ODF 1.3 packages](https://docs.oasis-open.org/office/OpenDocument/v1.3/OpenDocument-v1.3-part2-packages.html) and [schema](https://docs.oasis-open.org/office/OpenDocument/v1.3/OpenDocument-v1.3-part3-schema.html) | Reuse pinned ZIP and namespace-aware XML libraries. Import the supported core subset of ODF 1.2/1.3/1.4 and write deterministic 1.3; validate MIME/manifest, compression and expanded sizes, XML/resource limits and references. No extraction, embedded execution, external fetches or encryption. Unrepresentable constructs preserve the original without a partial editable projection. `ferrodoc-odt`'s Pandoc-oriented conversion and `lo_zip`'s wider engine coupling did not justify new dependencies; their licenses/fitness were not independently established and no commercial-compatibility claim is made for them. Supporting every ODF version/extension is not inferred from a ZIP/DOM library. |
| RTF: [Microsoft's normative RTF reference](https://learn.microsoft.com/en-us/openspecs/exchange_server_protocols/ms-oxrtfex/3402a589-2578-4a15-99ce-04a91545f327), [`rclip-rtf` tokenizer](https://docs.rs/rclip-rtf/0.1.0/rclip_rtf/token/index.html), [`rtf-parser`](https://github.com/d0rianb/rtf-parser) | Use only `rclip-rtf`'s borrowed byte tokenizer, with default/optional allocation and codepage features disabled. Its semantic reader is clipboard-oriented and cannot establish a complete Writer projection. The adapter must retain group state, signed UTF-16 escapes/surrogate pairs and positional `uc` fallback; explicitly support known codepages and refuse replacement-character decoding. Guard tokenizer numeric saturation, ignored NUL, binary lengths, depth and token counts. Unknown semantic content is not a successful import. The small 0.1 dependency remains a maturity risk, not a claim of a battle-tested complete RTF engine. |
| TXT encoding: [Unicode BOM FAQ](https://www.unicode.org/faq/utf_bom.html) | Strict UTF-8, optional UTF-8 BOM or BOM-marked UTF-16 LE/BE; no guessed legacy encoding or replacement decoding. Normalize CRLF/CR to paragraph boundaries as documented while retaining exact nonempty originals. Export UTF-8/LF and preserve interior FEFF/combining/emoji text. TXT explicitly loses rich formatting, metadata and structural distinctions. |
| PDF generation: [`pdf-writer`](https://github.com/typst/pdf-writer), [`svgtypes`](https://github.com/linebender/svgtypes), [OFL use in documents](https://openfontlicense.org/ofl-faq/) | Write vector PDF from the existing Rust page geometry and glyph outlines, using a mature low-level PDF object writer and SVG path/transform parser. Do not invoke another layout engine or browser print pipeline. Reuse glyph forms, convert coordinate systems explicitly and bound output incrementally. Outlined output is not searchable/selectable, tagged PDF/UA or PDF/A; report those limits before saving. Existing bundled-font notices remain applicable. |
| Open/Save As and loss review | General Open accepts only readable formats; PDF is export only. Rust assesses conversion warnings against session/document/revision, and export rejects stale review or absent acknowledgement. The existing local native save/autosave lane remains authoritative. Downloading an external copy does not mark newer work saved or overwrite a browser record; the browser controls filesystem destination/duplicate names. |

### Targeted review gates

- **Critical:** no partial imported document may replace live work, consume
  history or discard an original. Supported external imports are detached and
  their complete native/source file must commit to IndexedDB before activation.
  Unsupported RTF/ODT/DOCX content remains read-only under the existing policy.
- **Critical:** codepage/Unicode fallback, XML whitespace, named-style defaults,
  page geometry, unsafe ZIP names/duplicates and unknown properties must not
  become silent text/formatting loss. Parse acceptance alone is not fidelity.
- **Critical:** Save As warnings need an explicit acknowledgement and the exact
  reviewed origin. PDF export must use native pagination and reject unsupported
  layout rather than inventing CSS pages or flattening a failed layout silently.
- **Important:** external formats cannot promise native IDs, exact optional
  property/run distinctions or original retention. Keep a native backup. Browser
  downloads cannot establish crash durability, filesystem replacement or the
  final path chosen by the user.
- **Important:** independent interoperability evidence must supplement self
  round trips. LibreOffice-produced RTF often includes script properties, unused
  styles and list tables even for a basic paragraph. Such an input can remain
  preserved read-only; its presence in a fixture is not an editable-support claim.
- **Important:** the additional graph's permissive declarations do not complete
  a commercial audit. `rclip-rtf` and `rclip-core` publish dual-license SPDX
  declarations but omit standalone license files even at their published VCS
  roots. The selected Apache-2.0 text and exact package/source/author evidence
  are retained without inventing an upstream copyright grant; see DEPENDENCIES.

The new integration tests exercise actual compiled WASM and the production
browser UI for import → Rust edits/formatting → native local save/reload/reopen →
external export/reimport, loss gates, source bytes, malformed inputs, PDF-only
behavior, Unicode filenames and shipped notices. Results belong in the final
milestone verification record, not inferred here from test configuration.

### Corrections from the implementation review

- A valid ODT title split by comments/processing instructions/CDATA initially
  retained only its first text node. A failing native regression reproduced
  `A` instead of `ABC é 👩🏽‍💻`. The adapter now concatenates its immediate text
  children while retaining the existing unsupported nested-element checks;
  all 18 focused ODT regressions passed after this targeted correction.
- The first facade's document-only `Writer` export could not retain a source
  kept in `WriterFile`. It now explicitly rejects that call; `export_file`
  accepts the complete file and preserves originals for native copies. The
  browser already used full-file serialization, but the public native facade
  needed the same safe contract. Regression tests cover the natural import/file
  copy workflow as well as the rejected document-only route.
- TXT export initially allowed native NUL content that its own importer rejects,
  and literal CR could violate its LF/output-reopen policy. Export now refuses
  NUL and normalizes CRLF/CR in a streaming pass, including split runs. A semantic
  initial U+FEFF also needs a separate transport BOM so reimport does not strip
  real document content. Focused regressions cover all three cases; these rules
  are explicit conversion limitations rather than silent model repair.
- A model value can be valid natively but exceed an external adapter's own
  import profile: oversized RTF font/title labels or extreme ODT font sizes
  could initially produce an unusable copy. Before publishing editable-format
  bytes, the facade now performs a bounded reopenability validation and rejects
  out-of-profile output. This guards the adapter contract; it is not semantic
  equivalence or independent-consumer proof. Error labels/messages are bounded
  at construction so rejected values cannot amplify transport diagnostics.
- Literal TAB/LF/CR in ODT metadata/style attributes undergo XML normalization.
  A failing regression demonstrated title CR becoming LF. The exporter now
  emits numeric character references for these characters, retaining exact
  title/style-label spelling. All 19 focused ODT tests and scoped Clippy passed
  after both targeted metadata fixes.
- Actual Chromium identified a Save As focus regression: Cancel could return to
  a hidden menu trigger, losing subsequent keyboard input. The correction closes
  the modal before unmount and restores the guarded visible invoker or logical
  visible fallback without an animation-frame delay. Closed-details content can
  retain nonzero client rectangles while refusing focus; menu invokers explicitly
  return to their visible summary instead of relying on that geometry heuristic.
  The focused actual-Chromium regression passed menu return and immediate keys
  after keyboard Escape, Cancel and Download. Complete suite and other-browser
  results belong in the milestone verification report.

The broader [`svg2pdf`](https://github.com/typst/svg2pdf) alternative would bring
a separate usvg/resvg/image/font stack despite already-resolved glyph geometry;
its upstream repository was archived at review time. The smaller pinned PDF
writer/path parser fits the current
boundary without that duplicated rendering pipeline. See the selected manifests
in [DEPENDENCIES](DEPENDENCIES.md) for exact versions and notices.

### Independent-consumer check executed

Using the real `project-notes.writer` fixture, the adapters generated DOCX, ODT,
RTF, TXT and PDF. LibreOfficeDev 26.8.0.0.alpha0 (`2c87e51…`) independently opened
the four editable-format exports and wrote UTF-8 text. Each retained the native
body's complete 405 Unicode scalars, adding one final paragraph LF (406 total);
leading output BOM handling and the reader's one terminal newline were checked
explicitly. This comparison did not normalize combining marks or emoji.

LibreOffice also saved the three rich-format exports as flat ODF and PDF.
Its flat-ODF styles retained requested Tinos/20-point/bold heading properties,
Arimo/12-point body and a bold `dependable editing` span. Independent PyMuPDF
1.26.6 extraction of the LibreOffice PDFs verified a 20.0-point bold serif
heading, 12.0-point nonbold body, 12.0-point bold emphasis and color `#24211d`.
Actual installed-font substitution occurred: DOCX/RTF headings used Noto Serif,
ODT used Liberation Serif, and bodies used Noto Sans. All three samples were one
page. This establishes these sample properties, not equal font metrics,
pagination or general Word/ODF/RTF fidelity.

The engine's own outlined PDF was independently accepted as PDF 1.7 by Poppler
26.05.0, with title `Project notes`, one 612×792-point Letter page, no encryption
or JavaScript and no tagging. Its 72-DPI raster was inspected and showed the
intended heading, mixed bold body and monochrome emoji. `pdftotext` returned only
a page form-feed, confirming the disclosed absence of searchable text. A separate
four-page PDF/geometry fixture is checked by the optional independent pypdf
verifier. These tools are test consumers, not shipped conversion dependencies.

The checked-in `export_format` example reproduces the format artifacts using the
same public adapters; native output uses full `WriterFile` serialization. Exact
producer/consumer commands and fixture hashes are in
[`tests/fixtures/formats/README.md`](../tests/fixtures/formats/README.md).

[input]: https://www.w3.org/TR/input-events-2/
[beforeinput]: https://developer.mozilla.org/en-US/docs/Web/API/Element/beforeinput_event
[editcontext]: https://developer.mozilla.org/en-US/docs/Web/API/EditContext
[pm-view]: https://raw.githubusercontent.com/ProseMirror/website/master/markdown/guide/view.md
[pm-state]: https://raw.githubusercontent.com/ProseMirror/website/master/markdown/guide/state.md
[wasm-deploy]: https://wasm-bindgen.github.io/wasm-bindgen/reference/deployment.html
[wasm-cli]: https://wasm-bindgen.github.io/wasm-bindgen/reference/cli.html
[vite-assets]: https://vite.dev/guide/assets
[vite-wasm]: https://vite.dev/guide/features.html#webassembly
[dom]: https://dom.spec.whatwg.org/#interface-characterdata
[selection]: https://developer.mozilla.org/en-US/docs/Web/API/Selection/setBaseAndExtent
[unicode17]: https://www.unicode.org/reports/tr29/tr29-47.html
[unicode-latest]: https://www.unicode.org/reports/tr29/
[composition-order]: https://www.w3.org/TR/2026/WD-input-events-2-20261015/#input-event-order-during-composition
[compositionend]: https://developer.mozilla.org/en-US/docs/Web/API/Element/compositionend_event
[iscomposing]: https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent/isComposing
[ooxml-paragraphs]: https://learn.microsoft.com/en-us/office/open-xml/word/working-with-paragraphs
[ooxml-bold]: https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.bold?view=openxml-3.0.1
[libreoffice]: https://docs.libreoffice.org/sw/html/classSwDoc.html
[onlyoffice]: https://api.onlyoffice.com/docs/docs-api/get-started/how-it-works/
[onlyoffice-license]: https://github.com/ONLYOFFICE/sdkjs
[execcommand]: https://developer.mozilla.org/en-US/docs/Web/API/Document/execCommand
[idb]: https://www.w3.org/TR/IndexedDB-3/
[idb-complete]: https://developer.mozilla.org/en-US/docs/Web/API/IDBTransaction/complete_event
[quotas]: https://developer.mozilla.org/en-US/docs/Web/API/Storage_API/Storage_quotas_and_eviction_criteria
[uuid]: https://developer.mozilla.org/en-US/docs/Web/API/Crypto/randomUUID
[textbox]: https://developer.mozilla.org/en-US/docs/Web/Accessibility/ARIA/Reference/Roles/textbox_role
[toolbar]: https://www.w3.org/WAI/ARIA/apg/patterns/toolbar/
[keyboard]: https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/
[focus]: https://www.w3.org/WAI/WCAG22/Understanding/focus-visible.html
[status]: https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html
[react-license]: https://github.com/facebook/react/blob/main/LICENSE
[vite-license]: https://github.com/vitejs/vite/blob/main/LICENSE
[ts-license]: https://github.com/microsoft/TypeScript/blob/main/LICENSE.txt
[pw-license]: https://github.com/microsoft/playwright/blob/main/LICENSE
[wasm-license]: https://github.com/wasm-bindgen/wasm-bindgen
[cf-assets]: https://developers.cloudflare.com/workers/static-assets/
[cf-config]: https://developers.cloudflare.com/cf/projects/cloudflare-config/
[cdp-ime]: https://chromedevtools.github.io/devtools-protocol/tot/Input/#method-imeSetComposition

## Save, export and mobile-input reliability review

Questions: why can a retry remain broken, can explicit Save be lost during a
document replacement, how long should an export URL survive, and do paged input
focus/viewport updates interfere with touch editing?

Primary sources reviewed:

- [IndexedDB transaction completion](https://developer.mozilla.org/en-US/docs/Web/API/IDBTransaction/complete_event)
  and [abnormal connection close](https://developer.mozilla.org/en-US/docs/Web/API/IDBDatabase/close_event):
  keep completion/etag guards; reconnect a failed or unexpectedly closed connection.
- [WebKit user activation](https://webkit.org/blog/13862/the-user-activation-api/):
  non-mouse pointerup can trigger activation. Keep focus synchronous in the tap;
  do not assume pointerup itself is forbidden. Compatibility mouse/click events
  can subsequently move focus, so complete focus in that same gesture.
- [VisualViewport](https://developer.mozilla.org/en-US/docs/Web/API/VisualViewport):
  layout and visible viewports differ with keyboards and zoom. Observe its scroll
  and resize events; keep the input and Rust caret visible. Bound added scroll
  padding to prevent feedback when almost none of the workspace is visible.
- [Anchor download](https://developer.mozilla.org/en-US/docs/Web/API/HTMLAnchorElement/download),
  [HTML downloading](https://html.spec.whatwg.org/multipage/links.html#downloading-resources)
  and [Blob URL lifetime](https://developer.mozilla.org/en-US/docs/Web/URI/Reference/Schemes/blob):
  the browser controls delivery and offers no success receipt. Keep one copy
  alive with fresh native download/open links, then revoke on explicit lifecycle
  boundaries. [WebKit's historical blob-download issue](https://bugs.webkit.org/show_bug.cgi?id=211234)
  is context, not proof of current iOS behavior.

Reviewed `WriterStorage`, `WriterSaveController`, App file dialogs/conversions,
`PagedEditor`, `EditorSurface`, Rust/WASM export APIs and the existing workflow
tests. Confirmed critical save-recovery defects: a rejected open was cached and
a replacement blank document's explicit Save could be swallowed by an exhausted
old save lane. Targeted fixes preserve transaction completion, conflict checking,
source retention and current-session dirty state. Important browser defects:
Save As shortcuts reset consent/stacked dialogs; a trusted touch tap could lose
focus; stale geometry prevented focus; viewport reveal ignored the keyboard.
Regression tests cover each. Review also found padding feedback under nearly
complete occlusion; bound it without changing Rust geometry.

Retain existing serializers and browser download support rather than introduce
Web Share, File System Access, another storage library or a new editing layer.
No new dependency or licensing decision. Native download/open links are a modest
fallback for blocked automatic handoff, not a claim of OS-level success. Exact
results, initial test-harness corrections and untested physical iOS/PWA behavior
are recorded in [WRITER_RELIABILITY.md](WRITER_RELIABILITY.md).
