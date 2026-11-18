# Step 8 audit and corrections

2026-11-18, directly on main from 3569291. This is a corrective increment,
**not completion of every requested Step 8 feature**. No branch, PR, deployment
or unrelated application change is included.

## Existing functionality preserved

Main already contained standalone image insertion/properties/history, desktop
in-flow drag and resize handles, mobile drag/pinch, row/column insertion/removal,
whole-row pagination, external verified fonts, page setup, section boundaries,
paragraph keep settings and bounded widow/orphan handling. These were inspected
and reused rather than replaced. Rust owns content, transactions and geometry.

## Corrections and additions

- Restore compiler/Clippy/test compatibility after schema7 paragraph pagination:
  initialize absent new test properties, remove an unused import, fix stale
  future-version/migration assertions, and apply required Rust formatting.
  Old schema rejection and data-integrity assertions remain intact.
- Narrow image-command TypeScript types; remove an unused gesture assignment;
  import Node TextDecoder explicitly in three existing tests. Add page-setup,
  section-break and paragraph-pagination WASM tests to the actual test command.
- Reassert Rust image selection and browser command focus on taps of an already
  selected image, so Delete works after toolbar/export interactions.
- Non-passive touch handling scoped to image gestures prevents native browser
  magnification from cancelling custom pinch. Pointer cancellation still drops
  unfinished edits; text/paper gestures keep their existing behavior. Physical
  Safari validation is unavailable.
- Add atomic column-width transactions and Widen/Narrow (12pt) / Equal widths
  controls. Transfer width to an adjacent column, preserve exact total width,
  retain stable cell/paragraph identities, and reject unusable widths/page overflow.
  No new schema or dependency. Core/native-WASM and real-browser regressions
  cover original text, exact undo/redo and native save/reopen.
- Fix DOCX export silently omitting supported keep_with_next/keep_together flags.
  Export w:keepNext and w:keepLines with explicit on/off values. The existing
  DOCX/ODT/RTF keep-property roundtrip test exposed and verifies the correction.

## Still incomplete

Merged/split cells, cell borders/shading, drag table resizing, row height controls,
splitting a tall row across pages and repeating table headers are not implemented.
They require a coordinated typed-model/version migration, transaction rules,
layout/selection/input projection and format adapters; this patch does not fake
these by flattening cells or accepting editable partial imports. Whole rows can
paginate over multiple pages, but over-height rows remain explicit layout errors.

Image placement remains in-flow and within the original section; floating/wrapped
pictures and cross-section image dragging remain unsupported. DOCX fidelity is
bounded by the existing supported profile, not full Microsoft Word compatibility.
Unsupported content retains original bytes and read-only protection. No claim of
complete Step 8, arbitrary Word fidelity or physical-device certification is made.

## Verification

- cargo fmt --all -- --check: passed.
- cargo clippy --workspace --all-targets -- -D warnings: passed.
- cargo test --workspace: 821 passed, zero failures (including doctests).
- cargo build --workspace: passed.
- npm run build: passed (release WASM, TypeScript, production Vite).
- npm run lint: passed; no lint rule disabled.
- npm test: 87 passed.
- npm run test:wasm: 91 passed, zero skipped; real compiled engine.
- npm run test:build: 10 passed.
- Image/table Chromium final run: 29 passed, zero failures, 2.5 minutes.
  Includes actual Chromium multi-touch pinch, image drag/handles, table layout,
  structural edits, column sizing and persistence. Mobile events are emulated.

Earlier browser execution exposed Delete focus and pinch cancellation failures;
the corrected focused rerun passed both. The final combined run supersedes those
intermediate outcomes. Do not add overlapping reruns to suite totals.
Full unrelated browser suite, Firefox/WebKit, physical iOS/Android, Microsoft Word
and hosted cross-platform CI are not claimed executed for this increment.
Existing hosted-CI and development dependency advisory limits remain documented.
