import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import { writerFontAssets } from './scripts/font-assets.mjs';

export default defineConfig({
  plugins: [writerFontAssets(), react()],
  build: { target: 'es2022' },
  preview: {
    headers: {
      'Content-Security-Policy': "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors https://proairetos.com https://research-and-development-proairetos.boleydavid7.workers.dev",
      'X-Content-Type-Options': 'nosniff',
      'Referrer-Policy': 'no-referrer',
    },
  },
});
