//! Bounded ODF ZIP and XML reading, without extraction or external resources.

use crate::FormatError;
use quick_xml::{
    Reader, XmlVersion,
    events::{BytesDecl, BytesStart, Event},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
};
use zip::{
    CompressionMethod, ZipArchive,
    read::{ArchiveOffset, Config},
};

const MAX_INPUT: usize = 16 * 1024 * 1024;
const MAX_PART: usize = 8 * 1024 * 1024;
const MAX_EXPANDED: usize = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;
const MAX_XML: usize = 2 * 1024 * 1024;
const MAX_NODES: usize = 32_768;
const MAX_ATTRIBUTES: usize = 65_536;

pub(super) fn invalid(reason: impl std::fmt::Display) -> FormatError {
    FormatError::new("invalid_odt", format!("Invalid ODT: {reason}"))
}

fn limit(resource: &str) -> FormatError {
    FormatError::new(
        "format_limit",
        format!("ODT {resource} exceeds the supported resource limit."),
    )
}

fn u16_at(bytes: &[u8], at: usize) -> Result<usize, FormatError> {
    let value = bytes
        .get(at..at + 2)
        .ok_or_else(|| invalid("truncated ZIP directory"))?;
    Ok(u16::from_le_bytes([value[0], value[1]]) as usize)
}

fn u32_at(bytes: &[u8], at: usize) -> Result<usize, FormatError> {
    let value = bytes
        .get(at..at + 4)
        .ok_or_else(|| invalid("truncated ZIP directory"))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]) as usize)
}

pub(super) fn read(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, FormatError> {
    if bytes.len() > MAX_INPUT {
        return Err(limit("input bytes"));
    }
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err(invalid("expected an ordinary ZIP package"));
    }
    let start = bytes.len().saturating_sub(65_557);
    let end = (start..bytes.len().saturating_sub(21))
        .rev()
        .find(|&at| {
            bytes.get(at..at + 4) == Some(b"PK\x05\x06")
                && u16_at(bytes, at + 20).is_ok_and(|comment| at + 22 + comment == bytes.len())
        })
        .ok_or_else(|| invalid("missing ZIP end directory"))?;
    let count = u16_at(bytes, end + 10)?;
    if count == 0 || count > MAX_ENTRIES {
        return Err(limit("package entries"));
    }
    if u16_at(bytes, end + 4)? != 0
        || u16_at(bytes, end + 6)? != 0
        || u16_at(bytes, end + 8)? != count
    {
        return Err(invalid("split ZIP packages are unsupported"));
    }
    let directory = u32_at(bytes, end + 16)?;
    let size = u32_at(bytes, end + 12)?;
    if directory.checked_add(size) != Some(end) {
        return Err(invalid("ZIP64, offset or overlapping ZIP directory"));
    }
    let mut archive = ZipArchive::with_config(
        Config {
            archive_offset: ArchiveOffset::Known(0),
        },
        Cursor::new(bytes),
    )
    .map_err(invalid)?;
    if archive.len() != count
        || archive.central_directory_start() != directory as u64
        || archive.offset() != 0
        || archive.has_duplicate_names()
    {
        return Err(invalid("inconsistent or duplicate ZIP directory"));
    }
    archive.validate_header_consistency().map_err(invalid)?;
    if archive.has_overlapping_entries().map_err(invalid)? {
        return Err(invalid("overlapping ZIP members"));
    }
    let mut result = BTreeMap::new();
    let mut total = 0_usize;
    for index in 0..count {
        let mut file = archive.by_index(index).map_err(invalid)?;
        let name = std::str::from_utf8(file.name_raw())
            .map_err(invalid)?
            .to_owned();
        if !name.is_ascii()
            || name.is_empty()
            || name.starts_with('/')
            || name.contains('\\')
            || name.contains(':')
            || name.chars().any(char::is_control)
            || name
                .trim_end_matches('/')
                .split('/')
                .any(|piece| matches!(piece, "" | "." | ".."))
            || file.name().map_err(invalid)?.as_ref() != name
        {
            return Err(invalid("noncanonical ZIP member name"));
        }
        if file.encrypted()
            || file.is_symlink()
            || !matches!(
                file.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err(invalid(
                "encrypted, linked or unsupported compressed member",
            ));
        }
        if index == 0
            && (name != "mimetype"
                || file.header_start() != 0
                || file.compression() != CompressionMethod::Stored
                || u16_at(bytes, 28)? != 0)
        {
            return Err(invalid(
                "mimetype must be first, stored, with no local extra field",
            ));
        }
        if file.size() > MAX_PART as u64 {
            return Err(limit("part bytes"));
        }
        let mut content = Vec::new();
        let mut chunk = [0_u8; 8192];
        loop {
            let read = file.read(&mut chunk).map_err(invalid)?;
            if read == 0 {
                break;
            }
            if content.len() + read > MAX_PART || total + content.len() + read > MAX_EXPANDED {
                return Err(limit("expanded bytes"));
            }
            content.extend_from_slice(&chunk[..read]);
        }
        if content.len() as u64 != file.size() || (file.is_dir() && !content.is_empty()) {
            return Err(invalid("ZIP member length differs from its directory"));
        }
        total += content.len();
        result.insert(name, content);
    }
    if result.get("mimetype").map(Vec::as_slice) != Some(super::MEDIA_TYPE.as_bytes()) {
        return Err(invalid("package is not an OpenDocument text document"));
    }
    Ok(result)
}

pub(super) fn xml<'a>(part: &str, bytes: &'a [u8]) -> Result<roxmltree::Document<'a>, FormatError> {
    if bytes.len() > MAX_XML {
        return Err(limit("XML bytes"));
    }
    let text = std::str::from_utf8(bytes).map_err(invalid)?;
    if text.chars().any(|c| !matches!(c, '\u{9}' | '\u{a}' | '\u{d}' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')) {
        return Err(invalid("illegal XML scalar"));
    }
    if bytes.iter().filter(|&&b| b == b'<').count() > MAX_NODES
        || bytes.iter().filter(|&&b| b == b'=').count() > MAX_ATTRIBUTES
    {
        return Err(limit("XML allocation markers"));
    }
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_comments = true;
    let mut depth = 0_usize;
    let mut nodes = 1_usize;
    let mut attributes = 0_usize;
    let mut namespaces = 0_usize;
    loop {
        let event = reader.read_event().map_err(invalid)?;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(element) | Event::Empty(element) => {
                nodes += 1;
                if depth >= 64 {
                    return Err(limit("XML depth"));
                }
                let mut declared = BTreeSet::new();
                for (index, attr) in element.attributes().with_checks(false).enumerate() {
                    if index >= 64 {
                        return Err(limit("XML attributes"));
                    }
                    let attr = attr.map_err(invalid)?;
                    let value = attr
                        .normalized_value(XmlVersion::Explicit1_0)
                        .map_err(invalid)?;
                    attributes += 1;
                    let name = attr.key.into_inner();
                    if name == "xmlns" || name.starts_with("xmlns:") {
                        namespaces += 1;
                        if name == "xmlns:xmlns"
                            || (name != "xmlns" && value.is_empty())
                            || !declared.insert(name)
                        {
                            return Err(invalid("invalid namespace declaration"));
                        }
                    }
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::GeneralRef(reference) => {
                reference.resolve_char_ref().map_err(invalid)?;
                nodes += 1;
            }
            Event::DocType(_) => return Err(invalid("DTDs and entities are unsupported")),
            Event::Decl(declaration) => declaration_profile(&declaration)?,
            Event::Text(_) | Event::CData(_) | Event::Comment(_) | Event::PI(_) => nodes += 1,
            Event::Eof => break,
        }
        if nodes > MAX_NODES || attributes > MAX_ATTRIBUTES || namespaces > 512 {
            return Err(limit("XML nodes/attributes"));
        }
    }
    roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_NODES as u32,
            entity_resolver: None,
        },
    )
    .map_err(|e| invalid(format!("{part}: {e}")))
}

fn declaration_profile(declaration: &BytesDecl<'_>) -> Result<(), FormatError> {
    // Accessors alone can accept duplicate or reordered declaration attributes.
    let element = BytesStart::from_content(declaration.as_ref(), 3);
    let mut count = 0;
    let mut encoding = false;
    for attribute in element.attributes().with_checks(false) {
        let attribute = attribute.map_err(invalid)?;
        match (count, attribute.key.as_ref(), attribute.value.as_ref()) {
            (0, "version", "1.0") => {}
            (1, "encoding", value) if value.eq_ignore_ascii_case("UTF-8") => {
                encoding = true;
            }
            (1, "standalone", "yes" | "no") => {}
            (2, "standalone", "yes" | "no") if encoding => {}
            _ => {
                return Err(invalid(
                    "only a strict UTF-8 XML 1.0 declaration is supported",
                ));
            }
        }
        count += 1;
    }
    if count == 0 {
        return Err(invalid("missing declaration version"));
    }
    Ok(())
}
