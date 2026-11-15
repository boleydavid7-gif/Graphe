# Writer theme integration

Writer currently uses the Lamplight brown-and-gold palette by default. The UI now exposes semantic CSS custom properties to allow a ProairetOS host to apply the user's chosen palette later without Writer storing or managing appearance settings.

## Integration

Set semantic variables on the Writer app container (`.writer-app`) or its ancestor/root. The host should pass the already-resolved user palette, not a theme name. CSS variables inherit, and changes can be applied without reloading.

```css
.writer-app {
  --writer-shell: #24201b;
  --writer-workspace: #302a24;
  --writer-surface: #2b251f;
  --writer-surface-raised: #352d25;
  --writer-surface-menu: #332b23;
  --writer-control: #3b332b;
  --writer-control-hover: #44392e;
  --writer-control-selected: #53452a;
  --writer-text: #eee9e1;
  --writer-text-strong: #f7f2e9;
  --writer-text-muted: #c5baab;
  --writer-text-control: #f2ece2;
  --writer-border: #4b4034;
  --writer-border-control: #77634b;
  --writer-border-subtle: #393027;
  --writer-accent: #edc669;
  --writer-accent-hover: #f5d58b;
  --writer-accent-contrast: #282117;
  --writer-focus: var(--writer-accent);
}
```

Keep text/background and accent/contrast pairs accessible. `--writer-paper` and `--writer-paper-border` are intentionally separate from application colors; the document's page geometry, formatting and print/export appearance must not be recolored by the OS theme.

## Scope and follow-up

This is a first pass over the common application chrome and major shared colors. Several exceptional statuses, legacy one-off controls, decorative colors and shadows still use fixed values and require a dedicated visual audit. The versioned bridge in `src/lamplightBridge.ts` accepts complete, validated token sets only from the exact trusted parent window and allowed ProairetOS origins. Its ready handshake is bound to a trusted referrer; standalone Writer retains its own palette. Main’s embedded mobile spacing, hidden redundant identity and editing focus mode are retained alongside the audited selection and keyboard guards. The host owns appearance settings and persistence; Writer does not add a separate theme store or selector. Verify each theme in desktop/mobile layouts and run build and interaction tests before merging.
