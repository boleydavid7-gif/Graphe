# Writer × ProairetOS Lamplight integration (phase 1)

Writer remains an independent repository and deployment. ProairetOS remains the authority for Lamplight scheme generation; Writer consumes a small, versioned semantic-token message.

## Current implementation

- ProairetOS branch `feature/writer-lamplight-integration` registers `writer` in `src/lamplight/palette.ts` and exports `connectWriterLamplight(frame, scheme)` from `src/app/writerLamplight.ts`.
- Writer branch `design/writer-lamplight-polish` installs `src/lamplightBridge.ts` in `main.tsx`.
- The bridge only accepts a `proairetos:lamplight` message from its own parent on the exact same origin, with `version: 1`, `app: 'writer'`, a valid mode and ten validated six-digit hex tokens.
- Writer announces `writer:lamplight-ready` to the parent when embedded. The parent replies with the current generated scheme. The host should call `send` again when appearance changes (currently the adapter's `send` is internal, so reconnecting is required until a refresh method is exposed).
- Standalone Writer keeps its existing CSS fallback colors. Document paper is not themed.

## Remaining integration work

1. Configure Cloudflare routing so `/writer` and `/writer/*` serve Writer while the main OS continues to serve other routes. Configure Vite's `base` and fix root-absolute assets; test reloads and WASM asset loading at `/writer`.
2. Mount Writer inside the ProairetOS desktop window and mobile app navigation, then connect the host adapter. A same-origin iframe is the first integration path, not a requirement for all future integrations.
3. Expose a `refresh()` method on the host adapter and invoke it on OS light/dark and theme changes.
4. Add tests for valid/invalid theme messages, source and origin checks, theme changes and standalone fallback.
5. Audit storage and service-worker scope before sharing an origin. Keep Writer IndexedDB names and document ownership isolated, and review the security impact of sharing origin with the OS.
6. Add a separate, explicitly permissioned file-handoff interface for Files → Writer. Do not pass account credentials or document contents over the theme bridge.
7. Run Writer's build, Rust/WASM and browser tests and ProairetOS's existing Lamplight tests before release.

Do not merge or deploy until both branches pass validation.
