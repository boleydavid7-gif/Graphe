/** Versioned, trusted-origin theme handoff for Writer embedded in ProairetOS.
 * The OS owns Lamplight's scheme generation. Writer only consumes semantic tokens.
 * Standalone Writer retains the CSS fallback palette.
 */
const TOKEN_MAP = {
  wall: '--writer-shell',
  page: '--writer-workspace',
  card: '--writer-surface',
  inset: '--writer-surface-raised',
  raised: '--writer-surface-menu',
  mica: '--writer-control',
  ink: '--writer-text',
  ink2: '--writer-text-muted',
  lamp: '--writer-accent',
  onLamp: '--writer-accent-contrast',
} as const;

type TokenName = keyof typeof TOKEN_MAP;
const HEX = /^#[0-9a-f]{6}$/i;
const NAMES = Object.keys(TOKEN_MAP) as TokenName[];
const TRUSTED_PARENTS = new Set(['https://proairetos.com', 'https://research-and-development-proairetos.boleydavid7.workers.dev']);

export function installLamplightBridge(): () => void {
  if (window.parent === window) return () => {};
  // Never accept theme messages from another origin or from unrelated frames.
  const receive = (event: MessageEvent) => {
    if (event.source !== window.parent || !TRUSTED_PARENTS.has(event.origin)) return;
    const data = event.data;
    if (!data || data.type !== 'proairetos:lamplight' || data.version !== 1 || data.app !== 'writer') return;
    if (data.mode !== 'light' && data.mode !== 'dark') return;
    if (!data.tokens || typeof data.tokens !== 'object' || !NAMES.every((name) => typeof data.tokens[name] === 'string' && HEX.test(data.tokens[name]))) return;
    const root = document.documentElement;
    for (const name of NAMES) root.style.setProperty(TOKEN_MAP[name], data.tokens[name]);
    root.style.setProperty('--writer-text-strong', data.tokens.ink);
    root.style.setProperty('--writer-text-control', data.tokens.ink);
    root.style.setProperty('--writer-control-hover', data.tokens.inset);
    root.style.setProperty('--writer-control-selected', data.tokens.raised);
    root.style.setProperty('--writer-accent-hover', data.tokens.lamp);
    root.style.setProperty('--writer-border', data.tokens.ink2);
    root.style.setProperty('--writer-border-subtle', data.tokens.inset);
    root.style.setProperty('--writer-border-control', data.tokens.ink2);
    root.style.setProperty('--writer-focus', data.tokens.lamp);
    root.dataset.writerAppearance = data.mode;
    root.style.colorScheme = data.mode;
    // Document paper and content intentionally remain independent of shell colors.
  };
  window.addEventListener('message', receive);
  // Send the ready handshake only to the exact trusted embedding origin.
  let parentOrigin: string | null = null;
  try { const origin = new URL(document.referrer).origin; if (TRUSTED_PARENTS.has(origin)) parentOrigin = origin; } catch { /* No referrer: wait for the trusted parent's theme message. */ }
  if (parentOrigin) window.parent.postMessage({ type: 'writer:lamplight-ready', version: 1 }, parentOrigin);
  return () => window.removeEventListener('message', receive);
}
