"""Reproduce synthetic relationship-boundary fixtures; producer originals are separate.

Run from this directory. Uses only the Python standard library and existing
repository/MIT SDK fixture parts; it does not invoke or impersonate Word.
"""

from copy import deepcopy
from pathlib import Path
import zipfile
import xml.etree.ElementTree as E

HERE = Path(__file__).resolve().parent
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/"
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = REL.rstrip("/")
CT = "http://schemas.openxmlformats.org/package/2006/content-types"
WORD_CT = "application/vnd.openxmlformats-officedocument.wordprocessingml."
MAIN_RELS = "word/_rels/document.xml.rels"
ROOT_RELS = "_rels/.rels"
E.register_namespace("w", W)
E.register_namespace("r", R)


def xml(text):
    return text.encode("utf-8")


def relation(kind, target, identity, external=False):
    value = {"Id": identity, "Type": REL + kind, "Target": target}
    if external:
        value["TargetMode"] = "External"
    return value


def write_relationships(values):
    root = E.Element(f"{{{PKG}}}Relationships")
    for value in values:
        E.SubElement(root, f"{{{PKG}}}Relationship", value)
    return E.tostring(root, encoding="utf-8", xml_declaration=True)


def write_package(name, parts, types, root_rels, main_rels):
    parts = dict(parts)
    parts[ROOT_RELS] = write_relationships(root_rels)
    parts[MAIN_RELS] = write_relationships(main_rels)
    content = E.Element(f"{{{CT}}}Types")
    E.SubElement(content, f"{{{CT}}}Default", {
        "Extension": "rels", "ContentType": "application/vnd.openxmlformats-package.relationships+xml",
    })
    E.SubElement(content, f"{{{CT}}}Default", {"Extension": "xml", "ContentType": "application/xml"})
    for part, media in sorted(types.items()):
        E.SubElement(content, f"{{{CT}}}Override", {"PartName": "/" + part, "ContentType": media})
    parts["[Content_Types].xml"] = E.tostring(content, encoding="utf-8", xml_declaration=True)
    with zipfile.ZipFile(HERE / name, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as package:
        for path, data in sorted(parts.items()):
            entry = zipfile.ZipInfo(path, date_time=(1980, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            package.writestr(entry, data, compresslevel=9)


with zipfile.ZipFile(HERE.parent / "supported-basic.docx") as basic:
    body = basic.read("document.xml")
with zipfile.ZipFile(HERE / "microsoft-sdk-hello-world.docx") as microsoft:
    theme = microsoft.read("word/theme/theme1.xml")
with zipfile.ZipFile(HERE.parent / "writer-source-preservation.docx") as existing:
    pixel = existing.read("word/media/image1.png")

parts = {
    "word/document.xml": body,
    "word/styles.xml": xml(f'''<w:styles xmlns:w="{W}"><w:style w:type="paragraph" w:styleId="FixtureNormal" w:default="1"><w:name w:val="Fixture Normal"/><w:rPr><w:rFonts w:ascii="Arimo" w:hAnsi="Arimo" w:eastAsia="Arimo" w:cs="Arimo"/><w:sz w:val="24"/><w:szCs w:val="24"/></w:rPr></w:style></w:styles>'''),
    "word/theme/theme1.xml": theme,
    "word/fontTable.xml": xml(f'''<w:fonts xmlns:w="{W}"><w:font w:name="Arimo"><w:family w:val="swiss"/><w:pitch w:val="variable"/></w:font></w:fonts>'''),
    "word/settings.xml": xml(f'''<w:settings xmlns:w="{W}"><w:zoom w:percent="100"/><w:proofState w:spelling="clean" w:grammar="clean"/><w:rsids><w:rsidRoot w:val="00000001"/><w:rsid w:val="00000001"/></w:rsids></w:settings>'''),
    "word/webSettings.xml": xml(f'''<w:webSettings xmlns:w="{W}"><w:optimizeForBrowser/><w:allowPNG/></w:webSettings>'''),
    "docProps/app.xml": xml('''<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Writer relationship fixture</Application><AppVersion>0.1</AppVersion></Properties>'''),
    "docProps/custom.xml": xml('''<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="Project"><vt:lpwstr>Relationship regression</vt:lpwstr></property></Properties>'''),
    "docProps/core.xml": xml('''<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>Relationship fixture é 👩🏽‍💻</dc:title><dc:creator>Fixture Author</dc:creator><cp:lastModifiedBy>Writer relationship fixture</cp:lastModifiedBy><dcterms:created xsi:type="dcterms:W3CDTF">2026-01-01T00:00:00Z</dcterms:created></cp:coreProperties>'''),
}
types = {
    "word/document.xml": WORD_CT + "document.main+xml",
    "word/styles.xml": WORD_CT + "styles+xml",
    "word/settings.xml": WORD_CT + "settings+xml",
    "word/fontTable.xml": WORD_CT + "fontTable+xml",
    "word/webSettings.xml": WORD_CT + "webSettings+xml",
    "word/theme/theme1.xml": "application/vnd.openxmlformats-officedocument.theme+xml",
    "docProps/app.xml": "application/vnd.openxmlformats-officedocument.extended-properties+xml",
    "docProps/custom.xml": "application/vnd.openxmlformats-officedocument.custom-properties+xml",
    "docProps/core.xml": "application/vnd.openxmlformats-package.core-properties+xml",
}
root_rels = [
    relation("officeDocument", "word/document.xml", "main"),
    relation("extended-properties", "docProps/app.xml", "app"),
    relation("custom-properties", "docProps/custom.xml", "custom"),
    {"Id": "core", "Type": PKG + "/metadata/core-properties", "Target": "docProps/core.xml"},
]
main_rels = [relation(kind, target, kind) for kind, target in (
    ("styles", "styles.xml"), ("settings", "settings.xml"), ("fontTable", "fontTable.xml"),
    ("webSettings", "webSettings.xml"), ("theme", "theme/theme1.xml"),
)]


def candidate():
    return deepcopy(parts), deepcopy(types), deepcopy(root_rels), deepcopy(main_rels)


write_package("supported-auxiliary-parts.docx", *candidate())

p, t, r, m = candidate()
p["docProps/thumbnail.png"] = pixel
t["docProps/thumbnail.png"] = "image/png"
r.append({"Id": "thumbnail", "Type": PKG + "/metadata/thumbnail", "Target": "docProps/thumbnail.png"})
write_package("supported-root-thumbnail.docx", p, t, r, m)
m.append(relation("image", "../docProps/thumbnail.png", "body-image"))
write_package("unsupported-main-image-same-thumbnail.docx", p, t, r, m)

for kind, extension, content, media in (
    ("image", "png", pixel, "image/png"),
    ("header", "xml", xml(f'<w:hdr xmlns:w="{W}"><w:p><w:r><w:t>Unsupported header</w:t></w:r></w:p></w:hdr>'), WORD_CT + "header+xml"),
    ("footer", "xml", xml(f'<w:ftr xmlns:w="{W}"><w:p><w:r><w:t>Unsupported footer</w:t></w:r></w:p></w:ftr>'), WORD_CT + "footer+xml"),
    ("altchunk", "html", b"<html><body><p>Unsupported imported HTML</p></body></html>", "text/html"),
):
    p, t, r, m = candidate()
    target = f"{kind}.{extension}"
    p["word/" + target] = content
    t["word/" + target] = media
    m.append(relation("aFChunk" if kind == "altchunk" else kind, target, "unsafe"))
    write_package(f"unsupported-{kind}.docx", p, t, r, m)

p, t, r, m = candidate()
m.append(relation("hyperlink", "https://example.invalid/hyperlink", "unsafe", external=True))
write_package("unsupported-hyperlink.docx", p, t, r, m)

p, t, r, m = candidate()
r[1] = relation("extended-properties", "https://example.invalid/app.xml", "app", external=True)
write_package("unsupported-external-relationship.docx", p, t, r, m)

for kind in ("font", "color", "style"):
    p, t, r, m = candidate()
    path = "word/styles.xml" if kind == "style" else "word/document.xml"
    root = E.fromstring(p[path])
    if kind == "style":
        element = root.find(f".//{{{W}}}rFonts")
        element.attrib.clear()
        element.set(f"{{{W}}}asciiTheme", "minorHAnsi")
        element.set(f"{{{W}}}hAnsiTheme", "minorHAnsi")
    else:
        run = root.find(f".//{{{W}}}r")
        properties = E.Element(f"{{{W}}}rPr")
        run.insert(0, properties)
        if kind == "font":
            E.SubElement(properties, f"{{{W}}}rFonts", {f"{{{W}}}asciiTheme": "minorHAnsi", f"{{{W}}}hAnsiTheme": "minorHAnsi"})
        else:
            E.SubElement(properties, f"{{{W}}}color", {f"{{{W}}}val": "4472C4", f"{{{W}}}themeColor": "accent1", f"{{{W}}}themeTint": "BF"})
    p[path] = E.tostring(root, encoding="utf-8", xml_declaration=True)
    write_package(f"unsupported-theme-{kind}-reference.docx", p, t, r, m)

p, t, r, m = candidate()
t["word/settings.xml"] = "image/png"
write_package("mismatched-auxiliary-mime.docx", p, t, r, m)

p, t, r, m = candidate()
p["word/settings.xml"] = xml(f'<w:document xmlns:w="{W}"><w:body><w:p><w:r><w:t>Hidden body</w:t></w:r></w:p></w:body></w:document>')
write_package("mismatched-auxiliary-root.docx", p, t, r, m)

p, t, r, m = candidate()
m = [entry for entry in m if entry["Id"] != "fontTable"]
r.append(relation("fontTable", "word/fontTable.xml", "misplaced"))
write_package("misplaced-root-font-table.docx", p, t, r, m)

p, t, r, m = candidate()
r = [entry for entry in r if entry["Id"] != "app"]
m.append(relation("extended-properties", "../docProps/app.xml", "misplaced"))
write_package("misplaced-main-app-properties.docx", p, t, r, m)

p, t, r, m = candidate()
next(entry for entry in m if entry["Id"] == "theme")["Type"] = "https://example.invalid/relationships/theme"
write_package("unknown-theme-relationship.docx", p, t, r, m)

p, t, r, m = candidate()
p["word/settings2.xml"] = p["word/settings.xml"]
t["word/settings2.xml"] = t["word/settings.xml"]
m.append(relation("settings", "settings2.xml", "duplicate"))
write_package("duplicate-settings-relationship.docx", p, t, r, m)

p, t, r, m = candidate()
font = E.fromstring(p["word/fontTable.xml"])
E.SubElement(font.find(f"{{{W}}}font"), f"{{{W}}}embedRegular", {f"{{{R}}}id": "embedded", f"{{{W}}}fontKey": "{00000000-0000-0000-0000-000000000000}"})
p["word/fontTable.xml"] = E.tostring(font, encoding="utf-8", xml_declaration=True)
p["word/_rels/fontTable.xml.rels"] = write_relationships([relation("font", "fonts/font1.odttf", "embedded")])
p["word/fonts/font1.odttf"] = b"unsupported opaque embedded-font sentinel"
t["word/fonts/font1.odttf"] = "application/vnd.openxmlformats-officedocument.obfuscatedFont"
write_package("unsupported-font-embedding.docx", p, t, r, m)

for name, element in (
    ("compat", f'<w:compat xmlns:w="{W}"><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat>'),
    ("default-tabs", f'<w:defaultTabStop xmlns:w="{W}" w:val="720"/>'),
):
    p, t, r, m = candidate()
    settings = E.fromstring(p["word/settings.xml"])
    settings.append(E.fromstring(element))
    p["word/settings.xml"] = E.tostring(settings, encoding="utf-8", xml_declaration=True)
    write_package(f"unsupported-settings-{name}.docx", p, t, r, m)

p, t, r, m = candidate()
p["word/theme/_rels/theme1.xml.rels"] = write_relationships([
    relation("image", "https://example.invalid/hidden.png", "external", external=True),
])
write_package("unsupported-auxiliary-external-chain.docx", p, t, r, m)

# Authoring-session IDs and a resolved Ignorable declaration do not contain
# revision content. The declaration never authorizes ignoring actual markup.
MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"
W14 = "http://schemas.microsoft.com/office/word/2010/wordml"
for name in ("supported-authoring-metadata", "unsupported-unbound-ignorable", "unsupported-ignorable-extension", "unsupported-revision", "unsupported-malformed-session-id", "unsupported-malformed-default-session-id"):
    p, t, r, m = candidate()
    document = p["word/document.xml"].decode("utf-8")
    prefixes = "missing" if name == "unsupported-unbound-ignorable" else "w14"
    document = document.replace(f'<w:document xmlns:w="{W}">', f'<w:document xmlns:w="{W}" xmlns:mc="{MC}" xmlns:w14="{W14}" mc:Ignorable="{prefixes}">')
    session = "not-hex" if name == "unsupported-malformed-session-id" else "00A012Ff"
    document = document.replace("<w:p>", f'<w:p w:rsidR="{session}" w:rsidRPr="00000002" w:rsidP="00000003" w:rsidDel="00000004">', 1)
    default_session = "not-hex" if name == "unsupported-malformed-default-session-id" else "00000005"
    document = document.replace(f'<w:p w:rsidR="{session}"', f'<w:p w:rsidR="{session}" w:rsidRDefault="{default_session}"', 1)
    if name == "unsupported-ignorable-extension":
        document = document.replace("<w:r>", '<w:pPr><w14:unrecognizedLayoutProperty/></w:pPr><w:r>', 1)
    if name == "unsupported-revision":
        document = document.replace("<w:r>", '<w:ins w:id="1" w:author="Fixture author"><w:r>', 1).replace("</w:r>", "</w:r></w:ins>", 1)
    p["word/document.xml"] = document.encode("utf-8")
    write_package(name + ".docx", p, t, r, m)

# Strict metadata relationship names use camelCase, unlike their Transitional
# counterparts. Package-level core/thumbnail roles and MIME types are unchanged.
STRICT_REL = "http://purl.oclc.org/ooxml/officeDocument/relationships/"
strict_namespaces = (
    (REL + "extended-properties", STRICT_REL + "extendedProperties"),
    (REL + "custom-properties", STRICT_REL + "customProperties"),
    (REL, STRICT_REL),
    (W, "http://purl.oclc.org/ooxml/wordprocessingml/main"),
    ("http://schemas.openxmlformats.org/drawingml/2006/main", "http://purl.oclc.org/ooxml/drawingml/main"),
    ("http://schemas.openxmlformats.org/officeDocument/2006/extended-properties", "http://purl.oclc.org/ooxml/officeDocument/extendedProperties"),
    ("http://schemas.openxmlformats.org/officeDocument/2006/custom-properties", "http://purl.oclc.org/ooxml/officeDocument/customProperties"),
    ("http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes", "http://purl.oclc.org/ooxml/officeDocument/docPropsVTypes"),
)
for source_name, target_name in (
    ("supported-root-thumbnail.docx", "supported-strict-auxiliary-parts.docx"),
    ("supported-authoring-metadata.docx", "supported-strict-authoring-metadata.docx"),
    ("unsupported-theme-font-reference.docx", "unsupported-strict-theme-font-reference.docx"),
    ("unsupported-theme-color-reference.docx", "unsupported-strict-theme-color-reference.docx"),
    ("unsupported-theme-style-reference.docx", "unsupported-strict-theme-style-reference.docx"),
):
    with zipfile.ZipFile(HERE / source_name) as package:
        converted = {name: package.read(name) for name in package.namelist()}
    for name, data in converted.items():
        if name.endswith((".xml", ".rels")):
            for transitional, strict in strict_namespaces:
                data = data.replace(transitional.encode(), strict.encode())
            converted[name] = data
    content_types = E.fromstring(converted.pop("[Content_Types].xml"))
    strict_types = {node.attrib["PartName"].lstrip("/"): node.attrib["ContentType"] for node in content_types if node.tag == f"{{{CT}}}Override"}
    strict_root = [dict(node.attrib) for node in E.fromstring(converted.pop(ROOT_RELS))]
    strict_main = [dict(node.attrib) for node in E.fromstring(converted.pop(MAIN_RELS))]
    write_package(target_name, converted, strict_types, strict_root, strict_main)
