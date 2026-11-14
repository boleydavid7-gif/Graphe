# Writer engineering research

Research date: **2026-11-14**. This record precedes Step 4.5 implementation.
It distinguishes established evidence, accepted prototype choices, and work
requiring later approval. Research is not a claim that browser behavior or
future Word compatibility has already been verified.

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
