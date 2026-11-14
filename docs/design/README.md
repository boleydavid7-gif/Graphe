# Approved Lamplight Writer references

These are the owner's Step 7 design references, supplied in this conversation.
Use them as the primary visual references rather than replacing Writer with a
generic word processor shell:

- [Desktop reference](writer-lamplight-reference.png), 1280 × 720.
- [Rough mobile reference](writer-lamplight-mobile-reference.png), 853 × 1280.

The supplied attachments were JPEG files. They were decoded and saved as PNG at
the requested repository paths; dimensions, color mode and every decoded pixel
were compared and remained identical. These files are references, not screenshots
of implemented functionality.

## Implementation priorities

Preserve the dark warm-brown/charcoal hierarchy, gold lantern and active-tab
accent, compact header, editable title, four Write/Insert/Layout/Review tabs,
compact formatting controls, centered warm-white paper and minimal status/zoom
bar. Mobile moves the title to a second row, keeps essential formatting visible
and uses a small document-action bar. Desktop remains the primary experience.

The owner's written constraints govern details where the images suggest
decorative effects: use opaque flat surfaces, restrained rounding and shadows,
without glass, transparency, decorative gradients, oversized cards or unnecessary
animation. Do not recreate the phone's operating-system clock, battery or shell.

Only working commands belong in the interface. Reuse existing Rust operations
rather than adding advanced features to fill tabs. Device-local save status must
not imply cloud synchronization; proprietary fonts must not be claimed as bundled.
Tables, images, comments, collaboration and complex style authoring remain future
work. A named style already present in a document may be assigned using the
existing core operation, without adding a style editor.

Document meaningful deviations in the Step 7 verification report. Capture actual
desktop and mobile application screenshots separately from these references.
After Step 7 approval, pause feature development for the owner's planned
technical, security, DOCX-compatibility and commercial-licensing audit.
