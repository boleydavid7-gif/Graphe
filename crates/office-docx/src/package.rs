//! Bounded classic-ZIP reading without filesystem extraction.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
};

use zip::{
    CompressionMethod, ZipArchive,
    read::{ArchiveOffset, Config},
};

use crate::{
    DocxError, MAX_EXPANDED_BYTES, MAX_INPUT_BYTES, MAX_PACKAGE_ENTRIES, MAX_PART_BYTES,
    error::limit,
};

const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
const CENTRAL_SIGNATURE: &[u8; 4] = b"PK\x01\x02";
const LOCAL_SIGNATURE: &[u8; 4] = b"PK\x03\x04";
const ZIP64_LOCATOR_SIGNATURE: &[u8; 4] = b"PK\x06\x07";
const DATA_DESCRIPTOR_SIGNATURE: &[u8; 4] = b"PK\x07\x08";
const EOCD_LENGTH: usize = 22;
const CENTRAL_LENGTH: usize = 46;

pub(crate) struct Package {
    pub parts: BTreeMap<String, Vec<u8>>,
    pub total_uncompressed_bytes: u64,
}

struct Directory {
    start: usize,
    count: usize,
}

pub(crate) fn read_package(bytes: &[u8]) -> Result<Package, DocxError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(limit("compressed_bytes", MAX_INPUT_BYTES, bytes.len()));
    }
    // The ZIP index has no public entry/allocation limit. Inspect the classic
    // directory before allowing the library to allocate that index.
    let directory = preflight_directory(bytes)?;
    let mut archive = ZipArchive::with_config(
        Config {
            archive_offset: ArchiveOffset::Known(0),
        },
        Cursor::new(bytes),
    )
    .map_err(invalid)?;
    if archive.has_duplicate_names() {
        return Err(invalid("duplicate package entry names"));
    }
    if archive.len() != directory.count
        || archive.central_directory_start() != directory.start as u64
        || archive.offset() != 0
    {
        return Err(invalid("ZIP index disagrees with the preflight directory"));
    }
    archive.validate_header_consistency().map_err(invalid)?;
    if archive.has_overlapping_entries().map_err(invalid)? {
        return Err(invalid("overlapping package entries"));
    }

    let mut parts = BTreeMap::new();
    let mut total = 0_usize;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(invalid)?;
        let raw_name = ascii_name(file.name_raw())?;
        // Unicode extra fields must not give a canonical ASCII member another
        // identity. No URI or filename normalization is performed here.
        if file.name().map_err(invalid)?.as_ref() != raw_name {
            return Err(invalid("conflicting ZIP filename encodings"));
        }
        let name = raw_name.to_owned();
        let is_directory = file.is_dir();
        if file.is_symlink() {
            return Err(unsupported("symbolic-link package entries"));
        }
        if file.encrypted() {
            return Err(unsupported("encrypted package entries"));
        }
        if !matches!(
            file.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err(unsupported("compression other than stored or deflate"));
        }
        let declared_size = file.size();
        let mut content = Vec::new();
        let mut chunk = [0_u8; 8192];
        loop {
            // Always read to actual EOF: a ZIP CRC check may run only then.
            let read = file.read(&mut chunk).map_err(invalid)?;
            if read == 0 {
                break;
            }
            let observed = content.len() + read;
            if observed > MAX_PART_BYTES {
                return Err(limit("part_bytes", MAX_PART_BYTES, observed));
            }
            if total + observed > MAX_EXPANDED_BYTES {
                return Err(limit(
                    "expanded_bytes",
                    MAX_EXPANDED_BYTES,
                    total + observed,
                ));
            }
            content.extend_from_slice(&chunk[..read]);
        }
        if content.len() as u64 != declared_size {
            return Err(invalid("observed member size differs from its ZIP header"));
        }
        total += content.len();
        if !is_directory {
            parts.insert(name, content);
        }
    }
    Ok(Package {
        parts,
        total_uncompressed_bytes: total as u64,
    })
}

/// The prototype deliberately accepts a narrower spelling than all OPC URIs.
pub(crate) fn validate_part_name(name: &str) -> Result<(), DocxError> {
    if !name.is_ascii() {
        return Err(unsupported("non-ASCII package part names"));
    }
    if name.contains('%') {
        return Err(unsupported("percent-escaped package part names"));
    }
    if name == "[Content_Types].xml" {
        return Ok(());
    }
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-/".contains(&byte))
        || name
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment.ends_with('.'))
    {
        return Err(invalid(format!("noncanonical package part name: {name:?}")));
    }
    Ok(())
}

fn ascii_name(bytes: &[u8]) -> Result<&str, DocxError> {
    std::str::from_utf8(bytes)
        .map_err(|_| unsupported("non-ASCII package part names"))
        .and_then(|name| {
            if name.is_ascii() {
                Ok(name)
            } else {
                Err(unsupported("non-ASCII package part names"))
            }
        })
}

fn preflight_directory(bytes: &[u8]) -> Result<Directory, DocxError> {
    let eocd = locate_eocd(bytes)?;
    let disk = word(bytes, eocd + 4)?;
    let directory_disk = word(bytes, eocd + 6)?;
    let disk_count = word(bytes, eocd + 8)?;
    let count = word(bytes, eocd + 10)?;
    let size = dword(bytes, eocd + 12)?;
    let start = dword(bytes, eocd + 16)?;
    if count == u16::MAX
        || disk_count == u16::MAX
        || size == u32::MAX
        || start == u32::MAX
        || eocd.checked_sub(20).is_some_and(|position| {
            bytes.get(position..position + 4) == Some(ZIP64_LOCATOR_SIGNATURE)
        })
    {
        return Err(unsupported("ZIP64 archives"));
    }
    if disk != 0 || directory_disk != 0 || disk_count != count {
        return Err(unsupported("multidisk ZIP archives"));
    }
    let count = usize::from(count);
    if count > MAX_PACKAGE_ENTRIES {
        return Err(limit("package_entries", MAX_PACKAGE_ENTRIES, count));
    }
    let start = start as usize;
    let end = start
        .checked_add(size as usize)
        .filter(|end| *end <= eocd)
        .ok_or_else(|| invalid("central directory outside archive bounds"))?;
    if end != eocd {
        return Err(unsupported(
            "data between the central directory and its end record",
        ));
    }

    let mut position = start;
    let mut observed_count = 0_usize;
    let mut total_size = 0_u64;
    let mut names = BTreeSet::new();
    let mut first_local = None;
    let mut member_ranges = Vec::new();
    while position < end {
        observed_count += 1;
        if observed_count > MAX_PACKAGE_ENTRIES {
            return Err(limit(
                "package_entries",
                MAX_PACKAGE_ENTRIES,
                observed_count,
            ));
        }
        if bytes.get(position..position + 4) != Some(CENTRAL_SIGNATURE)
            || position + CENTRAL_LENGTH > end
        {
            return Err(invalid("invalid or truncated central directory header"));
        }
        let flags = word(bytes, position + 8)?;
        let compression = word(bytes, position + 10)?;
        let compressed_size = dword(bytes, position + 20)?;
        let expanded_size = dword(bytes, position + 24)?;
        let name_length = usize::from(word(bytes, position + 28)?);
        let extra_length = usize::from(word(bytes, position + 30)?);
        let comment_length = usize::from(word(bytes, position + 32)?);
        let entry_disk = word(bytes, position + 34)?;
        let local = dword(bytes, position + 42)?;
        if compressed_size == u32::MAX || expanded_size == u32::MAX || local == u32::MAX {
            return Err(unsupported("ZIP64 archive entries"));
        }
        if entry_disk != 0 {
            return Err(unsupported("multidisk ZIP archive entries"));
        }
        if flags & ((1 << 0) | (1 << 6) | (1 << 13)) != 0 {
            return Err(unsupported("encrypted package entries"));
        }
        if compression != 0 && compression != 8 {
            return Err(unsupported("compression other than stored or deflate"));
        }
        if expanded_size as u64 > MAX_PART_BYTES as u64 {
            return Err(DocxError::LimitExceeded {
                resource: "part_bytes",
                limit: MAX_PART_BYTES as u64,
                actual: expanded_size as u64,
            });
        }
        total_size += u64::from(expanded_size);
        if total_size > MAX_EXPANDED_BYTES as u64 {
            return Err(DocxError::LimitExceeded {
                resource: "expanded_bytes",
                limit: MAX_EXPANDED_BYTES as u64,
                actual: total_size,
            });
        }
        let next = position + CENTRAL_LENGTH + name_length + extra_length + comment_length;
        if next > end {
            return Err(invalid(
                "central directory entry extends beyond the directory",
            ));
        }
        let name_start = position + CENTRAL_LENGTH;
        let name_end = name_start + name_length;
        let name = ascii_name(&bytes[name_start..name_end])?;
        let canonical = name.strip_suffix('/').unwrap_or(name);
        validate_part_name(canonical)?;
        if name.ends_with('/') && expanded_size != 0 {
            return Err(invalid("directory entry contains document data"));
        }
        if !names.insert(canonical.to_ascii_lowercase()) {
            return Err(invalid("duplicate or case-aliased package entry names"));
        }
        validate_extra_fields(&bytes[name_end..name_end + extra_length])?;
        let local = local as usize;
        if local >= start || local + 30 > start {
            return Err(invalid("local file header outside the member-data region"));
        }
        let data_start = validate_local_header(bytes, local, start)?;
        let data_end = data_start
            .checked_add(compressed_size as usize)
            .filter(|end| *end <= start)
            .ok_or_else(|| invalid("member payload extends beyond the member-data region"))?;
        let uses_descriptor = flags & (1 << 3) != 0;
        if uses_descriptor != (word(bytes, local + 6)? & (1 << 3) != 0) {
            return Err(invalid(
                "inconsistent local and central data-descriptor flags",
            ));
        }
        let member_end = if uses_descriptor {
            validate_data_descriptor(
                bytes,
                data_end,
                start,
                [dword(bytes, position + 16)?, compressed_size, expanded_size],
            )?
        } else {
            data_end
        };
        member_ranges.push(local..member_end);
        first_local = Some(first_local.map_or(local, |first: usize| first.min(local)));
        position = next;
    }
    if observed_count != count {
        return Err(invalid(
            "central directory count differs from its end record",
        ));
    }
    if first_local.unwrap_or(eocd) != 0 {
        return Err(unsupported("self-extracting or prefixed ZIP archives"));
    }
    member_ranges.sort_unstable_by_key(|range| range.start);
    if member_ranges
        .windows(2)
        .any(|pair| pair[0].end > pair[1].start)
    {
        return Err(invalid(
            "overlapping member headers, payloads, or data descriptors",
        ));
    }
    if count > 0 && bytes.get(..4) != Some(LOCAL_SIGNATURE) {
        return Err(invalid(
            "archive does not begin with its first local file header",
        ));
    }
    Ok(Directory { start, count })
}

fn validate_local_header(
    bytes: &[u8],
    position: usize,
    directory: usize,
) -> Result<usize, DocxError> {
    if bytes.get(position..position + 4) != Some(LOCAL_SIGNATURE) {
        return Err(invalid("invalid local file header signature"));
    }
    if dword(bytes, position + 18)? == u32::MAX || dword(bytes, position + 22)? == u32::MAX {
        return Err(unsupported("ZIP64 local entry sizes"));
    }
    if word(bytes, position + 6)? & ((1 << 0) | (1 << 6) | (1 << 13)) != 0 {
        return Err(unsupported("encrypted local package entries"));
    }
    let name_length = usize::from(word(bytes, position + 26)?);
    let extra_length = usize::from(word(bytes, position + 28)?);
    let extra_start = position + 30 + name_length;
    let extra_end = extra_start + extra_length;
    if extra_end > directory {
        return Err(invalid("local file header overlaps the central directory"));
    }
    validate_extra_fields(&bytes[extra_start..extra_end])?;
    Ok(extra_end)
}

fn validate_data_descriptor(
    bytes: &[u8],
    position: usize,
    directory: usize,
    expected: [u32; 3],
) -> Result<usize, DocxError> {
    // The signature is optional. A legitimate unsigned descriptor can begin
    // with a CRC whose value equals that signature, so compare whole candidate
    // records with the central metadata instead of guessing from four bytes.
    let matches = |start| {
        expected
            .iter()
            .enumerate()
            .all(|(index, value)| dword(bytes, start + index * 4).ok() == Some(*value))
    };
    if position + 12 <= directory && matches(position) {
        return Ok(position + 12);
    }
    if position + 16 <= directory
        && bytes.get(position..position + 4) == Some(DATA_DESCRIPTOR_SIGNATURE)
        && matches(position + 4)
    {
        return Ok(position + 16);
    }
    Err(invalid(
        "missing, truncated, or inconsistent ZIP data descriptor",
    ))
}

fn validate_extra_fields(bytes: &[u8]) -> Result<(), DocxError> {
    let mut position = 0;
    while position < bytes.len() {
        let kind = word(bytes, position)?;
        let length = usize::from(word(bytes, position + 2)?);
        position += 4;
        if position + length > bytes.len() {
            return Err(invalid("truncated ZIP extra field"));
        }
        if kind == 1 {
            return Err(unsupported("ZIP64 entry extra fields"));
        }
        position += length;
    }
    Ok(())
}

fn locate_eocd(bytes: &[u8]) -> Result<usize, DocxError> {
    if bytes.len() < EOCD_LENGTH {
        return Err(invalid("missing or truncated ZIP end record"));
    }
    let first_possible = bytes.len().saturating_sub(EOCD_LENGTH + u16::MAX as usize);
    let mut trailing_data = false;
    for position in (first_possible..=bytes.len() - EOCD_LENGTH).rev() {
        if bytes.get(position..position + 4) == Some(EOCD_SIGNATURE) {
            let comment_length = usize::from(word(bytes, position + 20)?);
            let end = position + EOCD_LENGTH + comment_length;
            if end == bytes.len() {
                // zip accepts end records before EOF and scans signatures in
                // comments too. Reject that ambiguity before its index is
                // allocated, so it cannot choose an unbudgeted second record.
                if bytes[position + EOCD_LENGTH..]
                    .windows(EOCD_SIGNATURE.len())
                    .any(|window| window == EOCD_SIGNATURE)
                {
                    return Err(unsupported(
                        "ambiguous ZIP end records inside the archive comment",
                    ));
                }
                return Ok(position);
            }
            trailing_data |= end < bytes.len();
        }
    }
    if trailing_data {
        Err(unsupported("bytes appended after the ZIP end record"))
    } else {
        Err(invalid("missing or truncated ZIP end record"))
    }
}

fn word(bytes: &[u8], position: usize) -> Result<u16, DocxError> {
    let raw = bytes
        .get(position..position + 2)
        .ok_or_else(|| invalid("truncated ZIP header"))?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn dword(bytes: &[u8], position: usize) -> Result<u32, DocxError> {
    let raw = bytes
        .get(position..position + 4)
        .ok_or_else(|| invalid("truncated ZIP header"))?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn invalid(reason: impl std::fmt::Display) -> DocxError {
    DocxError::InvalidArchive {
        reason: reason.to_string(),
    }
}

fn unsupported(feature: impl Into<String>) -> DocxError {
    DocxError::Unsupported {
        feature: feature.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    fn archive(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(method);
        for (name, content) in entries {
            if name.ends_with('/') {
                writer.add_directory(*name, options).unwrap();
            } else {
                writer.start_file(*name, options).unwrap();
                writer.write_all(content).unwrap();
            }
        }
        writer.finish().unwrap().into_inner()
    }

    fn stored(entries: &[(&str, &[u8])]) -> Vec<u8> {
        archive(entries, CompressionMethod::Stored)
    }

    fn central(bytes: &[u8]) -> usize {
        let eocd = locate_eocd(bytes).unwrap();
        dword(bytes, eocd + 16).unwrap() as usize
    }

    fn set_word(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn set_dword(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn with_descriptor(
        content: &[u8],
        signed: bool,
        method: CompressionMethod,
    ) -> (Vec<u8>, usize) {
        let mut bytes = archive(&[("part.bin", content)], method);
        let header = central(&bytes);
        let fields = [
            dword(&bytes, header + 16).unwrap(),
            dword(&bytes, header + 20).unwrap(),
            dword(&bytes, header + 24).unwrap(),
        ];
        set_word(&mut bytes, 6, 1 << 3);
        set_word(&mut bytes, header + 8, 1 << 3);
        for offset in [14, 18, 22] {
            set_dword(&mut bytes, offset, 0);
        }
        let mut descriptor = Vec::new();
        if signed {
            descriptor.extend_from_slice(DATA_DESCRIPTOR_SIGNATURE);
        }
        for value in fields {
            descriptor.extend_from_slice(&value.to_le_bytes());
        }
        let length = descriptor.len();
        bytes.splice(header..header, descriptor);
        let eocd = locate_eocd(&bytes).unwrap();
        set_dword(&mut bytes, eocd + 16, (header + length) as u32);
        (bytes, header)
    }

    #[test]
    fn signed_and_unsigned_classic_data_descriptors_are_supported() {
        for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
            for signed in [false, true] {
                let (bytes, _) = with_descriptor(b"Writer document data", signed, method);
                let package = read_package(&bytes).unwrap();
                assert_eq!(package.parts["part.bin"], b"Writer document data");
            }
        }
    }

    #[test]
    fn unsigned_descriptor_crc_may_equal_the_optional_signature() {
        // This four-byte payload has CRC32 0x08074b50: its unsigned descriptor
        // must not be mistaken for a signed descriptor missing four bytes.
        let content = [0xac, 0x0a, 0x7a, 0xd5];
        let (bytes, descriptor) = with_descriptor(&content, false, CompressionMethod::Stored);
        assert_eq!(
            &bytes[descriptor..descriptor + 4],
            DATA_DESCRIPTOR_SIGNATURE
        );
        assert_eq!(read_package(&bytes).unwrap().parts["part.bin"], content);
    }

    #[test]
    fn inconsistent_and_truncated_data_descriptors_are_rejected() {
        for signed in [false, true] {
            let (original, descriptor) =
                with_descriptor(b"hello", signed, CompressionMethod::Stored);
            for field in 0..3 {
                let mut bytes = original.clone();
                set_dword(
                    &mut bytes,
                    descriptor + usize::from(signed) * 4 + field * 4,
                    0,
                );
                assert!(matches!(
                    read_package(&bytes),
                    Err(DocxError::InvalidArchive { .. })
                ));
            }
            let length = if signed { 16 } else { 12 };
            for removed in 1..=length {
                let mut bytes = original.clone();
                bytes.drain(descriptor + length - removed..descriptor + length);
                let eocd = locate_eocd(&bytes).unwrap();
                set_dword(
                    &mut bytes,
                    eocd + 16,
                    (descriptor + length - removed) as u32,
                );
                assert!(matches!(
                    read_package(&bytes),
                    Err(DocxError::InvalidArchive { .. })
                ));
            }
        }
    }

    #[test]
    fn local_and_central_descriptor_flags_must_agree() {
        let (original, _) = with_descriptor(b"hello", true, CompressionMethod::Stored);
        for local in [false, true] {
            let mut bytes = original.clone();
            let header = central(&bytes);
            set_word(&mut bytes, if local { 6 } else { header + 8 }, 0);
            assert!(matches!(
                read_package(&bytes),
                Err(DocxError::InvalidArchive { .. })
            ));
        }
    }

    #[test]
    fn descriptors_cannot_overlap_another_members_local_header() {
        let mut bytes = stored(&[("one", b"one"), ("two", b"two")]);
        let first = central(&bytes);
        let second = first + CENTRAL_LENGTH + 3;
        let next_local = dword(&bytes, second + 42).unwrap() as usize;
        let first_data = 30 + usize::from(word(&bytes, 26).unwrap());
        let descriptor = next_local + 10;
        let compressed = (descriptor - first_data) as u32;
        // Forge a descriptor across the second header's time/date, CRC and
        // compressed-size fields. Every descriptor field matches the first
        // central header; the occupied ranges must still reject the overlap.
        let crc = dword(&bytes, descriptor).unwrap();
        let expanded = dword(&bytes, next_local + 18).unwrap();
        set_dword(&mut bytes, next_local + 14, compressed);
        set_dword(&mut bytes, second + 16, compressed);
        set_word(&mut bytes, 6, 1 << 3);
        set_word(&mut bytes, first + 8, 1 << 3);
        set_dword(&mut bytes, first + 16, crc);
        set_dword(&mut bytes, first + 20, compressed);
        set_dword(&mut bytes, first + 24, expanded);
        assert!(matches!(
            preflight_directory(&bytes),
            Err(DocxError::InvalidArchive { reason }) if reason.contains("overlapping")
        ));
    }

    #[test]
    fn stored_and_deflated_parts_preserve_exact_bytes() {
        let text = "Writer: e\u{301} 中文 👩🏽‍💻\r\n";
        for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
            let bytes = archive(
                &[
                    ("word/", b""),
                    ("word/document.xml", text.as_bytes()),
                    ("[Content_Types].xml", b"<Types/>"),
                ],
                method,
            );
            let package = read_package(&bytes).unwrap();
            assert_eq!(package.parts.len(), 2);
            assert_eq!(package.parts["word/document.xml"], text.as_bytes());
            assert_eq!(
                package.total_uncompressed_bytes,
                (text.len() + b"<Types/>".len()) as u64
            );
        }
    }

    #[test]
    fn standard_zip_comments_and_empty_zip_are_supported() {
        let mut bytes = stored(&[("part.xml", b"hello")]);
        let eocd = locate_eocd(&bytes).unwrap();
        set_word(&mut bytes, eocd + 20, 7);
        bytes.extend_from_slice(b"comment");
        assert!(read_package(&bytes).is_ok());
        let package = read_package(&stored(&[])).unwrap();
        assert!(package.parts.is_empty());
        assert_eq!(package.total_uncompressed_bytes, 0);
    }

    #[test]
    fn end_record_signatures_in_comments_cannot_bypass_the_preflight() {
        let mut bytes = stored(&[("part.xml", b"hello")]);
        let eocd = locate_eocd(&bytes).unwrap();
        let mut comment = vec![0_u8; EOCD_LENGTH + 1];
        comment[..4].copy_from_slice(EOCD_SIGNATURE);
        // The library accepts this record's comment ending one byte before
        // EOF, whereas the adapter selected the real earlier end record.
        set_word(&mut comment, 8, 257);
        set_word(&mut comment, 10, 257);
        set_word(&mut bytes, eocd + 20, comment.len() as u16);
        bytes.extend_from_slice(&comment);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::Unsupported { .. })
        ));
    }

    #[test]
    fn part_names_are_validated_without_normalization() {
        for name in ["word/document.xml", "_rels/.rels", "[Content_Types].xml"] {
            assert!(validate_part_name(name).is_ok(), "{name}");
        }
        for name in [
            "",
            "/word.xml",
            "word/",
            "word//a.xml",
            "./a.xml",
            "../a.xml",
            "word/../a.xml",
            "word/part.",
            "word\\a.xml",
            "word/a#x.xml",
            "word/a?x.xml",
            "word/a:b.xml",
            "word/a\0.xml",
            "word/a b.xml",
        ] {
            assert!(
                matches!(
                    validate_part_name(name),
                    Err(DocxError::InvalidArchive { .. })
                ),
                "{name:?}"
            );
        }
        for name in ["word/%61.xml", "word/é.xml"] {
            assert!(matches!(
                validate_part_name(name),
                Err(DocxError::Unsupported { .. })
            ));
            assert!(matches!(
                read_package(&stored(&[(name, b"content")])),
                Err(DocxError::Unsupported { .. })
            ));
        }
    }

    #[test]
    fn case_aliases_and_exact_duplicate_central_names_are_rejected() {
        let case_aliases = stored(&[("word/one.xml", b"one"), ("WORD/ONE.xml", b"two")]);
        assert!(matches!(
            read_package(&case_aliases),
            Err(DocxError::InvalidArchive { .. })
        ));
        let mut duplicate = stored(&[("one", b"one"), ("two", b"two")]);
        let first = central(&duplicate);
        let second = first + CENTRAL_LENGTH + 3;
        duplicate[second + CENTRAL_LENGTH..second + CENTRAL_LENGTH + 3].copy_from_slice(b"one");
        assert!(matches!(
            read_package(&duplicate),
            Err(DocxError::InvalidArchive { .. })
        ));
    }

    #[test]
    fn counts_are_checked_before_allocating_the_zip_index() {
        let mut bytes = stored(&[("part.xml", b"hello")]);
        let eocd = locate_eocd(&bytes).unwrap();
        set_word(&mut bytes, eocd + 8, 0);
        set_word(&mut bytes, eocd + 10, 0);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::InvalidArchive { .. })
        ));
        set_word(&mut bytes, eocd + 8, 257);
        set_word(&mut bytes, eocd + 10, 257);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::LimitExceeded {
                resource: "package_entries",
                ..
            })
        ));
        // A forged small advertised count cannot hide excessive actual headers.
        let entries: Vec<_> = (0..257)
            .map(|index| (format!("part{index}.xml"), b"x".as_slice()))
            .collect();
        let borrowed: Vec<_> = entries
            .iter()
            .map(|(name, value)| (name.as_str(), *value))
            .collect();
        let mut bytes = stored(&borrowed);
        let eocd = locate_eocd(&bytes).unwrap();
        set_word(&mut bytes, eocd + 8, 1);
        set_word(&mut bytes, eocd + 10, 1);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::LimitExceeded {
                resource: "package_entries",
                ..
            })
        ));
    }

    #[test]
    fn invalid_directory_bounds_and_truncated_headers_are_rejected() {
        assert!(read_package(b"not a ZIP").is_err());
        let mut bytes = stored(&[("part.xml", b"hello")]);
        let eocd = locate_eocd(&bytes).unwrap();
        set_dword(&mut bytes, eocd + 16, 0xffff_fffe);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::InvalidArchive { .. })
        ));
        let mut bytes = stored(&[("part.xml", b"hello")]);
        let header = central(&bytes);
        set_word(&mut bytes, header + 28, u16::MAX);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::InvalidArchive { .. })
        ));
    }

    #[test]
    fn every_truncated_prefix_is_rejected_without_panicking() {
        let bytes = stored(&[("word/document.xml", b"document text")]);
        for end in 0..bytes.len() {
            assert!(read_package(&bytes[..end]).is_err(), "prefix length {end}");
        }
    }

    #[test]
    fn zip64_multidisk_prefixes_and_appended_data_are_explicitly_unsupported() {
        let original = stored(&[("part.xml", b"hello")]);
        let mut bytes = original.clone();
        let eocd = locate_eocd(&bytes).unwrap();
        set_word(&mut bytes, eocd + 10, u16::MAX);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::Unsupported { .. })
        ));
        let mut bytes = original.clone();
        set_word(&mut bytes, eocd + 4, 1);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::Unsupported { .. })
        ));
        let mut bytes = original.clone();
        bytes.push(0);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::Unsupported { .. })
        ));
        let mut bytes = Vec::from(b"MZ".as_slice());
        bytes.extend_from_slice(&original);
        let header = central(&original) + 2;
        set_dword(&mut bytes, header + 42, 2);
        set_dword(&mut bytes, eocd + 2 + 16, header as u32);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::Unsupported { .. })
        ));
    }

    #[test]
    fn zip64_local_fields_are_rejected_even_with_a_classic_central_directory() {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("part.xml", SimpleFileOptions::default().large_file(true))
            .unwrap();
        writer.write_all(b"hello").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::Unsupported { .. })
        ));
    }

    #[test]
    fn encrypted_entries_and_other_compressors_are_rejected_before_reading() {
        for (field, value) in [(8, 1), (8, 1 << 6), (10, 12)] {
            let mut bytes = stored(&[("part.xml", b"hello")]);
            let header = central(&bytes);
            set_word(&mut bytes, header + field, value);
            assert!(matches!(
                read_package(&bytes),
                Err(DocxError::Unsupported { .. })
            ));
        }
    }

    #[test]
    fn crc_header_mismatch_and_overlapping_members_are_rejected() {
        let mut bytes = stored(&[("part.xml", b"hello")]);
        let payload =
            30 + usize::from(word(&bytes, 26).unwrap()) + usize::from(word(&bytes, 28).unwrap());
        bytes[payload] ^= 1;
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::InvalidArchive { .. })
        ));
        let mut bytes = stored(&[("part.xml", b"hello")]);
        bytes[30] = b'P';
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::InvalidArchive { .. })
        ));
        let mut bytes = stored(&[("one", b"one"), ("two", b"two")]);
        let first = central(&bytes);
        let second = first + CENTRAL_LENGTH + 3;
        set_dword(&mut bytes, second + 42, 0);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::InvalidArchive { .. })
        ));
    }

    #[test]
    fn symlinks_and_data_in_directory_entries_are_rejected() {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .add_symlink("link", "word/document.xml", SimpleFileOptions::default())
            .unwrap();
        assert!(matches!(
            read_package(&writer.finish().unwrap().into_inner()),
            Err(DocxError::Unsupported { .. })
        ));
        let mut bytes = stored(&[("part.xml", b"hello")]);
        let header = central(&bytes);
        bytes[30 + 7] = b'/';
        bytes[header + CENTRAL_LENGTH + 7] = b'/';
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::InvalidArchive { .. })
        ));
    }

    #[test]
    fn advertised_expansion_limits_are_checked_without_allocating_claimed_sizes() {
        let mut bytes = stored(&[("part.xml", b"hello")]);
        let header = central(&bytes);
        set_dword(&mut bytes, header + 24, MAX_PART_BYTES as u32 + 1);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::LimitExceeded {
                resource: "part_bytes",
                ..
            })
        ));
        let mut bytes = stored(&[
            ("one", b"x"),
            ("two", b"x"),
            ("thr", b"x"),
            ("for", b"x"),
            ("fiv", b"x"),
        ]);
        let first = central(&bytes);
        for index in 0..5 {
            set_dword(
                &mut bytes,
                first + index * (CENTRAL_LENGTH + 3) + 24,
                MAX_PART_BYTES as u32,
            );
        }
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::LimitExceeded {
                resource: "expanded_bytes",
                ..
            })
        ));
    }

    #[test]
    fn forged_small_expansion_sizes_do_not_hide_large_deflate_payloads() {
        let mut bytes = archive(
            &[("part.xml", &vec![b'x'; 100_000])],
            CompressionMethod::Deflated,
        );
        let header = central(&bytes);
        set_dword(&mut bytes, header + 24, 1);
        set_dword(&mut bytes, 22, 1);
        assert!(matches!(
            read_package(&bytes),
            Err(DocxError::InvalidArchive { .. })
        ));
    }

    #[test]
    fn input_limit_is_independent_of_archive_validity() {
        assert!(matches!(
            read_package(&vec![0; MAX_INPUT_BYTES + 1]),
            Err(DocxError::LimitExceeded {
                resource: "compressed_bytes",
                ..
            })
        ));
    }
}
