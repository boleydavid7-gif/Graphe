"""Generate deterministic, handcrafted OPC fixtures; no Office engine is used."""

from pathlib import Path
from textwrap import dedent
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo
import binascii
import struct
import zlib

ROOT = Path(__file__).parent
W = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
R = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
P = 'http://schemas.openxmlformats.org/package/2006/relationships'
C = 'http://schemas.openxmlformats.org/package/2006/content-types'
MAIN = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml'
RELS = 'application/vnd.openxmlformats-package.relationships+xml'


def xml(value):
    return ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
            + dedent(value).strip() + '\n').encode('utf-8')


def png():
    def chunk(name, data):
        return (struct.pack('>I', len(data)) + name + data
                + struct.pack('>I', binascii.crc32(name + data) & 0xffffffff))
    return (b'\x89PNG\r\n\x1a\n'
            + chunk(b'IHDR', struct.pack('>IIBBBBB', 1, 1, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(b'\x00\x80\x55\x22\xff'))
            + chunk(b'IEND', b''))


def archive(name, parts):
    with ZipFile(ROOT / name, 'w', compression=ZIP_DEFLATED) as target:
        target.comment = b'Office Engine handcrafted fixture; not an interoperability certification.'
        for path, content in parts.items():
            info = ZipInfo(path, date_time=(2020, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            target.writestr(info, content, compress_type=ZIP_DEFLATED, compresslevel=6)


archive('writer-source-preservation.docx', {
    '[Content_Types].xml': xml(f'''
        <Types xmlns="{C}">
          <Default Extension="rels" ContentType="{RELS}"/>
          <Default Extension="xml" ContentType="application/xml"/>
          <Default Extension="png" ContentType="image/png"/>
          <Override PartName="/word/document.xml" ContentType="{MAIN}"/>
          <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
          <Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
        </Types>'''),
    '_rels/.rels': xml(f'''
        <Relationships xmlns="{P}">
          <Relationship Id="rId1" Type="{R}/officeDocument" Target="word/document.xml"/>
          <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
        </Relationships>'''),
    'word/document.xml': xml(f'''
        <w:document xmlns:w="{W}" xmlns:r="{R}"
          xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
          xmlns:x="urn:writer:future-fixture" mc:Ignorable="x">
          <w:body>
            <w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:t>Quarterly planning notes</w:t></w:r></w:p>
            <w:p><w:r><w:t xml:space="preserve">Café, 日本語, 👩🏽‍💻 — </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>preserve the source</w:t></w:r></w:p>
            <w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="3600"/></w:tblGrid><w:tr><w:tc><w:tcPr/><w:p><w:r><w:t>Unsupported table content</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
            <w:p><w:fldSimple w:instr="DATE"><w:r><w:t>2026-11-14</w:t></w:r></w:fldSimple></w:p>
            <w:p><w:hyperlink r:id="rIdLink"><w:r><w:t>External source</w:t></w:r></w:hyperlink></w:p>
            <mc:AlternateContent><mc:Choice Requires="x"><x:futureFeature>Do not discard this extension.</x:futureFeature></mc:Choice><mc:Fallback><w:p><w:r><w:t>Fallback text</w:t></w:r></w:p></mc:Fallback></mc:AlternateContent>
            <w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr>
          </w:body>
        </w:document>'''),
    'word/_rels/document.xml.rels': xml(f'''
        <Relationships xmlns="{P}">
          <Relationship Id="rIdStyles" Type="{R}/styles" Target="styles.xml"/>
          <Relationship Id="rIdImage" Type="{R}/image" Target="media/image1.png"/>
          <Relationship Id="rIdCustom" Type="{R}/customXml" Target="../customXml/item1.xml"/>
          <Relationship Id="rIdLink" Type="{R}/hyperlink" Target="https://example.invalid/source" TargetMode="External"/>
        </Relationships>'''),
    'word/styles.xml': xml(f'''
        <w:styles xmlns:w="{W}">
          <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults>
          <w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:rPr><w:b/><w:sz w:val="40"/></w:rPr></w:style>
        </w:styles>'''),
    'docProps/core.xml': xml('''
        <cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/">
          <dc:title>Quarterly planning notes</dc:title><dc:creator>Office Engine fixture</dc:creator>
        </cp:coreProperties>'''),
    'customXml/item1.xml': xml('<x:original xmlns:x="urn:writer:fixture">Original metadata remains intact.</x:original>'),
    'word/media/image1.png': png(),
})

archive('strict-relocated.docx', {
    '[Content_Types].xml': xml(f'''<ct:Types xmlns:ct="{C}"><ct:Default Extension="rels" ContentType="{RELS}"/><ct:Override PartName="/documents/text/main.xml" ContentType="{MAIN}"/></ct:Types>'''),
    '_rels/.rels': xml(f'''<pr:Relationships xmlns:pr="{P}"><pr:Relationship Id="main" Type="http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument" Target="/documents/text/main.xml"/></pr:Relationships>'''),
    'documents/text/main.xml': xml('''<s:document xmlns:s="http://purl.oclc.org/ooxml/wordprocessingml/main"><s:body><s:p><s:r><s:t>Strict namespace, relocated main part.</s:t></s:r></s:p></s:body></s:document>'''),
})
