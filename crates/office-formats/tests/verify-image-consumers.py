"""Independent image-consumer probe, using stdlib and Pillow (no engine import).

Export the adjacent project-review-images.writer as review.odt/docx/pdf with
the export_format example. Use LibreOffice to convert ODT->DOCX into from-odt/,
DOCX->ODT into from-docx/, and each to PDF into odt-pdf/ and docx-pdf/.
Rasterize all three PDFs with `pdftoppm -png -r 72` using prefixes native,
odt and docx in the artifact directory, then pass that directory here.
This verifies real independent consumers, not Microsoft Word certification.
"""

import base64
import io
import json
import posixpath
import sys
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

from PIL import Image, ImageChops, ImageOps

BASE = Path(sys.argv[1])
SOURCE = Path(__file__).parent / "fixtures/project-review-images.writer"
MODEL = json.loads(SOURCE.read_text())["document"]
IMAGES = [block["content"] for section in MODEL["sections"]
          for block in section["blocks"] if block["kind"] == "image"]
NS = {
    "draw": "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0",
    "svg": "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0",
    "xlink": "http://www.w3.org/1999/xlink",
    "a": "http://schemas.openxmlformats.org/drawingml/2006/main",
    "wp": "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
}


def decoded(data):
    return ImageOps.exif_transpose(Image.open(io.BytesIO(data))).convert("RGBA")


def compare_resources(relative):
    with zipfile.ZipFile(BASE / relative) as archive:
        if relative.endswith(".odt"):
            root = ET.fromstring(archive.read("content.xml"))
            frames = root.findall(".//draw:frame", NS)
            resources = [archive.read(frame.find("draw:image", NS).get(
                "{" + NS["xlink"] + "}href").removeprefix("./")) for frame in frames]
            descriptions = [frame.findtext("svg:desc", "", NS) for frame in frames]
        else:
            root = ET.fromstring(archive.read("word/document.xml"))
            rels = ET.fromstring(archive.read("word/_rels/document.xml.rels"))
            targets = {node.get("Id"): posixpath.normpath("word/" + node.get("Target"))
                       for node in rels}
            inlines = root.findall(".//wp:inline", NS)
            resources = [archive.read(targets[inline.find(".//a:blip", NS).get(
                "{" + NS["r"] + "}embed")]) for inline in inlines]
            descriptions = [inline.find("wp:docPr", NS).get("descr", "") for inline in inlines]
    assert len(resources) == 3, (relative, "lost picture")
    for index, (data, description, model) in enumerate(zip(resources, descriptions, IMAGES)):
        original = base64.b64decode(model["asset"]["bytes_base64"])
        expected, actual = decoded(original), decoded(data)
        assert actual.size == expected.size, (relative, index, "orientation/dimensions")
        # The Rust and Pillow JPEG decoders may differ slightly in rounding.
        difference = ImageChops.difference(actual, expected)
        assert max(high for _, high in difference.getextrema()) <= 4, (relative, index, "pixels/alpha")
        assert description == model["alt_text"], (relative, index, "alternative text")
        if relative in ("review.odt", "review.docx") and index < 2:
            assert data == original, (relative, index, "ordinary original raster bytes")
    print(relative, "PASS: three ordered pictures, oriented pixels, alpha and Unicode descriptions")


for path in ["review.odt", "review.docx", "from-docx/review.odt", "from-odt/review.docx"]:
    compare_resources(path)


def close(actual, expected):
    return all(abs(a - e) <= 6 for a, e in zip(actual, expected))


for prefix in ["native", "odt", "docx"]:
    pages = sorted(BASE.glob(prefix + "-*.png"))
    assert len(pages) == 2, (prefix, "pagination")
    page = Image.open(pages[0]).convert("RGB")
    assert page.size == (612, 792), (prefix, "Letter page geometry")
    # Find the first image's opaque upper-left quadrant independently. It must
    # be centered at x=234pt with the configured physical 144x96pt extent.
    red = [(x, y) for y in range(72, 190) for x in range(220, 320)
           if close(page.getpixel((x, y)), (255, 0, 0))]
    assert red, (prefix, "first picture missing")
    x, y = min(p[0] for p in red), min(p[1] for p in red)
    assert abs(x - 234) <= 1, (prefix, "center alignment")
    assert close(page.getpixel((x + 36, y + 24)), (255, 0, 0)), prefix
    assert close(page.getpixel((x + 108, y + 24)), (127, 255, 127)), (prefix, "half alpha")
    assert close(page.getpixel((x + 36, y + 72)), (0, 0, 255)), (prefix, "vertical orientation")
    assert close(page.getpixel((x + 108, y + 72)), (255, 255, 255)), (prefix, "transparent alpha")
    second = Image.open(pages[1]).convert("RGB")
    blue = [(x, y) for y in range(72, 180) for x in range(65, 110)
            if close(second.getpixel((x, y)), (0, 0, 255))]
    assert blue, (prefix, "EXIF picture missing")
    x, y = min(p[0] for p in blue), min(p[1] for p in blue)
    assert abs(x - 72) <= 1, (prefix, "left alignment")
    assert close(second.getpixel((x + 18, y + 27)), (0, 0, 255)), (prefix, "EXIF6 upper-left")
    assert close(second.getpixel((x + 54, y + 27)), (255, 0, 0)), (prefix, "EXIF6 upper-right")
    assert close(second.getpixel((x + 54, y + 81)), (0, 255, 0)), (prefix, "EXIF6 lower-right")
    print(prefix, "PASS: two rendered Letter pages, physical placement, alpha and EXIF6 orientation")
