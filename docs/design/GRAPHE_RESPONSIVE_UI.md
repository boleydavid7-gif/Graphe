# Graphe responsive Writing First interface

## Design decision
Reuse existing four-section ribbon and Rust-rendered page. Keep the ribbon in one 52px horizontal lane on mobile; its More button exposes the existing advanced commands in the same lane instead of creating vertical rows. Add four bottom tools: Outline (named Heading/Title paragraphs), Format (existing inspector in a bottom sheet), Find and Preview. Keep file, save, import/export and image/table commands as implemented. Do not change engine document schema, input projection, or pagination.

## Theme
Standalone Graphe uses opaque navy/ink Lamplight with restrained gold. When inside ProairetOS, the trusted existing theme bridge supplies the user's semantic system palette. Only app chrome, focus and selection/caret colors adapt. Document ink/paper/print formatting remain document-owned.

## Reliability and tests
Outline navigation passes a selection at offset zero through Rust's set_selection, never measures page DOM to derive offsets. Keep editing, composition, keyboard, autosave and unsupported-file guards intact. Run typecheck, lint, unit, browser/mobile and theme embedding tests, and production build in a runner. Direct connector writes cannot independently execute these checks; physical iOS/Android interaction remains a separate validation step.
