# Graphe image regression fixtures

These small test images were generated for this repository with Pillow. They
contain synthetic colored quadrants and no third-party artwork or personal data.

- `quadrants.png`: 320 × 160 RGBA. Top left red, top right green, bottom left
  blue, bottom right translucent gold. Exercises aspect ratio and alpha.
- `phone-orientation-6.jpg`: 120 × 60 RGB JPEG with EXIF orientation 6. Its
  presented dimensions must be 60 × 120; top left is blue after clockwise rotation.
- `repeated-ifd.png`: valid PNG pixels with a 68-byte synthetic EXIF/TIFF graph.
  IFD0 has orientation 1 and two ExifIFDPointer entries pointing at the same IFD
  with an ExifVersion field. The bounded fixture reproduces the repeated-offset
  condition behind GHSA-53gf-vqf6-qvp8 without allocating a large graph. Strict
  `kamadak-exif` 0.6.2 must reject it; document content/history remain unchanged.
- `color-approx.png`: copied from the repository's generated
  `crates/office-formats/tests/fixtures/images/color-approx.png`. Its non-sRGB
  gamma metadata exercises an immediate visible normalization notice while
  preserving exact original image bytes.

Malformed, active SVG, and oversized-file cases are generated in the browser test
from small buffers; no large or active external file needs to be committed.
