use quick_xml::{
    Reader, XmlVersion,
    events::{BytesDecl, BytesStart, Event},
};

use crate::{
    DocxError, MAX_XML_ATTRIBUTES, MAX_XML_BYTES, MAX_XML_DEPTH, MAX_XML_NAMESPACE_DECLARATIONS,
    MAX_XML_NODES, MAX_XML_TOTAL_ATTRIBUTES, error::limit,
};

/// Parses the bounded UTF-8 XML 1.0 profile without resolving external entities.
///
/// The streaming pass bounds work before the namespace-aware DOM parser allocates
/// its indexes. Namespace and expanded-name checks remain the DOM parser's job.
pub(crate) fn parse_xml<'a>(
    part: &str,
    bytes: &'a [u8],
) -> Result<roxmltree::Document<'a>, DocxError> {
    check_limit("xml_bytes", MAX_XML_BYTES, bytes.len())?;
    reject_other_encoding(part, bytes)?;
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    preflight(part, bytes)?;

    let text = std::str::from_utf8(bytes).map_err(|error| invalid_xml(part, error))?;

    // roxmltree 0.21.2 initially reserves capacity from these raw markers, even
    // inside text/comments/CDATA. Bound those allocations independently of the
    // event counts; these are explicit profile limits, not XML syntax rules.
    check_limit(
        "xml_node_markers",
        MAX_XML_NODES,
        bytes.iter().filter(|&&byte| byte == b'<').count(),
    )?;
    check_limit(
        "xml_attribute_markers",
        MAX_XML_TOTAL_ATTRIBUTES,
        bytes.iter().filter(|&&byte| byte == b'=').count(),
    )?;

    roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_XML_NODES as u32,
            entity_resolver: None,
        },
    )
    .map_err(|error| match error {
        roxmltree::Error::NodesLimitReached => limit("xml_nodes", MAX_XML_NODES, MAX_XML_NODES + 1),
        error => invalid_xml(part, error),
    })
}

fn reject_other_encoding(part: &str, bytes: &[u8]) -> Result<(), DocxError> {
    // XML's initial-byte patterns identify UTF-16/32 before UTF-8 decoding can
    // turn a recognized unsupported encoding into an opaque syntax error.
    let unsupported = bytes.starts_with(b"\xff\xfe")
        || bytes.starts_with(b"\xfe\xff")
        || bytes.starts_with(b"\x00\x00\xfe\xff")
        || bytes.starts_with(b"\x00\x00\x00<")
        || bytes.starts_with(b"<\x00\x00\x00")
        || bytes.starts_with(b"\x00<\x00?")
        || bytes.starts_with(b"<\x00?\x00");
    if unsupported {
        return Err(DocxError::Unsupported {
            feature: format!("non-UTF-8 XML encoding in {part}"),
        });
    }
    Ok(())
}

fn preflight(part: &str, bytes: &[u8]) -> Result<(), DocxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_comments = true;
    let mut depth = 0;
    let mut nodes = 1; // The DOM includes an implicit document root.
    let mut total_attributes = 0;
    let mut namespace_declarations = 0;
    let mut previous_text = false;

    loop {
        let event = reader
            .read_event()
            .map_err(|error| invalid_xml(part, error))?;
        if let Event::GeneralRef(reference) = &event {
            // roxmltree 0.21.2 substitutes U+FFFD for surrogate/out-of-range
            // numeric references. Reject those malformed scalar values first.
            reference
                .resolve_char_ref()
                .map_err(|error| invalid_xml(part, error))?;
        }
        let empty_element = matches!(event, Event::Empty(_));
        match event {
            Event::Start(element) | Event::Empty(element) => {
                check_limit("xml_depth", MAX_XML_DEPTH, depth + 1)?;
                count_attributes(
                    part,
                    &element,
                    &mut total_attributes,
                    &mut namespace_declarations,
                )?;
                nodes += 1;
                previous_text = false;
                if !empty_element {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                previous_text = false;
            }
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) => {
                // Entity references and adjacent CDATA/text become one DOM
                // text node. Top-level whitespace is not a document child.
                if depth > 0 && !previous_text {
                    nodes += 1;
                }
                previous_text = true;
            }
            Event::Comment(_) | Event::PI(_) => {
                nodes += 1;
                previous_text = false;
            }
            Event::Decl(declaration) => validate_declaration(part, &declaration)?,
            Event::DocType(_) => {
                // Also reject an empty DTD, which allow_dtd=false alone permits.
                return Err(DocxError::Unsupported {
                    feature: format!("XML document type declaration in {part}"),
                });
            }
            Event::Eof => break,
        }
        check_limit("xml_nodes", MAX_XML_NODES, nodes)?;
    }
    Ok(())
}

fn count_attributes(
    part: &str,
    element: &BytesStart<'_>,
    total: &mut usize,
    namespaces: &mut usize,
) -> Result<(), DocxError> {
    // Disabling duplicate checks here avoids a quadratic scan. Counts are
    // bounded before value/namespace processing. The DOM subsequently rejects
    // duplicate expanded attribute names.
    let mut count = 0;
    for (index, attribute) in element.attributes().with_checks(false).enumerate() {
        check_limit("xml_attributes", MAX_XML_ATTRIBUTES, index + 1)?;
        attribute.map_err(|error| invalid_xml(part, error))?;
        count += 1;
    }
    *total += count;
    check_limit("xml_total_attributes", MAX_XML_TOTAL_ATTRIBUTES, *total)?;

    let mut declared_namespaces = std::collections::BTreeSet::new();
    for attribute in element.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| invalid_xml(part, error))?;
        // Validate references without changing the original value. This also
        // prevents the DOM's invalid-scalar substitution in attribute values.
        let value = attribute
            .normalized_value(XmlVersion::Explicit1_0)
            .map_err(|error| invalid_xml(part, error))?;
        let name = attribute.key.into_inner();
        if name == "xmlns" || name.starts_with("xmlns:") {
            *namespaces += 1;
            check_limit(
                "xml_namespace_declarations",
                MAX_XML_NAMESPACE_DECLARATIONS,
                *namespaces,
            )?;
            // roxmltree 0.21.2 does not reject xmlns:xmlns or duplicate default/
            // xml bindings. Namespace 1.0 also forbids undeclaring a prefix.
            if name == "xmlns:xmlns"
                || (name != "xmlns" && value.is_empty())
                || !declared_namespaces.insert(name)
            {
                return Err(invalid_xml(
                    part,
                    "invalid or duplicate namespace declaration",
                ));
            }
        }
    }
    Ok(())
}

fn validate_declaration(part: &str, declaration: &BytesDecl<'_>) -> Result<(), DocxError> {
    // Declaration accessors intentionally ignore ordering and duplicates.
    // Parse its three permitted pseudo-attributes in their specified order.
    let element = BytesStart::from_content(declaration.as_ref(), 3);
    let mut count = 0;
    let mut encoding_seen = false;
    for attribute in element.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| invalid_xml(part, error))?;
        let name = attribute.key.as_ref();
        let value = attribute.value.as_ref();
        match (count, name) {
            (0, "version") => {
                let valid_number = value.split_once('.').is_some_and(|(major, minor)| {
                    !major.is_empty()
                        && !minor.is_empty()
                        && major.bytes().all(|byte| byte.is_ascii_digit())
                        && minor.bytes().all(|byte| byte.is_ascii_digit())
                });
                if !valid_number {
                    return Err(invalid_xml(part, "malformed XML version"));
                }
                if value != "1.0" {
                    return Err(DocxError::Unsupported {
                        feature: format!("XML version {value} in {part}"),
                    });
                }
            }
            (1, "encoding") => {
                let mut bytes = value.bytes();
                if !bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
                    || !bytes.all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                    })
                {
                    return Err(invalid_xml(part, "malformed XML encoding name"));
                }
                if !value.eq_ignore_ascii_case("UTF-8") {
                    return Err(DocxError::Unsupported {
                        feature: format!("XML encoding {value} in {part}"),
                    });
                }
                encoding_seen = true;
            }
            (1, "standalone") | (2, "standalone") if count == 1 || encoding_seen => {
                if !matches!(value, "yes" | "no") {
                    return Err(invalid_xml(part, "XML standalone must be yes or no"));
                }
            }
            _ => return Err(invalid_xml(part, "invalid XML declaration attribute order")),
        }
        count += 1;
    }
    if count == 0 {
        return Err(invalid_xml(part, "missing XML declaration version"));
    }
    Ok(())
}

fn check_limit(resource: &'static str, maximum: usize, actual: usize) -> Result<(), DocxError> {
    if actual > maximum {
        return Err(limit(resource, maximum, actual));
    }
    Ok(())
}

fn invalid_xml(part: &str, reason: impl std::fmt::Display) -> DocxError {
    DocxError::InvalidXml {
        part: part.to_owned(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PART: &str = "word/document.xml";

    fn parse(text: &str) -> Result<roxmltree::Document<'_>, DocxError> {
        parse_xml(PART, text.as_bytes())
    }

    fn assert_invalid(text: &str) {
        assert!(
            matches!(parse(text), Err(DocxError::InvalidXml { part, .. }) if part == PART),
            "expected invalid XML: {text:?}"
        );
    }

    fn assert_limit(text: &str, expected_resource: &'static str, expected_limit: usize) {
        match parse(text) {
            Err(DocxError::LimitExceeded {
                resource,
                limit,
                actual,
            }) => {
                assert_eq!(resource, expected_resource);
                assert_eq!(limit, expected_limit as u64);
                assert!(actual > limit);
            }
            result => panic!("expected {expected_resource} limit error, got {result:?}"),
        }
    }

    #[test]
    fn accepts_unicode_whitespace_and_namespace_prefix_aliases() {
        let document = parse(
            " \n<q:document xmlns:q='urn:word'><q:p>cafe\u{301} Ελληνικά 中文 👩🏽‍💻 &amp; &#x1F600;</q:p></q:document>\n ",
        )
        .expect("valid Unicode XML");
        let root = document.root_element();
        assert_eq!(root.tag_name().name(), "document");
        assert_eq!(root.tag_name().namespace(), Some("urn:word"));
        assert_eq!(
            root.first_element_child().and_then(|node| node.text()),
            Some("cafe\u{301} Ελληνικά 中文 👩🏽‍💻 & 😀")
        );
    }

    #[test]
    fn accepts_utf8_bom_and_valid_declarations() {
        for declaration in [
            "<?xml version='1.0'?>",
            "<?xml version = '1.0' encoding = 'uTf-8' ?>",
            "<?xml version='1.0' standalone='yes'?>",
            "<?xml version='1.0' encoding='UTF-8' standalone='no'?>",
        ] {
            let text = format!("\u{feff}{declaration}<r>日本語</r>");
            assert_eq!(
                parse(&text)
                    .expect("valid declaration")
                    .root_element()
                    .text(),
                Some("日本語")
            );
        }
    }

    #[test]
    fn rejects_unsupported_declared_encodings_before_invalid_body_utf8() {
        for encoding in ["UTF-16", "UTF-16LE", "ISO-8859-1", "windows-1252"] {
            let mut bytes = format!("<?xml version='1.0' encoding='{encoding}'?><r>").into_bytes();
            bytes.push(0xff);
            assert!(matches!(
                parse_xml(PART, &bytes),
                Err(DocxError::Unsupported { .. })
            ));
        }
    }

    #[test]
    fn rejects_utf16_and_utf32_byte_patterns_explicitly() {
        for bytes in [
            b"\xff\xfe<\0r\0/\0>\0".as_slice(),
            b"\xfe\xff\0<\0r\0/\0>".as_slice(),
            b"\x00\x00\xfe\xff\0\0\0<".as_slice(),
            b"\0<\0?\0x\0m\0l".as_slice(),
            b"<\0?\0x\0m\0l\0".as_slice(),
            b"\0\0\0<\0\0\0r".as_slice(),
            b"<\0\0\0r\0\0\0".as_slice(),
        ] {
            assert!(matches!(
                parse_xml(PART, bytes),
                Err(DocxError::Unsupported { .. })
            ));
        }
    }

    #[test]
    fn rejects_unsupported_versions_and_invalid_utf8() {
        for version in ["1.1", "1.2", "2.0"] {
            assert!(matches!(
                parse(&format!("<?xml version='{version}'?><r/>")),
                Err(DocxError::Unsupported { .. })
            ));
        }
        assert!(matches!(
            parse_xml(PART, b"<r>\xff</r>"),
            Err(DocxError::InvalidXml { .. })
        ));
    }

    #[test]
    fn rejects_malformed_declarations() {
        for text in [
            "<?xml?><r/>",
            "<?xml version='banana'?><r/>",
            "<?xml version='1.0' version='1.0'?><r/>",
            "<?xml encoding='UTF-8' version='1.0'?><r/>",
            "<?xml version='1.0' encoding=''?><r/>",
            "<?xml version='1.0' encoding='UTF 8'?><r/>",
            "<?xml version='1.0' standalone='YES'?><r/>",
            "<?xml version='1.0' standalone='true'?><r/>",
            "<?xml version='1.0' standalone='yes' standalone='no'?><r/>",
            "<?xml version='1.0' standalone='yes' encoding='UTF-8'?><r/>",
            "<?xml version='1.0' other='value'?><r/>",
            " <?xml version='1.0'?><r/>",
            "<r/><?xml version='1.0'?>",
        ] {
            assert_invalid(text);
        }
    }

    #[test]
    fn rejects_every_actual_doctype() {
        for text in [
            "<!DOCTYPE r><r/>",
            "<!DOCTYPE r []><r/>",
            "<!DOCTYPE r SYSTEM 'https://example.invalid/dtd'><r/>",
            "<!DOCTYPE r [<!ENTITY x 'value'>]><r>&x;</r>",
        ] {
            assert!(matches!(parse(text), Err(DocxError::Unsupported { .. })));
        }
    }

    #[test]
    fn permits_doctype_spelling_inside_comments_and_cdata() {
        let document = parse("<!-- <!DOCTYPE r> --><r><![CDATA[<!DOCTYPE r>]]></r>")
            .expect("DTD spelling is ordinary comment/CDATA content");
        assert_eq!(document.root_element().text(), Some("<!DOCTYPE r>"));
    }

    #[test]
    fn rejects_invalid_structure_entities_names_and_namespace_bindings() {
        for text in [
            "",
            "text",
            "<r>",
            "<r><a></r>",
            "<r/><s/>",
            "<r/>trailing",
            "<r>&unknown;</r>",
            "<r>&#0;</r>",
            "<r>&#xD800;</r>",
            "<r>&#x110000;</r>",
            "<r a='&#xD800;'/>",
            "<r a='&#x110000;'/>",
            "<r>&amp</r>",
            "<r a='&unknown;'/>",
            "<r a='<'/>",
            "<1invalid/>",
            "<p:r/>",
            "<r p:a='value'/>",
            "<r xmlns:xml='urn:wrong'/>",
            "<r xmlns:xmlns='urn:wrong'/>",
            "<r xmlns:p='urn:first' xmlns:p='urn:second'/>",
            "<r xmlns='urn:first' xmlns='urn:second'/>",
            "<r xmlns:xml='http://www.w3.org/XML/1998/namespace' xmlns:xml='http://www.w3.org/XML/1998/namespace'/>",
            "<r xmlns:p=''/>",
            "<r a='first' a='second'/>",
            "<r xmlns:a='urn:same' xmlns:b='urn:same' a:x='first' b:x='second'/>",
            "<r><!-- invalid -- comment --></r>",
        ] {
            assert_invalid(text);
        }
    }

    #[test]
    fn rejects_illegal_xml_characters_in_every_content_context() {
        for character in ['\0', '\u{1}', '\u{b}', '\u{fffe}', '\u{ffff}'] {
            for text in [
                format!("<r>{character}</r>"),
                format!("<r a='{character}'/>"),
                format!("<r><![CDATA[{character}]]></r>"),
                format!("<r><!-- {character} --></r>"),
                format!("<r><?p {character}?></r>"),
            ] {
                assert_invalid(&text);
            }
        }
    }

    #[test]
    fn enforces_part_byte_budget_before_parsing() {
        let bytes = vec![b' '; MAX_XML_BYTES + 1];
        assert_eq!(
            parse_xml(PART, &bytes).expect_err("oversized part"),
            limit("xml_bytes", MAX_XML_BYTES, MAX_XML_BYTES + 1)
        );
    }

    #[test]
    fn enforces_depth_budget_and_handles_empty_siblings() {
        let accepted = format!(
            "{}{}",
            "<r>".repeat(MAX_XML_DEPTH),
            "</r>".repeat(MAX_XML_DEPTH)
        );
        assert!(parse(&accepted).is_ok());
        let rejected = format!("<r>{accepted}</r>");
        assert_limit(&rejected, "xml_depth", MAX_XML_DEPTH);
        assert!(parse(&format!("<r>{}</r>", "<n/>".repeat(MAX_XML_DEPTH * 2))).is_ok());
    }

    #[test]
    fn enforces_element_attribute_budget_before_namespace_processing() {
        let attributes = (0..MAX_XML_ATTRIBUTES)
            .map(|index| format!(" a{index}='v'"))
            .collect::<String>();
        assert!(parse(&format!("<r{attributes}/>")).is_ok());
        assert_limit(
            &format!("<r{attributes} extra='v'/>"),
            "xml_attributes",
            MAX_XML_ATTRIBUTES,
        );

        // Even malformed/duplicate names cannot bypass the streaming budget.
        let repeated = " xmlns:p='urn:p'".repeat(MAX_XML_ATTRIBUTES + 1);
        assert_limit(
            &format!("<r{repeated}/>"),
            "xml_attributes",
            MAX_XML_ATTRIBUTES,
        );
    }

    #[test]
    fn enforces_aggregate_attribute_budget() {
        let attributes = (0..MAX_XML_ATTRIBUTES)
            .map(|index| format!(" a{index}='v'"))
            .collect::<String>();
        let body =
            format!("<n{attributes}/>").repeat(MAX_XML_TOTAL_ATTRIBUTES / MAX_XML_ATTRIBUTES);
        assert!(parse(&format!("<r>{body}</r>")).is_ok());
        assert_limit(
            &format!("<r>{body}<n last='v'/></r>"),
            "xml_total_attributes",
            MAX_XML_TOTAL_ATTRIBUTES,
        );
    }

    #[test]
    fn enforces_global_namespace_declaration_budget() {
        let mut body = String::new();
        for start in (0..MAX_XML_NAMESPACE_DECLARATIONS).step_by(MAX_XML_ATTRIBUTES) {
            let attributes = (start
                ..(start + MAX_XML_ATTRIBUTES).min(MAX_XML_NAMESPACE_DECLARATIONS))
                .map(|index| format!(" xmlns:p{index}='urn:{index}'"))
                .collect::<String>();
            body.push_str(&format!("<n{attributes}/>"));
        }
        assert!(parse(&format!("<r>{body}</r>")).is_ok());
        assert_limit(
            &format!("<r>{body}<n xmlns:last='urn:last'/></r>"),
            "xml_namespace_declarations",
            MAX_XML_NAMESPACE_DECLARATIONS,
        );
    }

    #[test]
    fn enforces_node_budget_including_text_and_implicit_root() {
        let body = "<n/>x".repeat((MAX_XML_NODES - 2) / 2);
        let text = format!("<r>{body}</r>");
        assert_eq!(
            parse(&text)
                .expect("exact node budget")
                .descendants()
                .count(),
            MAX_XML_NODES
        );
        assert_limit(&format!("<r>{body}<n/>x</r>"), "xml_nodes", MAX_XML_NODES);
    }

    #[test]
    fn adjacent_text_references_and_cdata_share_one_node() {
        let text = format!(
            "<r><![CDATA[a]]>{}<![CDATA[b]]></r>",
            "x&amp;".repeat(MAX_XML_NODES)
        );
        assert_eq!(
            parse(&text).expect("coalesced text").descendants().count(),
            3
        );
    }

    #[test]
    fn bounds_dom_initial_allocations_from_raw_markers() {
        let text = format!("<r><![CDATA[{}]]></r>", "<".repeat(MAX_XML_NODES));
        assert_limit(&text, "xml_node_markers", MAX_XML_NODES);
        let text = format!("<r>{}</r>", "=".repeat(MAX_XML_TOTAL_ATTRIBUTES + 1));
        assert_limit(&text, "xml_attribute_markers", MAX_XML_TOTAL_ATTRIBUTES);
    }
}
