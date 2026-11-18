"""Independent table-consumer probe (stdlib only, no engine import).

Generate review.docx/odt/pdf using export_format from the adjacent native
project-review-table.writer fixture. Convert DOCX→ODT into from-docx/ and
ODT→DOCX into from-odt/ using LibreOffice, then pass the artifact directory.
This verifies structure/text/negative underline; it is not Word certification.
"""
import json
import sys
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

BASE = Path(sys.argv[1])
SOURCE = Path(__file__).parent / "fixtures/project-review-table.writer"
NATIVE = json.loads(SOURCE.read_text())["document"]
EXPECTED = []
for section in NATIVE["sections"]:
    for block in section["blocks"]:
        paragraphs = [block["content"]] if block["kind"] == "paragraph" else [
            paragraph for row in block["content"]["rows"]
            for cell in row["cells"] for paragraph in cell["paragraphs"]
        ]
        EXPECTED.extend("".join(run["text"] for run in p["runs"]) for p in paragraphs)
NS = {
    "t": "urn:oasis:names:tc:opendocument:xmlns:text:1.0",
    "tb": "urn:oasis:names:tc:opendocument:xmlns:table:1.0",
    "w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
}


def odt_text(node):
    result = node.text or ""
    for child in node:
        if child.tag == "{" + NS["t"] + "}s":
            result += " " * int(child.get("{" + NS["t"] + "}c", "1"))
        elif child.tag == "{" + NS["t"] + "}tab":
            result += "\t"
        elif child.tag == "{" + NS["t"] + "}line-break":
            result += "\n"
        else:
            result += odt_text(child)
        result += child.tail or ""
    return result


for relative in ["review.odt", "from-docx/review.odt"]:
    with zipfile.ZipFile(BASE / relative) as archive:
        root = ET.fromstring(archive.read("content.xml"))
    tables = root.findall(".//tb:table", NS)
    assert len(tables) == 1, (relative, "table count")
    rows = tables[0].findall("tb:table-row", NS)
    assert len(rows) == 36, (relative, "row count")
    assert all(len(r.findall("tb:table-cell", NS)) == 2 for r in rows)
    actual = [odt_text(p) for p in root.findall(".//t:p", NS)]
    assert actual == EXPECTED, (relative, "Unicode/text/paragraph order")
    print(relative, "PASS: 36 rows, 72 cells, 75 exact Unicode paragraphs")

for relative in ["review.docx", "from-odt/review.docx"]:
    with zipfile.ZipFile(BASE / relative) as archive:
        root = ET.fromstring(archive.read("word/document.xml"))
    tables = root.findall(".//w:tbl", NS)
    assert len(tables) == 1, (relative, "table count")
    rows = tables[0].findall("w:tr", NS)
    assert len(rows) == 36, (relative, "row count")
    assert all(len(r.findall("w:tc", NS)) == 2 for r in rows)
    paragraphs = root.findall(".//w:p", NS)
    actual = ["".join(t.text or "" for t in p.findall(".//w:t", NS)) for p in paragraphs]
    assert actual == EXPECTED, (relative, "Unicode/text/paragraph order")
    for paragraph in paragraphs:
        for run in paragraph.findall(".//w:r", NS):
            text = "".join(t.text or "" for t in run.findall("w:t", NS))
            underline = run.find("w:rPr/w:u", NS)
            if text.startswith(("Phase ", "Row ")):
                assert underline is None or underline.get("{" + NS["w"] + "}val", "single") == "none", (relative, "unwanted underline")
    print(relative, "PASS: 36 rows, 72 cells, 75 exact Unicode paragraphs; body underline off")
