# Writer deployment paths

Writer builds for the root of its independent Worker. Commit `72a0113` explicitly
restored Vite's root asset paths after the earlier `/writer/` experiment. The
production entry references `/assets/...`, including its Vite-managed Rust WASM
asset. Keep that behavior unless a separately reviewed deployment changes it.

The previous version of this document described an unverified `/writer/` mount.
Its proposed `base: '/writer/'` and asset-prefix configuration do not describe the
current build and must not be applied as deployment instructions. The retained
`tests/subpath-routing.test.mjs` filename is historical; its checks now protect
the intended standalone root paths.

ProairetOS can embed the independent Writer application from a specifically
trusted parent origin. The current preview CSP and static `_headers` allow the
two documented ProairetOS origins; the theme bridge validates both parent source
and exact origin. Embedding does not require changing Writer's asset base. The
merge's browser regressions exercise the real production app in a routed local
test host; they do not verify live Cloudflare routing or private access.

Before any separately authorized deployment:

1. From `apps/writer`, run `npm run build` and
   `node --test tests/subpath-routing.test.mjs`.
2. Verify the standalone entry, direct refresh, JS/CSS/WASM responses and MIME
   types at the actual deployment origin. Check CSP and runtime errors.
3. Verify the trusted host, keyboard editing, save/reopen and access restrictions
   without affecting unrelated ProairetOS routes.
4. If a future `/writer/` proxy is needed, review both frontend asset paths and
   actual Worker request routing together. A route alone does not establish
   prefix stripping or working WASM loading.

No live routing, DNS, access policy or deployment configuration is changed here.
