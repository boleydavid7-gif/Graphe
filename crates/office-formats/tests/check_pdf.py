"""Independent optional pypdf verification of native PDF/geometry artifacts.

Generate with office-formats/examples/export_pdf and the existing
office-layout/examples/layout_snapshot. This is verification tooling only;
pypdf/Poppler are not dependencies of the application or engine.
"""

import json
import sys
from pathlib import Path

from pypdf import PdfReader


def check(pdf_path: str, layout_path: str) -> dict:
    """Reopen the generated PDF and compare its page boxes and vector resources."""
    reader = PdfReader(pdf_path, strict=True)
    layout = json.loads(Path(layout_path).read_text(encoding="utf-8"))
    assert len(reader.pages) == len(layout["pages"])
    largest_error = 0.0
    for page, expected in zip(reader.pages, layout["pages"]):
        for actual, native in zip(
            (float(page.mediabox.width), float(page.mediabox.height)),
            (expected["width"] / 64, expected["height"] / 64),
        ):
            error = abs(actual - native)
            largest_error = max(error, largest_error)
            # pdf-writer emits shortest round-tripping f32 decimals. The page
            # geometry originates in 1/64 pt units; <0.001 pt allows that PDF
            # decimal serialization without pretending to establish bit equality.
            assert error < 0.001, (actual, native)
        objects = page["/Resources"].get("/XObject", {})
        if any(line["glyphs"] for line in expected["lines"]):
            assert len(objects) > 0
        for indirect in objects.values():
            form = indirect.get_object()
            assert form["/Type"] == "/XObject"
            assert form["/Subtype"] == "/Form"
            assert "/BBox" in form
            # Whitespace glyph forms are intentionally empty; every referenced
            # stream still has to resolve and decode through an independent reader.
            form.get_data()
        assert not page.extract_text(), "Outlined profile must not imply text support"
    assert not reader.is_encrypted
    return {
        "pages": len(reader.pages),
        "title": reader.metadata.title,
        "largest_page_box_error_points": largest_error,
        "outlined_profile": True,
        "bytes": Path(pdf_path).stat().st_size,
    }


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: check_pdf.py output.pdf layout.json")
    print(json.dumps(check(sys.argv[1], sys.argv[2])))
