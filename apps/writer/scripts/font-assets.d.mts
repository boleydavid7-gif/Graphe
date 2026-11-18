import type { Plugin } from 'vite';

/** Publishes the verified, byte-exact font catalog for dev and production. */
export declare function writerFontAssets(options?: { fontsDirectory?: string }): Plugin;
