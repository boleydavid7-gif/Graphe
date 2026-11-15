# Phase 2, work area 4: measured library and editor performance

Start: `9648a46`, after completed selection/compatibility and the PDF investigation.
No deployment, integration change, native schema or database migration is part of
this work. Use isolated synthetic libraries and documents, never user data.

## Questions and method

Measure library-list latency and retained binary/heap memory at increasing record
counts and sizes. Measure Rust/WASM commit/layout and real-browser presentation
on increasing document/page counts. Separate warm font behavior, JS heap,
ArrayBuffer backing storage and OS/user-perceived latency; these are not equivalent.
Run timing windows separately to avoid benchmark contention. Report actual sample
counts and distributions, not estimated speedups or overlapping suite totals.

Inspection found `WriterStorage.list()` calls `getAll()` and then
`summary(decode(value))`. `decode` copies each complete native byte buffer even
though the returned summary has no bytes. The array also retains all structured
clones until traversal finishes. Compare the actual implementation with a bounded
cursor/metadata-validation alternative before making a change.

The editor already memoizes glyph definitions and page lines using all rendered
Rust properties. Do not reintroduce that completed optimization. Layout remains
Rust-owned; no browser text measurement or competing pagination is permitted.
Preserve existing native bounds and the explicit 32MiB layout-response failure.

## Research and invariants

- [MDN getAll](https://developer.mozilla.org/en-US/docs/Web/API/IDBObjectStore/getAll):
  results are structured clones of the selected full values.
- [MDN openCursor](https://developer.mozilla.org/en-US/docs/Web/API/IDBObjectStore/openCursor):
  iterate the existing object store without adding a metadata database/migration.
- [IndexedDB transaction completion](https://developer.mozilla.org/en-US/docs/Web/API/IDBTransaction/complete_event):
  return a completed list only when the read transaction completes; request success
  or the final cursor event alone is not the completion boundary.

Any optimization must retain validation of damaged records, deterministic sorting,
all record warnings, load/save byte ownership, write serialization, revisions,
etags, transaction completion and immutable originals. A cursor still reads each
full record; it is not a claim of metadata-only disk I/O. Do not trade corruption
checks or storage protections for benchmark results.

## Editor measurements

Chromium 155, frozen production bundle, isolated public synthetic text, warm fonts.
Twenty trusted keys per size; every key advances the Rust revision and is checked
in committed SVG geometry. No runtime errors or lost input occurred. The next RAF
is a scheduling observation, not proof of physical pixels or OS input-to-paint.

| Initial pages / UTF-16 units | Rust/WASM edit p50 | WASM layout export p50 | Key to committed SVG p50 / p95 | Initial paste to render |
| --- | --- | --- | --- | --- |
| 10 / 24,920 | 3.4ms | 67.1ms | 172.3 / 188.4ms | 1.12s |
| 29 / 62,441 | 8.0ms | 167.2ms | 360.4 / 459.9ms | 1.99s |
| 42 / 87,461 | 11.6ms | 229.9ms | 486.0 / 610.7ms | 2.82s |

The layout-export column includes the actual WASM export, not browser JSON parsing
or React/SVG work afterward. Component medians are not additive. Selection geometry
p50 was 0.1 / 0.2 / 0.3ms. Separate raw-WASM calibration included layout+JS parsing
p50 91 / 221 / 316ms and cached response+parsing 19 / 44 / 97ms; these are different
measurement windows, not extra correctness passes.

The 42-page case retained 82,757 SVG uses and 251,222 DOM nodes. After explicit GC,
reported JS heap was 77.4MB, embedder heap 370.4MB and WASM linear memory 134.3MB.
These counters overlap/differ in scope and **must not be summed as process RSS**.
They do not establish physical mobile memory behavior. A 540-paragraph/150,011
UTF-16-unit document hit the existing 32MiB layout-response limit; exact native
bytes and the engine snapshot were unchanged by that failure.

This confirms an important performance limit. Full relayout/serialization and
presentation dominate transaction work. The existing glyph memoization is useful
and remains. A bounded incremental paragraph/layout cache and smaller transport
need a concrete design and review, including style/geometry invalidation, outline
identity, bidi, pagination and stale-revision behavior. No speculative layout or
document-model rewrite is made here. Large-document responsiveness remains an
explicit release limitation rather than a claimed optimization.

Raw evidence: `/tmp/writer-phase2-editor-performance.json` and the corresponding
measurement script/logs. All sixty trusted keys are measurement correctness
observations, not sixty additional test-suite passes.

## Library results and accepted change

Actual updated `WriterStorage` versus an explicit audited `getAll`/`decode`
reference using the same validation/helpers, five interleaved trials per case:

| Synthetic library | List median before / after | Sampled binary backing delta before / after |
| --- | --- | --- |
| 1,000 × about 10.3KiB native files | 111.2 / 66.6ms | 10.08 / 6.48MiB |
| 100 × about 1.003MiB source-bearing native files | 210.8 / 154.3ms | 100.23 / 1.00MiB |
| 200 × about 1.003MiB source-bearing native files | 483.5 / 302.2ms | 200.45 / 48.11MiB |

These source-bearing files are valid public synthetic native documents with
retained TXT originals. They stay within the existing per-record limit. The
actual measured storage source SHA-256 is
`592ce1891ef8a316922fa435bce82848a756fa41905b8f7951c51e1e24587fe1`.

Memory was sampled through CDP `Runtime.getHeapUsage.backingStorageSize` at a
controlled request-success point. GC occurs before the probe, not while the
debugger is paused. Timing trials contain no debugger pauses. The figures are
not absolute peaks, live-object proofs or process RSS. Cursor caches and event/
GC lifetimes matter; **one-record memory is not guaranteed**. This is one managed
Linux Chromium 155.0.8059.79 environment, not a physical mobile result.

Earlier six-case controls also tested cursor iteration while retaining `decode`'s
byte copies: that was worse (100 large files 200.7ms versus old 185.0ms) than
cursor plus metadata-only validation (151.4ms). Small 100-record libraries could
be slower (9.0 versus 7.4ms). Those separate preliminary windows informed the
choice; the table above is the authoritative final actual-source comparison.

The actual production **Open document** UI, five separate before/after trials,
improved from 193.2 to 157.7ms median for 1,000 small records and 533.0 to 229.2ms
for 100 large records. Counts and order matched. The rebuilt `index-B8LnQAs5.js`
used five cursor calls and zero `getAll` calls in that workflow. UI timing includes
row presentation and the next frame; it is a different window from the API table.

Changed `storage.ts`: extract the existing checks into `validateRecord`; retain
owned byte copies in load/save `decode`; list via one read-only cursor, retaining
summaries and warnings, and sort at cursor exhaustion. Publish only on transaction
completion. No schema/version/index change, write-lane change, new dependency or
new metadata database. Reads still visit complete stored values. No storage or
document integrity protection was relaxed.

## Regressions and reproduction

- `storage.test.ts`: three new cases for metadata without redundant byte copying,
  corrupt-record continuation/sorting/unchanged bytes and etags, and first/final
  cursor aborts. Existing load/save buffer-isolation coverage now includes listing.
- `phase2-library.spec.ts`: four real Chromium desktop/touch workflows for large
  valid bodies, corrupt envelopes, healthy opening/saving, aborted final success,
  retry and continued editing. All four passed, zero failed/skipped.
- TypeScript passed; full frontend unit suite has 64 passes, including those three
  new cases. Targeted 24 storage cases overlap that total.
- `npm run build` passed on the restored Rust/PDF engine and updated frontend.
  The WASM asset remains `office_wasm_bg-BMarlPw6.wasm`.

The initial two browser abort tests failed because instrumentation read
`request.result` after `cursor.continue()` reset the real request to pending.
The test now captures the terminal state before Writer's handler and aborts
afterward; both unit and browser hooks use this ordering and browser cases assert
zero uncaught errors. The production implementation did not need a correction
for that test-driver error. An initial benchmark lint failure for browser globals
was corrected; final lint results are recorded with overall verification.

Portable manual development tools use existing dependencies, isolated synthetic
browser contexts/databases, correctness checks and output files that refuse
overwrites. They have no timing pass/fail thresholds and are not test-suite passes:

```sh
npm run build
npm run preview -- --port 4173 --strictPort
node scripts/benchmark-editor.mjs --output /tmp/writer-editor.json
node scripts/benchmark-library.mjs --output /tmp/writer-library.json
```

Both tools passed syntax checking, focused ESLint and small actual Chromium smoke
runs. The editor smoke used `--paragraphs 2 --samples 2`; the final library smoke
used `--case 10:6144 --trials 3`. No full benchmark was repeated merely for tool
packaging. The final regression command was:

```sh
npm run test:browser -- tests/phase2-library.spec.ts --project=chromium --output=/tmp/writer-phase2-library-browser-final-results --reporter=list
```

Committed compact evidence: [editor](performance/phase2-editor.json),
[library API](performance/phase2-library.json) and
[library UI](performance/phase2-library-ui.json). Raw per-key traces, all timing
samples and diagnostic logs remain under `/tmp/writer-phase2-*`. There are no
physical-device, Firefox/WebKit, total-RSS or general latency guarantees here.
