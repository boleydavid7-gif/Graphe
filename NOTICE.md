# Licensing and third-party notices

**Status: private open-source preparation, not a cleared public release.**

The planned license for original Graphe application and Office Engine source is the
[Mozilla Public License 2.0](LICENSE), subject to the release-readiness review.
This does not relicense third-party files, code or assets.

The following independently licensed resources are included in the repository:

- **Bundled fonts:** 42 original TTF files from 21 font families, under the SIL Open Font License 1.1. Individual full notices are under `crates/office-layout/fonts/*.OFL.txt`, collected in `crates/office-layout/fonts/NOTICE.txt`, and indexed with checksums and upstream provenance in `crates/office-layout/fonts/manifest.json`. Any reserved font names and other license conditions continue to apply.
- **Format dependencies:** `crates/office-formats/THIRD_PARTY_NOTICES.txt`.
- **Font-loading dependencies:** `crates/office-layout/THIRD_PARTY_NOTICES.txt`.
- **Image processing dependencies:** `crates/office-core/THIRD_PARTY_NOTICES.txt`.
- **Other dependencies:** `docs/DEPENDENCIES.md` describes a dated inventory, but **does not yet provide complete, verified release-time attribution for every Rust/WASM dependency**.
- **Test documents and externally sourced examples:** retain their original owners' rights. They require separate provenance and privacy review; they are **not automatically relicensed** as part of the Graphe source license.
- **Logos and branding:** source-code licensing does not grant trademarks. Review individual artwork rights and [TRADEMARKS.md](TRADEMARKS.md).

Graphe's current browser **About** dialog links the software license and the existing format, font, image and font-loading notices. The public-release audit must verify those links against the final emitted and served artifacts, and add any missing third-party runtime notices.

[Open-source release gates](docs/OPEN_SOURCE_RELEASE.md) · [Commercial licensing/IP audit](docs/COMMERCIAL_LICENSING_AUDIT.md)
