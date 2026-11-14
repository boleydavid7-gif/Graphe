//! Explicit Unicode plain-text decoding and paragraph-boundary normalization.

use office_core::{Block, Document, DocumentMetadata, Id, Paragraph, Section, TextRun};

use crate::{DocumentFormat, FormatError, ImportOutcome, retain_source};

pub(crate) fn import_document(
    bytes: &[u8],
    id: Id,
    filename: &str,
) -> Result<ImportOutcome, FormatError> {
    let text = decode(bytes)?;
    if text.contains('\0') {
        return Err(FormatError::new(
            "invalid_text",
            "TXT contains NUL characters; select an explicitly encoded Unicode text file.",
        ));
    }
    // CRLF and CR represent paragraph boundaries in the plain-text profile.
    // Original bytes remain separately retained; native strings are never
    // Unicode-normalized and an interior BOM is not treated as transport data.
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.len() > crate::MAX_INPUT_BYTES || normalized.split('\n').count() > 10_000 {
        return Err(FormatError::new(
            "file_too_large",
            "TXT is limited to 16 MiB of decoded UTF-8 and 10,000 paragraphs.",
        ));
    }
    let blocks = normalized
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            Ok(Block::Paragraph(Paragraph::new(
                Id::new(format!("{}:text:p:{index}", id.as_str()))?,
                vec![TextRun::new(line)],
            )))
        })
        .collect::<Result<Vec<_>, FormatError>>()?;
    let section_id = Id::new(format!("{}:text:s:0", id.as_str()))?;
    let title = filename
        .rsplit_once('.')
        .map_or(filename, |(stem, _)| stem)
        .to_owned();
    let document = Document::from_parts(
        id,
        DocumentMetadata { title },
        vec![Section::new(section_id, blocks)],
        vec![],
    )?;
    Ok(ImportOutcome::Editable(retain_source(
        DocumentFormat::Txt,
        document,
        bytes,
        filename,
    )?))
}

fn decode(bytes: &[u8]) -> Result<String, FormatError> {
    if bytes.starts_with(&[0xff, 0xfe, 0, 0]) || bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
        return Err(FormatError::new(
            "unsupported_encoding",
            "UTF-32 TXT is not supported; save as UTF-8 or BOM-marked UTF-16.",
        ));
    }
    if let Some(body) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return utf8(body);
    }
    let (body, little_endian) = if let Some(body) = bytes.strip_prefix(&[0xff, 0xfe]) {
        (body, true)
    } else if let Some(body) = bytes.strip_prefix(&[0xfe, 0xff]) {
        (body, false)
    } else {
        return utf8(bytes);
    };
    if body.len() % 2 != 0 {
        return Err(FormatError::new(
            "invalid_encoding",
            "UTF-16 TXT has an incomplete code unit.",
        ));
    }
    let units = body.as_chunks::<2>().0.iter().map(|pair| {
        if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        }
    });
    char::decode_utf16(units)
        .map(|scalar| {
            scalar.map_err(|_| {
                FormatError::new(
                    "invalid_encoding",
                    "UTF-16 TXT contains an unpaired surrogate.",
                )
            })
        })
        .collect()
}

fn utf8(bytes: &[u8]) -> Result<String, FormatError> {
    std::str::from_utf8(bytes).map(str::to_owned).map_err(|_| FormatError::new("invalid_encoding", "TXT must be UTF-8 or have an explicit UTF-16 byte-order mark; legacy encodings are not guessed."))
}

pub(crate) fn validate_export(document: &Document) -> Result<(), FormatError> {
    if document
        .sections()
        .iter()
        .flat_map(|section| section.blocks())
        .any(|block| {
            let Block::Paragraph(paragraph) = block;
            paragraph.runs().iter().any(|run| run.text.contains('\0'))
        })
    {
        return Err(FormatError::new(
            "unsupported_txt_export",
            "TXT's editable profile cannot represent embedded NUL characters. Keep the native Writer file instead.",
        ));
    }
    Ok(())
}

fn push(output: &mut String, scalar: char) -> Result<(), FormatError> {
    if output.len().saturating_add(scalar.len_utf8()) > crate::MAX_OUTPUT_BYTES {
        return Err(FormatError::new(
            "file_too_large",
            "TXT export exceeds the output limit.",
        ));
    }
    output.push(scalar);
    Ok(())
}

pub(crate) fn export_document(document: &Document) -> Result<Vec<u8>, FormatError> {
    validate_export(document)?;
    let mut output = String::new();
    let mut first = true;
    for section in document.sections() {
        for block in section.blocks() {
            let Block::Paragraph(paragraph) = block;
            if !first {
                push(&mut output, '\n')?;
            }
            first = false;
            let mut after_cr = false;
            for scalar in paragraph.runs().iter().flat_map(|run| run.text.chars()) {
                if after_cr && scalar == '\n' {
                    after_cr = false;
                    continue;
                }
                after_cr = scalar == '\r';
                // A semantic first FEFF would otherwise be mistaken for the
                // transport BOM on reopening. One extra transport BOM protects
                // it; ordinary UTF-8 output remains BOM-free.
                if output.is_empty() && scalar == '\u{feff}' {
                    push(&mut output, '\u{feff}')?;
                }
                push(&mut output, if after_cr { '\n' } else { scalar })?;
            }
        }
    }
    Ok(output.into_bytes())
}
