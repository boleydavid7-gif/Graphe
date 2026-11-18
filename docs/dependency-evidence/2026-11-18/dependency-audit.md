# Graphe dependency audit — generated evidence, NOT release clearance

Generated: 2026-11-18T10:44:09.848Z

The underlying lockfile is authoritative for package identities, NOT for license terms.
Cargo metadata supplies upstream declarations; license texts, distribution, provenance
and final compiled contents still require human review.

## Summary

- **source:** Committed Cargo.lock and apps/writer/package-lock.json
- **cargoLockSha256:** a3ba6afc9227a2b8d3287d981b7fa57ef1c6b69fc8cdbd9a6ea44fb0e75ac4a9
- **npmLockSha256:** a2138060a1f23b7dfd9237d8d9f61d2b0444243fb6623f34cb6f488d3ffec729
- **metadataComplete:** true
- **graphScope:** normal source dependencies; optimized binary contribution is not established
- **wasmRoot:** office-wasm
- **wasmRuntimeExternalRecords:** 91
- **wasmNormalWithHostMacrosExternalRecords:** 106
- **rustPackageRecords:** 128
- **externalRustPackageRecords:** 123
- **npmPackageRecords:** 212
- **npmProductionRecords:** 3
- **missingCargoLicenseMetadataRecords:** 6
- **potentiallyReciprocalRustLicenses:** 0
- **rustWithoutExactVersionMentionsInThreeSubsetNotices:** 104
- **manualRTFLicensingEvidence:** rclip-rtf, rclip-core
- **targetsAnalyzed:** wasm32-unknown-unknown, x86_64-unknown-linux-gnu

## Remaining evidence issues

- Missing/resolution-dependent Cargo license metadata: 6
- Potentially reciprocal Rust license declarations: 0
- `rclip-rtf` and `rclip-core` need exact upstream license-file evidence.
- A match in an existing notice file is a *search hint*, not proof that notice obligations are met.
- npm lockfile licenses are package declarations, not final produced-asset review.

| Cargo package | Declared license (metadata only) | Reachability | Subset notice reference | Review status |
| --- | --- | --- | --- | --- |
| `adler2@2.0.1` | 0BSD OR MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `arrayvec@0.7.8` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-formats/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `autocfg@1.5.1` | Apache-2.0 OR MIT | (not established) | crates/office-formats/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `base64@0.23.1` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `bitflags@2.13.2` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `block-buffer@0.12.1` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-layout/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `bumpalo@3.20.4` | MIT OR Apache-2.0 | (not established) | — | DECLARATION_ONLY_NOT_CLEARED |
| `bytemuck@1.25.2` | Zlib OR Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `bytemuck_derive@1.12.1` | Zlib OR Apache-2.0 OR MIT | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `cfg-if@1.0.5` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `cfg_aliases@0.2.2` | (not verified) | (not established) | — | MISSING_METADATA_OR_NOT_RESOLVED |
| `constant_time_eq@0.6.1` | CC0-1.0 OR MIT-0 OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `core_maths@0.1.1` | MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `cpufeatures@0.3.1` | MIT OR Apache-2.0 | x86_64-unknown-linux-gnu | crates/office-layout/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `crc32fast@1.5.5` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `crypto-common@0.2.2` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-layout/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `digest@0.11.3` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-layout/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `displaydoc@0.2.7` | MIT OR Apache-2.0 | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `equivalent@1.0.3` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `errno@0.3.15` | MIT OR Apache-2.0 | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `euclid@0.22.14` | MIT OR Apache-2.0 | x86_64-unknown-linux-gnu | crates/office-formats/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `fastrand@2.5.0` | Apache-2.0 OR MIT | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `fdeflate@0.3.7` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `flate2@1.1.11` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `foldhash@0.2.0` | Zlib | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `font-types@0.12.7` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `fontique@0.13.0` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `getrandom@0.4.3` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `harfrust@0.15.0` | MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `hashbrown@0.17.1` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `hybrid-array@0.4.15` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-layout/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `icu_collections@2.3.0` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `icu_locale_core@2.4.0` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `icu_locale_fallback@2.3.0` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `icu_locale_fallback_data@2.3.0` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `icu_normalizer@2.3.0` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `icu_normalizer_data@2.3.0` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `icu_properties@2.3.0` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `icu_properties_data@2.3.0` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `icu_provider@2.4.0` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `icu_segmenter@2.3.0` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `icu_segmenter_data@2.3.0` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `indexmap@2.14.2` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `itoa@1.0.19` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `js-sys@0.3.107` | (not verified) | (not established) | — | MISSING_METADATA_OR_NOT_RESOLVED |
| `kamadak-exif@0.6.2` | BSD-2-Clause | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `kurbo@0.13.1` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-formats/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `libc@0.2.191` | MIT OR Apache-2.0 | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `libm@0.2.16` | MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `linebender_resource_handle@0.1.1` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `linux-raw-sys@0.12.1` | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `litemap@0.8.3` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `memchr@2.8.3` | Unlicense OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `miniz_oxide@0.8.9` | MIT OR Zlib OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `miniz_oxide@0.9.1` | MIT OR Zlib OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `mutate_once@0.1.2` | BSD-2-Clause | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `nix@0.31.3` | (not verified) | (not established) | — | MISSING_METADATA_OR_NOT_RESOLVED |
| `num-traits@0.2.19` | MIT OR Apache-2.0 | x86_64-unknown-linux-gnu | crates/office-formats/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `once_cell@1.21.4` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `parlance@0.1.1` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `parley@0.13.0` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `parley_data@0.13.0` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `parley_emoji@0.13.0` | MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `parley_engine@0.13.0` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `pdf-writer@0.15.0` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-formats/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `png@0.18.1` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `polycool@0.4.0` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-formats/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `potential_utf@0.1.6` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `proc-macro2@1.0.107` | MIT OR Apache-2.0 | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `quick-xml@0.42.0` | MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `quote@1.0.47` | MIT OR Apache-2.0 | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `r-efi@6.0.0` | (not verified) | (not established) | — | MISSING_METADATA_OR_NOT_RESOLVED |
| `rclip-core@0.1.0` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-formats/THIRD_PARTY_NOTICES.txt | UPSTREAM_LICENSE_FILE_EVIDENCE_REQUIRED |
| `rclip-rtf@0.1.0` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-formats/THIRD_PARTY_NOTICES.txt | UPSTREAM_LICENSE_FILE_EVIDENCE_REQUIRED |
| `read-fonts@0.46.0` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `roxmltree@0.21.2` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `rustix@1.1.6` | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `rustversion@1.0.23` | MIT OR Apache-2.0 | (not established) | — | DECLARATION_ONLY_NOT_CLEARED |
| `ryu@1.0.23` | Apache-2.0 OR BSL-1.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-formats/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `serde@1.0.229` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `serde_core@1.0.229` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `serde_derive@1.0.229` | MIT OR Apache-2.0 | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `serde_json@1.0.154` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `sha2@0.11.1` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-layout/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `simd-adler32@0.3.10` | MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `siphasher@1.0.4` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-formats/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `skrifa@0.49.0` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `smallvec@1.16.5` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `spin@0.12.3` | MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `stable_deref_trait@1.2.1` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `svgtypes@0.16.2` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-formats/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `syn@2.0.119` | MIT OR Apache-2.0 | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `syn@3.0.10` | MIT OR Apache-2.0 | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `synstructure@0.14.0` | MIT | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `tempfile@3.27.0` | MIT OR Apache-2.0 | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `tinystr@0.8.5` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `typed-path@0.13.0` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `typenum@1.20.1` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | crates/office-layout/THIRD_PARTY_NOTICES.txt | DECLARATION_ONLY_NOT_CLEARED |
| `unicode-ident@1.0.27` | (MIT OR Apache-2.0) AND Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `unicode-segmentation@1.13.3` | MIT OR Apache-2.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `utf8_iter@1.1.0` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `utf_types@1.0.0` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `wasm-bindgen@0.2.130` | MIT OR Apache-2.0 | wasm32-unknown-unknown | — | DECLARATION_ONLY_NOT_CLEARED |
| `wasm-bindgen-macro@0.2.130` | MIT OR Apache-2.0 | (not established) | — | DECLARATION_ONLY_NOT_CLEARED |
| `wasm-bindgen-macro-support@0.2.130` | MIT OR Apache-2.0 | (not established) | — | DECLARATION_ONLY_NOT_CLEARED |
| `wasm-bindgen-shared@0.2.130` | MIT OR Apache-2.0 | wasm32-unknown-unknown | — | DECLARATION_ONLY_NOT_CLEARED |
| `windows-link@0.2.1` | (not verified) | (not established) | — | MISSING_METADATA_OR_NOT_RESOLVED |
| `windows-sys@0.61.2` | (not verified) | (not established) | — | MISSING_METADATA_OR_NOT_RESOLVED |
| `writeable@0.6.4` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `yoke@0.8.4` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `yoke-derive@0.8.4` | Unicode-3.0 | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `zerofrom@0.1.8` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `zerofrom-derive@0.1.8` | Unicode-3.0 | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `zeroize@1.9.1` | Apache-2.0 OR MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `zeroize_derive@1.5.0` | Apache-2.0 OR MIT | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `zerotrie@0.2.5` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `zerovec@0.11.9` | Unicode-3.0 | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `zerovec-derive@0.11.7` | Unicode-3.0 | x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `zip@9.5.1` | MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `zlib-rs@0.6.8` | Zlib | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `zmij@1.0.23` | MIT | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `zune-core@0.5.3` | MIT OR Apache-2.0 OR Zlib | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |
| `zune-jpeg@0.5.15` | MIT OR Apache-2.0 OR Zlib | wasm32-unknown-unknown, x86_64-unknown-linux-gnu | — | DECLARATION_ONLY_NOT_CLEARED |

## npm (lockfile declarations)

| npm package | License | Phase |
| --- | --- | --- |
| `@babel/code-frame@7.29.7` | MIT | development |
| `@babel/compat-data@7.29.7` | MIT | development |
| `@babel/core@7.29.13` | MIT | development |
| `@babel/generator@7.29.14` | MIT | development |
| `@babel/helper-compilation-targets@7.29.13` | MIT | development |
| `@babel/helper-globals@7.29.7` | MIT | development |
| `@babel/helper-module-imports@7.29.7` | MIT | development |
| `@babel/helper-module-transforms@7.29.14` | MIT | development |
| `@babel/helper-string-parser@7.29.14` | MIT | development |
| `@babel/helper-validator-identifier@7.29.7` | MIT | development |
| `@babel/helper-validator-option@7.29.7` | MIT | development |
| `@babel/helpers@7.29.14` | MIT | development |
| `@babel/parser@7.29.14` | MIT | development |
| `@babel/template@7.29.7` | MIT | development |
| `@babel/traverse@7.29.14` | MIT | development |
| `@babel/types@7.29.14` | MIT | development |
| `@cacheable/memory@2.2.0` | MIT | development |
| `@cacheable/utils@2.5.0` | MIT | development |
| `@emnapi/core@2.0.0-beta.2` | MIT | development (optional) |
| `@emnapi/runtime@2.0.0-beta.2` | MIT | development (optional) |
| `@emnapi/wasi-threads@2.2.0` | MIT | development (optional) |
| `@eslint-community/eslint-utils@4.10.1` | MIT | development |
| `@eslint-community/eslint-utils/node_modules/eslint-visitor-keys@3.4.3` | Apache-2.0 | development |
| `@eslint-community/regexpp@4.12.3` | MIT | development |
| `@eslint/config-array@0.23.6` | Apache-2.0 | development |
| `@eslint/config-helpers@0.7.0` | Apache-2.0 | development |
| `@eslint/core@1.2.1` | Apache-2.0 | development |
| `@eslint/js@10.0.1` | MIT | development |
| `@eslint/object-schema@3.0.5` | Apache-2.0 | development |
| `@eslint/plugin-kit@0.7.3` | Apache-2.0 | development |
| `@humanfs/core@0.19.2` | Apache-2.0 | development |
| `@humanfs/node@0.16.8` | Apache-2.0 | development |
| `@humanfs/types@0.15.0` | Apache-2.0 | development |
| `@humanwhocodes/module-importer@1.0.1` | Apache-2.0 | development |
| `@humanwhocodes/retry@0.4.3` | Apache-2.0 | development |
| `@isaacs/nonrecursive-glob-match@1.1.0` | BlueOak-1.0.0 | development |
| `@jridgewell/gen-mapping@0.3.13` | MIT | development |
| `@jridgewell/remapping@2.3.5` | MIT | development |
| `@jridgewell/resolve-uri@3.1.3` | MIT | development |
| `@jridgewell/sourcemap-codec@1.6.0` | MIT | development |
| `@jridgewell/trace-mapping@0.3.31` | MIT | development |
| `@keyv/bigmap@1.3.1` | MIT | development |
| `@keyv/serialize@1.1.1` | MIT | development |
| `@napi-rs/wasm-runtime@1.2.6` | MIT | development (optional) |
| `@oxc-project/types@0.156.0` | MIT | development |
| `@playwright/test@1.65.1` | Apache-2.0 | development |
| `@rolldown/binding-android-arm-eabi@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-android-arm64@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-darwin-arm64@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-darwin-x64@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-freebsd-x64@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-linux-arm-gnueabihf@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-linux-arm64-gnu@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-linux-arm64-musl@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-linux-ppc64-gnu@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-linux-s390x-gnu@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-linux-x64-gnu@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-linux-x64-musl@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-openharmony-arm64@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-wasm32-wasi@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-win32-arm64-msvc@1.2.19` | MIT | development (optional) |
| `@rolldown/binding-win32-x64-msvc@1.2.19` | MIT | development (optional) |
| `@rolldown/pluginutils@1.1.1` | MIT | development |
| `@tybys/wasm-util@0.10.4` | MIT | development (optional) |
| `@types/chai@5.2.3` | MIT | development |
| `@types/deep-eql@4.0.2` | MIT | development |
| `@types/esrecurse@4.3.1` | MIT | development |
| `@types/estree@1.0.10` | MIT | development |
| `@types/json-schema@7.0.15` | MIT | development |
| `@types/node@24.19.2` | MIT | development |
| `@types/react@19.3.0` | MIT | development |
| `@types/react-dom@19.3.0` | MIT | development |
| `@typescript-eslint/eslint-plugin@8.72.2` | MIT | development |
| `@typescript-eslint/eslint-plugin/node_modules/ignore@7.0.14` | MIT | development |
| `@typescript-eslint/parser@8.72.2` | MIT | development |
| `@typescript-eslint/project-service@8.72.2` | MIT | development |
| `@typescript-eslint/scope-manager@8.72.2` | MIT | development |
| `@typescript-eslint/tsconfig-utils@8.72.2` | MIT | development |
| `@typescript-eslint/type-utils@8.72.2` | MIT | development |
| `@typescript-eslint/types@8.72.2` | MIT | development |
| `@typescript-eslint/typescript-estree@8.72.2` | MIT | development |
| `@typescript-eslint/typescript-estree/node_modules/semver@7.8.6` | ISC | development |
| `@typescript-eslint/utils@8.72.2` | MIT | development |
| `@typescript-eslint/visitor-keys@8.72.2` | MIT | development |
| `@vitejs/plugin-react@6.3.0` | MIT | development |
| `@vitest/mocker@5.1.0` | MIT | development |
| `@vitest/spy@5.1.0` | MIT | development |
| `acorn@8.19.0` | MIT | development |
| `acorn-jsx@5.3.2` | MIT | development |
| `ajv@6.15.0` | MIT | development |
| `assertion-error@2.0.1` | MIT | development |
| `balanced-match@4.0.4` | MIT | development |
| `baseline-browser-mapping@2.11.37` | Apache-2.0 | development |
| `brace-expansion@5.0.13` | MIT | development |
| `browserslist@4.29.5` | MIT | development |
| `cacheable@2.5.0` | MIT | development |
| `caniuse-lite@1.0.30001826` | CC-BY-4.0 | development |
| `chai@6.3.0` | MIT | development |
| `convert-source-map@2.0.0` | MIT | development |
| `cross-spawn@7.0.6` | MIT | development |
| `csstype@3.2.3` | MIT | development |
| `debug@4.4.3` | MIT | development |
| `deep-is@0.1.4` | MIT | development |
| `detect-libc@2.1.2` | Apache-2.0 | development |
| `electron-to-chromium@1.5.485` | ISC | development |
| `es-module-lexer@2.3.2` | MIT | development |
| `escalade@3.2.0` | MIT | development |
| `escape-string-regexp@4.0.0` | MIT | development |
| `eslint@10.15.0` | MIT | development |
| `eslint-plugin-react-hooks@7.1.1` | MIT | development |
| `eslint-plugin-react-refresh@0.5.7` | MIT | development |
| `eslint-scope@9.1.2` | BSD-2-Clause | development |
| `eslint-visitor-keys@5.0.1` | Apache-2.0 | development |
| `espree@11.2.0` | BSD-2-Clause | development |
| `esquery@1.7.0` | BSD-3-Clause | development |
| `esrecurse@4.3.0` | BSD-2-Clause | development |
| `estraverse@5.3.0` | BSD-2-Clause | development |
| `estree-walker@3.0.3` | MIT | development |
| `esutils@2.0.3` | BSD-2-Clause | development |
| `expect-type@1.4.0` | Apache-2.0 | development |
| `fake-indexeddb@6.2.8` | Apache-2.0 | development |
| `fast-deep-equal@3.1.3` | MIT | development |
| `fast-json-stable-stringify@2.1.0` | MIT | development |
| `fast-levenshtein@2.0.6` | MIT | development |
| `fdir@6.5.0` | MIT | development |
| `file-entry-cache@11.1.5` | MIT | development |
| `find-up@5.0.0` | MIT | development |
| `flat-cache@6.1.23` | MIT | development |
| `flatted@3.4.4` | ISC | development |
| `fsevents@2.3.3` | MIT | development (optional) |
| `gensync@1.0.0-beta.2` | MIT | development |
| `glob-parent@6.0.2` | ISC | development |
| `hashery@1.5.1` | MIT | development |
| `hermes-estree@0.25.1` | MIT | development |
| `hermes-parser@0.25.1` | MIT | development |
| `hookified@1.15.1` | MIT | development |
| `ignore@5.3.2` | MIT | development |
| `imurmurhash@0.1.4` | MIT | development |
| `is-extglob@2.1.1` | MIT | development |
| `is-glob@4.0.3` | MIT | development |
| `isexe@2.0.0` | ISC | development |
| `js-tokens@4.0.0` | MIT | development |
| `jsesc@3.1.0` | MIT | development |
| `json-schema-traverse@0.4.1` | MIT | development |
| `json-stable-stringify-without-jsonify@1.0.1` | MIT | development |
| `json5@2.2.3` | MIT | development |
| `keyv@5.6.0` | MIT | development |
| `levn@0.4.1` | MIT | development |
| `lightningcss@1.33.0` | MPL-2.0 | development |
| `lightningcss-android-arm64@1.33.0` | MPL-2.0 | development (optional) |
| `lightningcss-darwin-arm64@1.33.0` | MPL-2.0 | development (optional) |
| `lightningcss-darwin-x64@1.33.0` | MPL-2.0 | development (optional) |
| `lightningcss-freebsd-x64@1.33.0` | MPL-2.0 | development (optional) |
| `lightningcss-linux-arm-gnueabihf@1.33.0` | MPL-2.0 | development (optional) |
| `lightningcss-linux-arm64-gnu@1.33.0` | MPL-2.0 | development (optional) |
| `lightningcss-linux-arm64-musl@1.33.0` | MPL-2.0 | development (optional) |
| `lightningcss-linux-x64-gnu@1.33.0` | MPL-2.0 | development (optional) |
| `lightningcss-linux-x64-musl@1.33.0` | MPL-2.0 | development (optional) |
| `lightningcss-win32-arm64-msvc@1.33.0` | MPL-2.0 | development (optional) |
| `lightningcss-win32-x64-msvc@1.33.0` | MPL-2.0 | development (optional) |
| `locate-path@6.0.0` | MIT | development |
| `lru-cache@5.1.1` | ISC | development |
| `magic-string@1.4.5` | MIT | development |
| `minimatch@11.0.8` | BlueOak-1.0.0 | development |
| `ms@2.1.3` | MIT | development |
| `nanoid@3.3.20` | MIT | development |
| `natural-compare@1.4.0` | MIT | development |
| `node-releases@2.0.64` | MIT | development |
| `obug@3.0.0` | MIT | development |
| `optionator@0.9.4` | MIT | development |
| `p-limit@3.1.0` | MIT | development |
| `p-locate@5.0.0` | MIT | development |
| `path-exists@4.0.0` | MIT | development |
| `path-key@3.1.1` | MIT | development |
| `picocolors@1.1.1` | ISC | development |
| `picomatch@4.0.7` | MIT | development |
| `playwright@1.65.1` | Apache-2.0 | development |
| `playwright-core@1.65.1` | Apache-2.0 | development |
| `postcss@8.5.33` | MIT | development |
| `prelude-ls@1.2.1` | MIT | development |
| `punycode@2.3.1` | MIT | development |
| `qified@0.10.1` | MIT | development |
| `qified/node_modules/hookified@2.2.0` | MIT | development |
| `react@19.3.1` | MIT | production |
| `react-dom@19.3.1` | MIT | production |
| `rolldown@1.2.19` | MIT | development |
| `scheduler@0.28.0` | MIT | production |
| `semver@6.3.2` | ISC | development |
| `shebang-command@2.0.0` | MIT | development |
| `shebang-regex@3.0.0` | MIT | development |
| `source-map-js@1.2.2` | BSD-3-Clause | development |
| `std-env@4.3.0` | MIT | development |
| `tinybench@6.2.3` | MIT | development |
| `tinyexec@1.3.1` | MIT | development |
| `tinyglobby@0.2.17` | MIT | development |
| `ts-api-utils@2.5.0` | MIT | development |
| `tslib@2.9.1` | 0BSD | development (optional) |
| `type-check@0.4.0` | MIT | development |
| `typescript@6.0.3` | Apache-2.0 | development |
| `typescript-eslint@8.72.2` | MIT | development |
| `undici-types@7.24.6` | MIT | development |
| `update-browserslist-db@1.3.4` | MIT | development |
| `uri-js@4.4.1` | BSD-2-Clause | development |
| `vite@8.3.8` | MIT | development |
| `vitest@5.1.0` | MIT | development |
| `which@2.0.2` | ISC | development |
| `why-is-node-running@3.2.1` | MIT | development |
| `word-wrap@1.2.5` | MIT | development |
| `yallist@3.1.1` | ISC | development |
| `yocto-queue@0.1.0` | MIT | development |
| `zod@4.6.7` | MIT | development |
| `zod-validation-error@4.0.2` | MIT | development |

## Release gating

This report is not a consolidated redistributable license notice, a legal opinion,
or a verified SBOM of the final binaries. See `docs/OPEN_SOURCE_RELEASE.md`.
