"""Generate deterministic handcrafted positive semantic-conversion fixtures."""

from pathlib import Path
from textwrap import dedent
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

ROOT = Path(__file__).parent
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
P = "http://schemas.openxmlformats.org/package/2006/relationships"
C = "http://schemas.openxmlformats.org/package/2006/content-types"
MAIN = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
STYLES = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"
CORE = "application/vnd.openxmlformats-package.core-properties+xml"


def xml(text):
    return ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n' + dedent(text).strip() + '\n').encode("utf-8")


def archive(name, parts):
    with ZipFile(ROOT / name, "w", compression=ZIP_DEFLATED, compresslevel=6) as output:
        for path, content in parts.items():
            item = ZipInfo(path, (2020, 1, 1, 0, 0, 0))
            item.create_system = 3
            item.external_attr = 0o100644 << 16
            item.compress_type = ZIP_DEFLATED
            output.writestr(item, content, compress_type=ZIP_DEFLATED, compresslevel=6)


basic = {
    "[Content_Types].xml": xml(f'''<Types xmlns="{C}"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/document.xml" ContentType="{MAIN}"/></Types>'''),
    "_rels/.rels": xml(f'''<Relationships xmlns="{P}"><Relationship Id="main" Type="{R}/officeDocument" Target="document.xml"/></Relationships>'''),
    "document.xml": xml(f'''<w:document xmlns:w="{W}"><w:body><w:p><w:r><w:t>DOCX hello — café 👩🏽‍💻</w:t></w:r></w:p></w:body></w:document>'''),
}
archive("supported-basic.docx", basic)

partial_spacing = dict(basic)
partial_spacing["document.xml"] = xml(f'''<w:document xmlns:w="{W}"><w:body><w:p><w:pPr><w:spacing w:line="300"/></w:pPr><w:r><w:t>Partial line spacing remains read-only.</w:t></w:r></w:p></w:body></w:document>''')
archive("unsupported-partial-line-spacing.docx", partial_spacing)

carriage_return = dict(basic)
carriage_return["document.xml"] = carriage_return["document.xml"].replace(
    b"</w:t>", b"&#13;literal carriage return</w:t>"
)
archive("unsupported-carriage-return.docx", carriage_return)

formatted = {
    "[Content_Types].xml": xml(f'''
        <Types xmlns="{C}">
          <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
          <Override PartName="/documents/main.xml" ContentType="{MAIN}"/>
          <Override PartName="/formatting/paragraphs.xml" ContentType="{STYLES}"/>
          <Override PartName="/metadata/core.xml" ContentType="{CORE}"/>
        </Types>'''),
    "_rels/.rels": xml(f'''
        <Relationships xmlns="{P}">
          <Relationship Id="main" Type="{R}/officeDocument" Target="documents/main.xml"/>
          <Relationship Id="metadata" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="metadata/core.xml"/>
        </Relationships>'''),
    "documents/_rels/main.xml.rels": xml(f'''<Relationships xmlns="{P}"><Relationship Id="styles" Type="{R}/styles" Target="../formatting/paragraphs.xml"/></Relationships>'''),
    "documents/main.xml": xml(f'''
        <w:document xmlns:w="{W}"><w:body>
          <w:p><w:pPr><w:pStyle w:val="Title"/><w:pageBreakBefore w:val="0"/><w:spacing w:before="240" w:after="120" w:line="360" w:lineRule="exact"/><w:ind w:left="720" w:right="180" w:firstLine="360"/><w:jc w:val="center"/></w:pPr>
            <w:r><w:rPr><w:rFonts w:ascii="Arimo" w:hAnsi="Arimo" w:eastAsia="Arimo" w:cs="Arimo"/><w:b/><w:bCs/><w:color w:val="112233"/><w:sz w:val="28"/><w:szCs w:val="28"/><w:u w:val="double"/></w:rPr><w:t xml:space="preserve">  Café 👩🏽‍💻 </w:t></w:r>
            <w:r><w:rPr><w:b w:val="0"/><w:bCs w:val="0"/><w:i w:val="0"/><w:iCs w:val="0"/></w:rPr><w:t xml:space="preserve">日本語 &amp; &lt;XML&gt; </w:t><w:br/><w:t>Second line</w:t><w:tab/><w:t>tabbed</w:t></w:r>
          </w:p>
          <w:p><w:pPr><w:sectPr><w:pgSz w:w="12240" w:h="15840" w:orient="portrait"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:pPr><w:r><w:t>First section closes here.</w:t></w:r></w:p>
          <w:p><w:pPr><w:pageBreakBefore/><w:spacing w:line="360" w:lineRule="auto"/><w:ind w:left="720" w:right="0" w:hanging="360"/><w:jc w:val="right"/></w:pPr><w:r><w:t>A4 section — العربية עברית</w:t></w:r></w:p>
          <w:sectPr><w:pgSz w:w="11906" w:h="16838" w:orient="portrait"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1134" w:header="567" w:footer="567" w:gutter="0"/></w:sectPr>
        </w:body></w:document>'''),
    "formatting/paragraphs.xml": xml(f'''
        <w:styles xmlns:w="{W}">
          <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Arimo" w:hAnsi="Arimo" w:eastAsia="Arimo" w:cs="Arimo"/><w:color w:val="auto"/><w:sz w:val="24"/><w:szCs w:val="24"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="240" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>
          <w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:name w:val="Normal"/><w:rPr><w:b/><w:bCs/></w:rPr></w:style>
          <w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:bCs/><w:i/><w:iCs/></w:rPr></w:style>
        </w:styles>'''),
    "metadata/core.xml": xml('''<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Supported Writer DOCX – café</dc:title></cp:coreProperties>'''),
}
archive("supported-formatting.docx", formatted)
