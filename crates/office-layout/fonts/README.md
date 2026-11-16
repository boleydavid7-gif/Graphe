# Writer font catalog `writer-fonts-v1`

These are unmodified, pinned upstream TrueType font files. The layout engine
registers their bytes explicitly; it must not discover host fonts or download
fonts during document layout. This makes font availability independent of the
operating system and browser. The 24 fonts total **12,978,372 bytes** (12.38 MiB), including 14 new
opt-in Latin fonts. The original ten faces are unchanged.

`manifest.json` records each original repository/path/commit, upstream Git blob
identity, byte count, SHA-256, internal version/family and variable-font axes.
All files use face index zero and ordinary `glyf` outlines. Local filenames
omit upstream axis brackets; their internal names and binary contents are
unchanged. `Arimo-Regular.ttf` is a variable upright font, not a static regular
face.

| Local font | Internal version | Weight/style |
| --- | --- | --- |
| Arimo-Regular.ttf | 1.341 | Upright; `wght` 400–700 |
| Arimo-Italic.ttf | 1.341 | Italic; `wght` 400–700 |
| Tinos-Regular.ttf | 1.340 | Upright 400 |
| Tinos-Italic.ttf | 1.340 | Italic 400 |
| Tinos-Bold.ttf | 1.340 | Upright 700 |
| Tinos-BoldItalic.ttf | 1.340 | Italic 700 |
| NotoSansArabic.ttf | 2.012 | Upright; `wght` 100–900; `wdth` 62.5–100 |
| NotoSansDevanagari.ttf | 2.007 | Upright; `wght` 100–900; `wdth` 62.5–100 |
| NotoSansHebrew.ttf | 3.001 | Upright; `wght` 100–900; `wdth` 62.5–100 |
| NotoEmoji.ttf | 3.002 | Monochrome upright; `wght` 300–700 |
| Carlito-Regular.ttf | 1.104 | Upright 400 |
| Carlito-Italic.ttf | 1.104 | Italic 400 |
| Carlito-Bold.ttf | 1.104 | Upright 700 |
| Carlito-BoldItalic.ttf | 1.104 | Italic 700 |
| Caladea-Regular.ttf | 1.001 | Upright 400 |
| Caladea-Italic.ttf | 1.001 | Italic 400 |
| Caladea-Bold.ttf | 1.001 | Upright 700 |
| Caladea-BoldItalic.ttf | 1.001 | Italic 700 |
| SourceSans3-Regular.ttf | 3.052 | Upright; `wght` 200–900 (default 200) |
| SourceSans3-Italic.ttf | 3.052 | Italic; `wght` 200–900 (default 200) |
| EBGaramond-Regular.ttf | 1.003 | Upright; `wght` 400–800 |
| EBGaramond-Italic.ttf | 1.003 | Italic; `wght` 400–800 |
| JetBrainsMono-Regular.ttf | 2.211 | Upright; `wght` 100–800 |
| JetBrainsMono-Italic.ttf | 2.211 | Italic; `wght` 100–800 |

Choose regular weight 400 or bold 700 explicitly, and width 100 for the script
fallbacks. Noto Sans Hebrew's file default is **100 (Thin)**, so relying on its
default variation coordinates would produce the wrong regular weight. The
Arabic, Devanagari, Hebrew and emoji fonts have no italic face in this catalog;
an engine must diagnose an unsatisfied italic request instead of claiming a
genuine italic face was used. No synthetic slant or synthetic bold is required for families with real
italic/bold faces. Source Sans 3's upstream variable face defaults to weight
200, but the layout engine requests normal 400 or bold 700 explicitly.

## Provenance and licenses

- Arimo comes from
  [googlefonts/Arimo at `4a6255f`](https://github.com/googlefonts/Arimo/tree/4a6255f269916ae7ad3fc2706b0935e7621396b8/fonts/variable).
- Tinos font bytes come from
  [google/fonts at `29b4ec6`](https://github.com/google/fonts/tree/29b4ec6df2522a8aee279414b78a07910b065874/ofl/tinos).
  That directory's metadata names source commit `3b4482a`; its missing license
  file is supplied from the matching
  [official Tinos source](https://github.com/googlefonts/tinos/blob/3b4482a99b80ea5fc75f187b1be3120a3f5905b3/OFL.txt).
- Noto Sans Arabic, Devanagari, Hebrew and Noto Emoji come from the same pinned
  [google/fonts commit](https://github.com/google/fonts/tree/29b4ec6df2522a8aee279414b78a07910b065874/ofl).
- The 14 added faces for Carlito, Caladea, Source Sans 3, EB Garamond and
  JetBrains Mono come from
  [google/fonts at `cc39c4c`](https://github.com/google/fonts/tree/cc39c4c2c304bc368c9b7c311ea2bcc6013299fe/ofl).
  Every exact file path, Git blob ID and SHA-256 is pinned in `manifest.json`.

All eleven families use **SIL Open Font License 1.1**. Each complete upstream
notice is retained as `*.OFL.txt`; `NOTICE.txt` combines those notices for
distribution with the application. Some added OFL notices contain Reserved Font Name declarations; the
unmodified upstream font bytes retain their original names, as permitted.
The other OFL redistribution and notice conditions still apply. Keep the copyright and full license when
redistributing or embedding these fonts, including in a WebAssembly bundle.
The production application should make `NOTICE.txt` available with its assets.
These third-party licenses do not select the Office engine's pending license.
No font has been subsetted, internally renamed or otherwise modified.

## Coverage and limitations

Arimo and Tinos provide the original sans-serif and serif measurements, including
Latin, Greek, Cyrillic and Hebrew coverage. Dedicated Noto faces provide Arabic,
Devanagari and Hebrew shaping support. Noto Emoji provides monochrome outlines;
it is **not** Noto Color Emoji. The catalog does not supply color glyph rendering,
CJK fonts, every Unicode script, every future emoji or every requested family.
Missing glyphs and family/style substitutions must be explicit layout diagnostics.
Never fetch a host font to hide a missing-glyph result.

Arial/Helvetica-to-Arimo and Times New Roman-to-Tinos mappings are substitutions,
not proof that proprietary fonts are installed. Preserve the document's requested
family independently from the resolved font. These metrics and fonts do not
establish Microsoft Word pagination parity.

The original ten pinned font files parsed successfully with FontTools 4.61.1
checksum validation before this expansion. The 14 additional files have their
unchanged upstream blob IDs and SHA-256 recorded in `manifest.json`; full
native/WASM shaping regression results remain a separate release check.
An isolated Parley 0.13.0/Skrifa 0.49.0 probe using only these files passed four
native tests: mixed-script shaping and outline extraction; several emoji ZWJ,
regional-indicator, keycap and skin-tone sequences; explicit missing CJK glyphs;
and genuinely different Latin-family advances with kerning. For example,
woman-astronaut and family sequences shaped into one glyph;
the keycap sequence used two glyphs. This is not a claim that characters equal
glyphs, nor an exhaustive script/emoji certification. The selected ICU data also
reports no Chinese/Japanese segmentation model; CJK is unsupported for both
catalog coverage and this initial line-breaking profile. WASM layout parity and
the engine's regression suite are verified separately.

The two original Latin families contain GPOS kerning and mark positioning. Their Latin
ligatures are discretionary (`dlig`), not the default `liga` feature; ordinary
`office` text must not be assumed to ligate. Noto Sans Arabic includes standard
and required ligature features. Do not enable discretionary typography silently
to make a glyph-count test pass.

## Verification and acquisition

From the repository root, use Python's standard library to verify every pinned
font and license, plus the combined notices:

```sh
python3 crates/office-layout/fonts/verify.py
```

Or, from this directory:

```sh
sha256sum --check SHA256SUMS
```

`python3 crates/office-layout/fonts/verify.py --download` reacquires only the
manifest's immutable official URLs, checks byte counts, upstream Git blob hashes
and SHA-256 before replacing files, regenerates the combined notices and verifies
the result. It is a maintenance command, not a build or runtime requirement.
No FontTools dependency is needed by that command or the application.

Changing any *existing* family or the legacy fallback policy requires a
layout-profile version change, a fresh upstream/license review and new golden
layouts. This additive batch keeps `writer-fonts-v1` as the baseline profile:
all original faces, default Arimo and old fallback order remain unchanged.
New fonts must be explicitly chosen, and their precise files are identified
independently by manifest SHA-256. Native/WASM/visual regressions must still
be run before treating the expansion as production-verified. Pinned data and checksum checks are not a
font-security certification; continue using maintained, bounded font parsers.
