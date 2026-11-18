# Graphe Step 8A: images — research and retrospective

Research date: 2026-11-18. This records the implementation strategy and acceptance
risks, rather than claiming that unexecuted tests or deployment checks passed.
See the final implementation report for measured results.

## Scope and architecture

The first profile is embedded, static PNG/JPEG images in the document body,
represented as format-neutral blocks with stable identities, explicit physical
dimensions and alternative text. Rust owns validation, editing transactions,
selection, image geometry and pagination. React renders the supplied geometry
and translates input; it does not infer document dimensions from browser image
layout. No remote resource fetching, SVG, animation, floating/wrapped images,
cropping, rotation controls, nested objects or image editing is implied.

Keep exact original image bytes in the native document. Decode once into a
bounded immutable asset, retain a canonical oriented RGBA/PNG presentation, and
share that runtime asset across document clones and undo snapshots. Runtime
pixels are derived data, not a second persisted source of truth. Browser and PDF
must use the same pixels. Original bytes may retain EXIF/location information;
native-file preservation must not be described as metadata removal.

Support images through the existing format adapters. A DOCX relationship,
DrawingML element or ODF frame must never become a core model type. The native
schema/envelope needs an explicit version increment and strict legacy decoding;
image fields must not be accepted under an older version label. Source-bearing
native files continue to preserve their complete original document independently.

## Codec alternatives and licensing

Published registry archives and upstream API/source documentation were reviewed.
The dependency lockfile and actual bundled notices must identify the versions
ultimately resolved; this review does not select a license for Graphe itself.

| Library evaluated | License and Rust floor | Decision |
| --- | --- | --- |
| `png` 0.18.1 | MIT OR Apache-2.0; Rust 1.73 | Narrow PNG decoder/encoder, bounds and CRC controls; selected. |
| `zune-jpeg` 0.5.15 / `zune-core` 0.5.3 | MIT OR Apache-2.0 OR Zlib; Rust 1.75 | Pure Rust JPEG decoding with strict mode and size/scan controls; selected without default architecture-specific features. |
| `kamadak-exif` 0.6.2 | BSD-2-Clause; Rust 1.60 | Mature TIFF/EXIF parsing for orientation; patched release selected over a handwritten TIFF parser. Its `mutate_once` dependency is also BSD-2-Clause. |
| `image` 0.25.10 | MIT OR Apache-2.0 | Useful general-purpose API, but broader operations and unconditional `moxcms` dependency; unnecessary for this bounded PNG/JPEG profile. Default features also enable many codecs. |

The low-level choices avoid a complete graphics/editor engine and retain native
and `wasm32-unknown-unknown` portability without host codecs. Preserve complete
upstream license texts and notices for every newly resolved dependency, including
transitives. Release cadence alone does not establish maintenance or safety.

Final registry-archive review identified a critical integration risk in the
initially evaluated 0.6.1: [GHSA-53gf-vqf6-qvp8](https://github.com/kamadak/exif-rs/security/advisories/GHSA-53gf-vqf6-qvp8)
affects versions through 0.6.1, including `Reader::read_raw`. Repeated IFD pointers
can expand a small bounded TIFF input into cubic memory usage. A metadata byte
limit and raw-TIFF parsing do not prevent that attack. Version 0.6.2, published
2026-11-05, rejects already visited IFD offsets. The implementation was upgraded
and locked to 0.6.2 before completion, with unchanged BSD-2-Clause license text.
Bounded repeated-IFD regressions exercise rejection through core PNG/JPEG and
browser/WASM import paths without constructing a large allocation attack.

The older [RUSTSEC-2021-0143](https://rustsec.org/advisories/RUSTSEC-2021-0143.html)
PNG-container denial of service was fixed in 0.5.3; it is a separate issue.
Indexed latest-version pages were stale during initial evaluation, so direct
published archive/version review was necessary. This targeted review does not
replace a full current lockfile advisory audit or parser fuzzing.

## Bounded validation and presentation policy

- Bound encoded bytes, dimensions, checked pixel multiplication, decoded working
  memory, per-document cumulative assets, metadata and JPEG scan count. Check
  dimensions before allocating the caller's pixel buffer. A compressed-byte cap
  alone does not prevent decompression exhaustion.
- PNG: configure `DecodeOptions` to verify checksums and disable skipping failed
  ancillary CRCs. Defaults ignore Adler-32 and tolerate bad ancillary CRCs.
  `read_header_info()` permits dimension preflight; decode the complete frame
  and call `finish()` to validate remaining chunks. `Limits` excludes the caller's
  output allocation, which needs its own checked budget.
- Reject APNG (`acTL`/animation state), unsupported critical chunks, malformed
  streams and unsupported JPEG modes explicitly. A successfully decoded first
  animation frame is not preservation of an animated image.
- JPEG: enable strict mode explicitly, set width/height and scan limits, inspect
  headers and checked output size before full RGBA decode. Forgiving decoding
  defaults must not silently repair damaged imported images. A bounded container
  marker pass rejects duplicate EXIF and metadata after the first scan, avoiding
  conflicting interpretations across consumers; it does not replace JPEG decoding.
- Parse bounded raw TIFF metadata with strict `exif::Reader::read_raw`; reject
  malformed/ambiguous orientation. Apply all eight EXIF orientations once in Rust,
  swapping dimensions for orientations 5–8. PNG `eXIf` also requires deliberate
  handling. Test actual corner pixels, not just reported dimensions.
- Canonical presentation uses a single documented RGBA8/color interpretation.
  Browser interpretation of original ICC/gamma/EXIF metadata must not produce
  different display from PDF. If unsupported color profiles or high bit depth
  are approximated, disclose that approximation and preserve originals; do not
  claim color-managed fidelity from stripping metadata or reducing precision.

## Format-specific compatibility

Microsoft's [inline picture example](https://learn.microsoft.com/en-us/office/open-xml/word/how-to-insert-a-picture-into-a-word-processing-document)
uses an image part, an internal relationship, `w:drawing/wp:inline`, explicit
extent, descriptive properties and a DrawingML picture. A first supported subset
can map a standalone picture paragraph to an image block. Validate the actual
relationship type, local part target, content type, decoded media and physical
extent. Do not relax existing ZIP/path/relationship protections or fetch external
links. Floating anchors, mixed text/pictures in one paragraph, crops, transforms,
effects and alternate-content/SVG fallbacks require explicit classification;
accepting a fallback alone can silently omit meaningful semantics.

[ODF 1.3 part 3](https://docs.oasis-open.org/office/OpenDocument/v1.3/os/part3-schema/OpenDocument-v1.3-os-part3-schema.html)
defines `draw:frame` (§10.4.2) and `draw:image` (§10.4.4), frame sizes and text
anchoring. A standalone `as-char` frame with a validated package-local image is
a suitable initial interoperable subset. Inspect all children, anchor behavior
and manifest media types. Unsupported content-bearing frames remain guarded.

DOCX/ODT export must write image relationships/package members through their
adapters and preserve document order. TXT/RTF or another target without this
implementation's image support must not quietly flatten/remove images: provide
a meaningful-loss assessment or refuse the unsupported export. PDF uses Rust
image/page geometry and handles alpha explicitly; its existing outlined-text
search/accessibility limitations remain separate. Retained whole-file originals
are recovery copies, not proof that a regenerated external copy preserved content.

## Retrospective findings and required regression coverage

Reviewed the existing paragraph/table model and traversal, transactional edits,
native versioning, bounded DOCX packaging, layout/PDF paths and browser snapshot
boundary. Targeted extensions are preferable to a replacement editor or renderer.

**Critical integration risks:** body text-range replacement currently protects
tables specifically; images must become structural barriers before insertion is
enabled, so a range around an image cannot silently delete it. Native decoding,
all export traversals, source-aware package validation and current-document
replacement must preserve image content or fail without mutation. Untrusted
dimensions/metadata must be bounded before allocation; animation, orientation
and malformed-file validation cannot be delegated to browser display success.

**Important:** avoid repeated decoding/re-encoding and serializing binary assets
inside every typing snapshot. Cache by immutable identity outside undo payloads;
serve presentation bytes through an origin/revision-checked WASM boundary.
Image selection must remain Rust-owned and coexist with text selections, IME,
mobile focus and clipboard safeguards. Keep image/alt-text DOM nodes out of
paragraph UTF-16 offset counting. Expose accessible alternative text even when
the visual editor uses SVG. Test absence of alt text as a limitation, not as
accessibility compliance.

**Optional/future:** floated/wrapped images, arbitrary inline pictures, cell
images, cropping, resizing gestures, additional codecs, full color management
and richer PDF accessibility need separate bounded designs and research.

Required checks include valid/invalid PNG/JPEG, transparency, EXIF 1–8,
non-square rotated pixels, APNG rejection, CRC/truncation/oversized metadata,
allocation/aggregate limits, duplicate/dangling asset references, atomic failed
transactions, insertion/deletion/history, image-separated text edits, native
save/reopen and strict old-version rejection. Exercise layout/page boundaries,
stable dimensions, DOCX/ODT import–edit–export–reopen, independent consumers and
PDF pixels/alpha. Browser tests must cover insertion, selection, editing around
images, undo/redo, native storage, export and rejected-import preservation.
Emulated mobile browsers do not establish real iOS/Android behavior.

At the reviewed baseline the WASM asset was 25,332,340 bytes. Cloudflare's
[individual static asset limit](https://developers.cloudflare.com/workers/platform/limits/)
is 25 MiB, leaving 882,060 bytes. Measure the final production bundle and fail
an oversized build; codec source size and gzip size do not establish compliance.
The patched production WASM measured 26,031,060 bytes, leaving 183,340 bytes;
future dependency/font changes still require the production asset-size guard.
This observation authorizes no deployment/configuration change.

## Additional primary references

- [PNG Third Edition](https://www.w3.org/TR/png-3/): APNG, image/color metadata and chunk integrity.
- [`png` decoder API](https://docs.rs/png/0.18.1/png/struct.Decoder.html),
  [reader API](https://docs.rs/png/0.18.1/png/struct.Reader.html),
  [upstream](https://github.com/image-rs/image-png).
- [`zune-jpeg` decoder API](https://docs.rs/zune-jpeg/0.5.15/zune_jpeg/struct.JpegDecoder.html),
  [`zune-core` options](https://docs.rs/zune-core/0.5.3/zune_core/options/struct.DecoderOptions.html).
- [`kamadak-exif` upstream](https://github.com/kamadak/exif-rs),
  [0.6.2 release notes](https://docs.rs/crate/kamadak-exif/0.6.2/source/NEWS),
  [published 0.6.2 archive](https://static.crates.io/crates/kamadak-exif/kamadak-exif-0.6.2.crate).
- [`image` upstream](https://github.com/image-rs/image): alternative codec feature/dependency review.
