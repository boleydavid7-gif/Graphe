# Layout reference fixtures

`layout-v1.writer` uses native format 2 and document schema 2. Its filename
versions the test fixture, not the native format. It contains stable identifiers,
inherited named styles, variable bold/italic, double underline, combining marks,
emoji, Arabic, Devanagari, Hebrew, wrapping and justification, indentation,
paragraph spacing, a soft line break, an explicit page break, custom page
dimensions and a second A4 section.

`native-parity.json` records the SHA-256 of the complete compact native Rust
layout JSON, including glyph paths and source-position mappings. The compiled
WASM suite compares its result with a newly executed native example and this
reviewed fingerprint. This checks runtime parity without committing a large
outline dump. It does not establish Word-equivalent pagination.

To deliberately update the fingerprint after inspecting a typography change,
run from `apps/writer` with the documented Rust toolchain in `PATH`:

```sh
node tests/layout-update-golden.mjs
```

The browser suite additionally compares the actual first-page SVG paths and
transforms against `layout-vector-v1.json` in its Playwright snapshot directory.
Generated element IDs are normalized; geometry and paths are unchanged. This is
a vector visual regression, independent of host font rasterization. The actual
running application is also captured in `docs/images/writer-layout-desktop.png`.
To deliberately update the SVG baseline:

```sh
npm run test:browser -- tests/layout.spec.ts --update-snapshots
```

Neither baseline is regenerated during normal test execution. Review baseline
changes rather than treating regeneration as a way to fix failing tests.
