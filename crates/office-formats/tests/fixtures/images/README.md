These small PNG/JPEG resources were generated procedurally for image regression
tests with Pillow. They contain four solid quadrants: red, green, blue and orange.
The PNG green quadrant is half transparent and the orange quadrant is fully
transparent. The ordinary JPEG is opaque. `oriented.jpg` adds EXIF orientation 6
to the same JPEG pixels. No third-party image artwork is included.
`color-approx.png` adds a valid nonstandard gAMA value of 50000 to the PNG,
exercising explicit color-approximation notices without a color-management engine.

The adjacent `../project-review-images.writer` fixture embeds all three original
files. It exercises two sections/pages, Unicode descriptions, alignment and native
byte preservation. `../../verify-image-consumers.py` checks our exported packages
and actual LibreOffice/Poppler output without importing the Rust engine.
