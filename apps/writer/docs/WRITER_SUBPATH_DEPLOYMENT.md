# Writer Worker at proairetos.com/writer

Writer is deployed independently from ProairetOS. Its Cloudflare Worker route is `proairetos.com/writer*` on the `proairetos.com` zone. The route must point to the **Writer** Worker, never the main ProairetOS Worker.

## Deployment configuration

The Vite build uses `base: '/writer/'`, so built JS/CSS and Vite-managed WASM assets are requested under `/writer/`. Configure the Writer Worker's static assets with `base_path: '/writer'` (or the corresponding dashboard setting). The static assets directory is `apps/writer/dist` and its root contains `index.html`. If deploying via Wrangler, an example configuration fragment is:

```jsonc
{
  "assets": {
    "directory": "./dist",
    "base_path": "/writer",
    "not_found_handling": "single-page-application"
  }
}
```

This fragment is documentation, not an automatically deployed Wrangler configuration. Check the actual deployment configuration before applying it. A route alone does **not** strip the `/writer` prefix. If the existing Worker has a custom fetch handler, ensure it serves the correct assets without swallowing requests. Route matching `/writer*` also includes `/writer-test`; use an explicit exclusion or narrower routing where practical.

## Verify before release

1. Build from `apps/writer` using `npm run build`, then `node --test tests/subpath-routing.test.mjs`.
2. Confirm `/writer/` returns HTML and all requested `/writer/assets/...` JS/CSS/WASM requests return HTTP 200 with correct MIME types, not HTML fallback.
3. Check `/writer` (no trailing slash): redirect to `/writer/` or serve the same entry point; check direct refresh.
4. Check the browser console for missing modules, blocked WASM, CSP and runtime errors.
5. Verify standalone Writer and document save/open behavior. Existing `frame-ancestors 'none'` in Vite preview headers may need separate, tightly scoped review before iframe embedding.
6. Verify the main `/` ProairetOS app is unaffected.

Do not claim live routing works until the Cloudflare deployment and these HTTP/browser checks pass.
