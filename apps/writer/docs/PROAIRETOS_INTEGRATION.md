# Writer × ProairetOS Lamplight integration (phase 1)

Writer remains an independent repository and deployment. ProairetOS remains the authority for Lamplight scheme generation; Writer consumes a small, versioned semantic-token message.

## Current implementation

- ProairetOS branch `feature/writer-lamplight-integration` registers `writer` in `src/lamplight/palette.ts` and exports `connectWriterLamplight(frame, scheme)` from `src/app/writerLamplight.ts`.
- Writer branch `design/writer-lamplight-polish` installs `src/lamplightBridge.ts` in `main.tsx`.
- The bridge only accepts a `proairetos:lamplight` message from its own parent on exactly `https://proairetos.com` or `https://research-and-development-proairetos.boleydavid7.workers.dev`, with `version: 1`, `app: 'writer'`, a valid mode and ten validated six-digit hex tokens. Unrelated frames and other origins are rejected. The preview CSP and static `_headers` permit those same embedding origins.
- Writer announces `writer:lamplight-ready` only when its incoming referrer identifies one of those trusted parent origins. When referrer information is absent, it waits for a validated parent theme message. The host should send the current scheme on load and again on appearance changes; no document contents or credentials are sent through this theme bridge.
- Standalone Writer keeps its existing CSS fallback colors. Document paper is not themed.
- Writer's independent build uses root asset paths, restored in `72a0113`. Embedding it does not require a `/writer/` asset base; see [deployment paths](WRITER_SUBPATH_DEPLOYMENT.md).

## Remaining integration work

1. Verify the separately authorized standalone deployment, access restrictions, direct refresh and JS/CSS/WASM loading at its actual origin. Any future `/writer/` proxy needs a separate review of asset paths and Worker routing; a route alone does not strip a prefix.
2. Verify the host mount and adapter against the exact trusted origins above. Browser regressions use a routed trusted test host and the actual production preview; they do not establish that the live ProairetOS deployment or private access configuration works.
3. Expose a `refresh()` method on the host adapter and invoke it on OS light/dark and theme changes.
4. Keep regression coverage for theme changes, validated token values, standalone fallback, paper geometry and embedded editing/save/reopen. Further source/origin threat-model checks and actual host/device verification remain necessary before release.
5. Audit storage and service-worker scope before sharing an origin. Keep Writer IndexedDB names and document ownership isolated, and review the security impact of sharing origin with the OS.
6. Add a separate, explicitly permissioned file-handoff interface for Files → Writer. Do not pass account credentials or document contents over the theme bridge.
7. Run Writer's build, Rust/WASM and browser tests and ProairetOS's existing Lamplight tests before release.

Do not merge or deploy until both branches pass validation.
