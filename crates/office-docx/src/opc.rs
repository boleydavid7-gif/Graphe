use std::collections::{BTreeMap, BTreeSet};

use roxmltree::{Document, Node};

use crate::{
    DocxConformance, DocxError, DocxInspection, DocxPart,
    error::limit,
    package::{Package, validate_part_name},
    xml::parse_xml,
};

const CONTENT_TYPES: &str = "[Content_Types].xml";
const ROOT_RELATIONSHIPS: &str = "_rels/.rels";
const CONTENT_TYPE_NAMESPACE: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const RELATIONSHIP_NAMESPACE: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const RELATIONSHIP_CONTENT_TYPE: &str = "application/vnd.openxmlformats-package.relationships+xml";
const DOCUMENT_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const TRANSITIONAL_DOCUMENT_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
// MS-OI29500, "Activating embedded packages in Office", specifies this root
// relationship type; the officeDocument path component also occurs in Strict.
const STRICT_DOCUMENT_RELATIONSHIP: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";
const TRANSITIONAL_DOCUMENT_NAMESPACE: &str =
    "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const STRICT_DOCUMENT_NAMESPACE: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
// RFC 6838 section 4.2 bounds each restricted-name to 127 ASCII characters.
// Validate this before cloning declarations or repeating types in the inventory.
const MAX_MIME_NAME_BYTES: usize = 127;
const MAX_MIME_CONTENT_TYPE_BYTES: usize = 2 * MAX_MIME_NAME_BYTES + 1;

#[derive(Default)]
struct ContentTypes {
    defaults: BTreeMap<String, String>,
    overrides: BTreeMap<String, String>,
}

pub(crate) struct Relationship {
    pub(crate) id: String,
    pub(crate) relationship_type: String,
    pub(crate) target: String,
    pub(crate) external: bool,
}

pub(crate) fn inspect_package(package: &Package) -> Result<DocxInspection, DocxError> {
    let lookup: BTreeMap<_, _> = package
        .parts
        .keys()
        .map(|name| (name.to_ascii_lowercase(), name.as_str()))
        .collect();
    let content_types = parse_content_types(package)?;
    let mut parts = Vec::with_capacity(package.parts.len().saturating_sub(1));
    for (name, bytes) in &package.parts {
        if name == CONTENT_TYPES {
            continue;
        }
        let content_type = content_types
            .for_part(name)
            .ok_or_else(|| invalid(Some(CONTENT_TYPES), format!("no content type for {name}")))?;
        reject_macros(name, content_type)?;
        parts.push(DocxPart {
            name: name.clone(),
            content_type: content_type.to_owned(),
            uncompressed_bytes: bytes.len() as u64,
        });
    }
    for name in content_types.overrides.keys() {
        if !lookup.contains_key(name) || name == &CONTENT_TYPES.to_ascii_lowercase() {
            return Err(invalid(
                Some(CONTENT_TYPES),
                format!("content-type override does not name a package part: {name}"),
            ));
        }
    }
    if !package.parts.contains_key(ROOT_RELATIONSHIPS) {
        return Err(invalid(
            Some(ROOT_RELATIONSHIPS),
            "the package relationships item is missing",
        ));
    }

    let mut relationship_count = 0;
    let mut external_relationship_count = 0;
    let mut main_document = None;
    for part in &parts {
        let is_relationship = part.name.to_ascii_lowercase().ends_with(".rels");
        if !is_relationship
            && !part
                .content_type
                .eq_ignore_ascii_case(RELATIONSHIP_CONTENT_TYPE)
        {
            continue;
        }
        if !is_relationship
            || !part
                .content_type
                .eq_ignore_ascii_case(RELATIONSHIP_CONTENT_TYPE)
        {
            return Err(invalid(
                Some(&part.name),
                "relationship part name and content type disagree",
            ));
        }
        let source = relationship_source(&part.name)?;
        if let Some(source) = &source
            && (!lookup.contains_key(&source.to_ascii_lowercase()) || is_control_part(source))
        {
            return Err(invalid(
                Some(&part.name),
                "relationship source is missing or is a control item",
            ));
        }
        let relationships = parse_relationships(&part.name, &package.parts[&part.name])?;
        relationship_count += relationships.len();
        for relationship in relationships {
            reject_macro_relationship(&relationship.relationship_type)?;
            if relationship.external {
                external_relationship_count += 1;
            }
            let target = if relationship.external {
                None
            } else {
                let resolved = resolve_target(source.as_deref(), &relationship.target, &part.name)?;
                if is_control_part(&resolved) {
                    return Err(invalid(
                        Some(&part.name),
                        "an internal relationship targets a control item",
                    ));
                }
                Some(
                    lookup
                        .get(&resolved.to_ascii_lowercase())
                        .ok_or_else(|| {
                            invalid(
                                Some(&part.name),
                                format!("internal relationship target is missing: {resolved}"),
                            )
                        })?
                        .to_string(),
                )
            };
            let conformance = document_relationship_family(&relationship.relationship_type);
            if source.is_none()
                && let Some(conformance) = conformance
            {
                if main_document.is_some() || target.is_none() {
                    return Err(invalid(
                        Some(ROOT_RELATIONSHIPS),
                        "exactly one internal officeDocument relationship is required",
                    ));
                }
                main_document = target.map(|target| (target, conformance));
            }
        }
    }

    let (main_document_part, conformance) = main_document.ok_or_else(|| {
        invalid(
            Some(ROOT_RELATIONSHIPS),
            "exactly one internal officeDocument relationship is required",
        )
    })?;
    if !content_types
        .for_part(&main_document_part)
        .is_some_and(|value| value.eq_ignore_ascii_case(DOCUMENT_CONTENT_TYPE))
    {
        return Err(invalid(
            Some(&main_document_part),
            "officeDocument target is not a DOCX main document content type",
        ));
    }
    validate_main_document(
        &main_document_part,
        &package.parts[&main_document_part],
        conformance,
    )?;
    for part in &parts {
        if part.name == main_document_part || part.name.ends_with(".rels") {
            continue;
        }
        if is_xml_part(&part.name, &part.content_type) {
            parse_xml(&part.name, &package.parts[&part.name])?;
        }
    }

    Ok(DocxInspection {
        main_document_part,
        conformance,
        parts,
        relationship_count,
        external_relationship_count,
        total_uncompressed_bytes: package.total_uncompressed_bytes,
    })
}

impl ContentTypes {
    fn for_part(&self, name: &str) -> Option<&str> {
        let lowercase = name.to_ascii_lowercase();
        self.overrides
            .get(&lowercase)
            .or_else(|| {
                let extension = lowercase.rsplit_once('.')?.1;
                self.defaults.get(extension)
            })
            .map(String::as_str)
    }
}

fn parse_content_types(package: &Package) -> Result<ContentTypes, DocxError> {
    let bytes = package
        .parts
        .get(CONTENT_TYPES)
        .ok_or_else(|| invalid(Some(CONTENT_TYPES), "the content-types item is missing"))?;
    let document = parse_xml(CONTENT_TYPES, bytes)?;
    let root = require_root(&document, CONTENT_TYPES, CONTENT_TYPE_NAMESPACE, "Types")?;
    check_attributes(root, CONTENT_TYPES, &[])?;
    let mut content_types = ContentTypes::default();
    for node in root.children() {
        if !node.is_element() {
            check_whitespace(node, CONTENT_TYPES)?;
            continue;
        }
        if node.tag_name().namespace() != Some(CONTENT_TYPE_NAMESPACE) {
            return Err(invalid(Some(CONTENT_TYPES), "unknown content-type element"));
        }
        let (key, value, map) = match node.tag_name().name() {
            "Default" => {
                check_attributes(node, CONTENT_TYPES, &["Extension", "ContentType"])?;
                let extension = required_attribute(node, CONTENT_TYPES, "Extension")?;
                if !extension.is_ascii() {
                    return Err(unsupported("non-ASCII content-type extension"));
                }
                if !extension
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                {
                    return Err(invalid(Some(CONTENT_TYPES), "invalid default extension"));
                }
                (
                    extension.to_ascii_lowercase(),
                    required_attribute(node, CONTENT_TYPES, "ContentType")?,
                    &mut content_types.defaults,
                )
            }
            "Override" => {
                check_attributes(node, CONTENT_TYPES, &["PartName", "ContentType"])?;
                let absolute = required_attribute(node, CONTENT_TYPES, "PartName")?;
                let relative = absolute.strip_prefix('/').ok_or_else(|| {
                    invalid(Some(CONTENT_TYPES), "override PartName must be absolute")
                })?;
                validate_metadata_part_name(relative, CONTENT_TYPES)?;
                (
                    relative.to_ascii_lowercase(),
                    required_attribute(node, CONTENT_TYPES, "ContentType")?,
                    &mut content_types.overrides,
                )
            }
            _ => return Err(invalid(Some(CONTENT_TYPES), "unknown content-type element")),
        };
        validate_content_type(value)?;
        if map.insert(key, value.to_owned()).is_some() {
            return Err(invalid(
                Some(CONTENT_TYPES),
                "duplicate case-equivalent content-type declaration",
            ));
        }
        check_empty_element(node, CONTENT_TYPES)?;
    }
    Ok(content_types)
}

fn validate_content_type(value: &str) -> Result<(), DocxError> {
    if value.len() > MAX_MIME_CONTENT_TYPE_BYTES {
        return Err(limit(
            "mime_content_type_bytes",
            MAX_MIME_CONTENT_TYPE_BYTES,
            value.len(),
        ));
    }
    if !value.is_ascii() || value.contains(';') {
        return Err(unsupported("non-ASCII or parameterized MIME content type"));
    }
    let Some((kind, subtype)) = value.split_once('/') else {
        return Err(invalid(Some(CONTENT_TYPES), "invalid MIME content type"));
    };
    let name_length = kind.len().max(subtype.len());
    if name_length > MAX_MIME_NAME_BYTES {
        return Err(limit("mime_name_bytes", MAX_MIME_NAME_BYTES, name_length));
    }
    let token = |value: &str| {
        value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-'
                    )
            })
    };
    if !token(kind) || !token(subtype) {
        return Err(invalid(Some(CONTENT_TYPES), "invalid MIME content type"));
    }
    Ok(())
}

pub(crate) fn relationship_source(name: &str) -> Result<Option<String>, DocxError> {
    if name == ROOT_RELATIONSHIPS {
        return Ok(None);
    }
    let Some((directory, file)) = name.rsplit_once('/') else {
        return Err(invalid(Some(name), "invalid relationship part location"));
    };
    let Some(source_file) = file.strip_suffix(".rels") else {
        return Err(invalid(Some(name), "invalid relationship part suffix"));
    };
    let source_directory = if directory == "_rels" {
        ""
    } else {
        directory.strip_suffix("/_rels").ok_or_else(|| {
            invalid(
                Some(name),
                "relationship parts must be in a _rels directory",
            )
        })?
    };
    if source_file.is_empty() {
        return Err(invalid(Some(name), "relationship source name is empty"));
    }
    let source = if source_directory.is_empty() {
        source_file.to_owned()
    } else {
        format!("{source_directory}/{source_file}")
    };
    validate_metadata_part_name(&source, name)?;
    Ok(Some(source))
}

pub(crate) fn parse_relationships(
    part: &str,
    bytes: &[u8],
) -> Result<Vec<Relationship>, DocxError> {
    let document = parse_xml(part, bytes)?;
    let root = require_root(&document, part, RELATIONSHIP_NAMESPACE, "Relationships")?;
    check_attributes(root, part, &[])?;
    let mut ids = BTreeSet::new();
    let mut relationships = Vec::new();
    for node in root.children() {
        if !node.is_element() {
            check_whitespace(node, part)?;
            continue;
        }
        if !node.has_tag_name((RELATIONSHIP_NAMESPACE, "Relationship")) {
            return Err(invalid(Some(part), "unknown relationship element"));
        }
        check_attributes(node, part, &["Id", "Type", "Target", "TargetMode"])?;
        let id = required_attribute(node, part, "Id")?;
        validate_relationship_id(id, part)?;
        if !ids.insert(id) {
            return Err(invalid(Some(part), "duplicate relationship identifier"));
        }
        let relationship_type = required_attribute(node, part, "Type")?;
        validate_uri_reference(relationship_type, true, part)?;
        let target = required_attribute(node, part, "Target")?;
        let external = match node.attribute("TargetMode") {
            None | Some("Internal") => false,
            Some("External") => true,
            Some(_) => return Err(invalid(Some(part), "invalid relationship TargetMode")),
        };
        if external {
            validate_uri_reference(target, false, part)?;
        }
        check_empty_element(node, part)?;
        relationships.push(Relationship {
            id: id.to_owned(),
            relationship_type: relationship_type.to_owned(),
            target: target.to_owned(),
            external,
        });
    }
    Ok(relationships)
}

fn validate_relationship_id(value: &str, part: &str) -> Result<(), DocxError> {
    if !value.is_ascii() {
        return Err(unsupported("non-ASCII relationship identifier"));
    }
    let mut bytes = value.bytes();
    if !bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(invalid(
            Some(part),
            "relationship Id is not an ASCII NCName",
        ));
    }
    Ok(())
}

fn validate_uri_reference(value: &str, absolute: bool, part: &str) -> Result<(), DocxError> {
    if !value.is_ascii() {
        return Err(unsupported("non-ASCII relationship URI"));
    }
    if absolute {
        let Some((scheme, remainder)) = value.split_once(':') else {
            return Err(invalid(
                Some(part),
                "relationship Type must be an absolute URI",
            ));
        };
        let mut bytes = scheme.bytes();
        if !bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
            || !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
            || remainder.is_empty()
        {
            return Err(invalid(Some(part), "invalid relationship URI scheme"));
        }
    }
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            if !bytes.next().is_some_and(|byte| byte.is_ascii_hexdigit())
                || !bytes.next().is_some_and(|byte| byte.is_ascii_hexdigit())
            {
                return Err(invalid(
                    Some(part),
                    "invalid percent escape in relationship URI",
                ));
            }
        } else if !byte.is_ascii_alphanumeric()
            && !matches!(
                byte,
                b'-' | b'.'
                    | b'_'
                    | b'~'
                    | b':'
                    | b'/'
                    | b'?'
                    | b'#'
                    | b'['
                    | b']'
                    | b'@'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
            )
        {
            return Err(invalid(Some(part), "invalid character in relationship URI"));
        }
    }
    Ok(())
}

pub(crate) fn resolve_target(
    source: Option<&str>,
    target: &str,
    part: &str,
) -> Result<String, DocxError> {
    if !target.is_ascii() || target.starts_with("//") || target.contains(['%', '?', '#', ':', '\\'])
    {
        return Err(unsupported(
            "noncanonical, network, or escaped internal relationship target",
        ));
    }
    if target
        .rsplit('/')
        .next()
        .is_some_and(|segment| matches!(segment, "." | ".."))
    {
        return Err(invalid(
            Some(part),
            "internal target resolves to a directory",
        ));
    }
    let absolute = target.starts_with('/');
    let target = target.strip_prefix('/').unwrap_or(target);
    let mut segments: Vec<&str> = source
        .filter(|_| !absolute)
        .and_then(|source| source.rsplit_once('/').map(|(directory, _)| directory))
        .map(|directory| directory.split('/').collect())
        .unwrap_or_default();
    for segment in target.split('/') {
        match segment {
            "" => return Err(invalid(Some(part), "empty internal target path segment")),
            "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(invalid(Some(part), "internal target escapes the package"));
                }
            }
            segment => segments.push(segment),
        }
    }
    let resolved = segments.join("/");
    validate_metadata_part_name(&resolved, part)?;
    Ok(resolved)
}

fn validate_metadata_part_name(name: &str, part: &str) -> Result<(), DocxError> {
    validate_part_name(name).map_err(|error| match error {
        DocxError::Unsupported { .. } => error,
        error => invalid(Some(part), format!("invalid referenced part name: {error}")),
    })
}

fn is_control_part(name: &str) -> bool {
    name.eq_ignore_ascii_case(CONTENT_TYPES) || name.to_ascii_lowercase().ends_with(".rels")
}

fn document_relationship_family(value: &str) -> Option<DocxConformance> {
    match value {
        TRANSITIONAL_DOCUMENT_RELATIONSHIP => Some(DocxConformance::Transitional),
        STRICT_DOCUMENT_RELATIONSHIP => Some(DocxConformance::Strict),
        _ => None,
    }
}

fn validate_main_document(
    part: &str,
    bytes: &[u8],
    conformance: DocxConformance,
) -> Result<(), DocxError> {
    let document = parse_xml(part, bytes)?;
    let namespace = match conformance {
        DocxConformance::Transitional => TRANSITIONAL_DOCUMENT_NAMESPACE,
        DocxConformance::Strict => STRICT_DOCUMENT_NAMESPACE,
    };
    let root = require_root(&document, part, namespace, "document")?;
    if root
        .children()
        .filter(|child| child.has_tag_name((namespace, "body")))
        .count()
        != 1
    {
        return Err(invalid(
            Some(part),
            "main document must contain exactly one direct WordprocessingML body",
        ));
    }
    Ok(())
}

fn is_xml_part(name: &str, content_type: &str) -> bool {
    let lowercase_type = content_type.to_ascii_lowercase();
    name.to_ascii_lowercase().ends_with(".xml")
        || lowercase_type.ends_with("+xml")
        || matches!(lowercase_type.as_str(), "application/xml" | "text/xml")
}

fn reject_macros(name: &str, content_type: &str) -> Result<(), DocxError> {
    let lowercase_name = name.to_ascii_lowercase();
    let lowercase_type = content_type.to_ascii_lowercase();
    if lowercase_name.rsplit('/').next().is_some_and(|file| {
        matches!(
            file,
            "vbaproject.bin" | "vbadata.xml" | "vbaprojectsignature.bin"
        )
    }) || lowercase_type.contains("macroenabled")
        || lowercase_type.contains("vbaproject")
        || lowercase_type.contains("vbadata")
    {
        return Err(unsupported("macro-enabled documents or VBA package parts"));
    }
    Ok(())
}

fn reject_macro_relationship(value: &str) -> Result<(), DocxError> {
    let lowercase = value.to_ascii_lowercase();
    if lowercase
        .rsplit('/')
        .next()
        .is_some_and(|value| matches!(value, "vbaproject" | "vbadata" | "vbaprojectsignature"))
    {
        return Err(unsupported("VBA relationship"));
    }
    Ok(())
}

fn require_root<'a, 'input>(
    document: &'a Document<'input>,
    part: &str,
    namespace: &str,
    name: &str,
) -> Result<Node<'a, 'input>, DocxError> {
    let root = document.root_element();
    if !root.has_tag_name((namespace, name)) {
        return Err(invalid(
            Some(part),
            format!("expected {{{namespace}}}{name} root"),
        ));
    }
    Ok(root)
}

fn check_attributes(node: Node<'_, '_>, part: &str, allowed: &[&str]) -> Result<(), DocxError> {
    if node
        .attributes()
        .any(|attribute| attribute.namespace().is_some() || !allowed.contains(&attribute.name()))
    {
        return Err(invalid(
            Some(part),
            "unknown or namespaced metadata attribute",
        ));
    }
    Ok(())
}

fn required_attribute<'a>(
    node: Node<'a, '_>,
    part: &str,
    name: &str,
) -> Result<&'a str, DocxError> {
    node.attribute(name)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(Some(part), format!("missing or empty {name} attribute")))
}

fn check_empty_element(node: Node<'_, '_>, part: &str) -> Result<(), DocxError> {
    for child in node.children() {
        if child.is_element() {
            return Err(invalid(
                Some(part),
                "metadata entry must not contain child elements",
            ));
        }
        check_whitespace(child, part)?;
    }
    Ok(())
}

fn check_whitespace(node: Node<'_, '_>, part: &str) -> Result<(), DocxError> {
    if node.is_text()
        && node.text().is_some_and(|text| {
            !text
                .bytes()
                .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        })
    {
        return Err(invalid(Some(part), "unexpected text in package metadata"));
    }
    Ok(())
}

fn invalid(part: Option<&str>, reason: impl Into<String>) -> DocxError {
    DocxError::InvalidPackage {
        part: part.map(str::to_owned),
        reason: reason.into(),
    }
}

fn unsupported(feature: &str) -> DocxError {
    DocxError::Unsupported {
        feature: feature.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(main_name: &str, strict: bool) -> Package {
        let relationship_type = if strict {
            STRICT_DOCUMENT_RELATIONSHIP
        } else {
            TRANSITIONAL_DOCUMENT_RELATIONSHIP
        };
        let namespace = if strict {
            STRICT_DOCUMENT_NAMESPACE
        } else {
            TRANSITIONAL_DOCUMENT_NAMESPACE
        };
        let mut parts = BTreeMap::new();
        parts.insert(
            CONTENT_TYPES.to_owned(),
            format!(
                "<Types xmlns=\"{CONTENT_TYPE_NAMESPACE}\"><Default Extension=\"rels\" ContentType=\"{RELATIONSHIP_CONTENT_TYPE}\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/{main_name}\" ContentType=\"{DOCUMENT_CONTENT_TYPE}\"/></Types>"
            )
            .into_bytes(),
        );
        parts.insert(
            ROOT_RELATIONSHIPS.to_owned(),
            format!(
                "<Relationships xmlns=\"{RELATIONSHIP_NAMESPACE}\"><Relationship Id=\"main\" Type=\"{relationship_type}\" Target=\"{main_name}\"/></Relationships>"
            )
            .into_bytes(),
        );
        parts.insert(
            main_name.to_owned(),
            format!("<q:document xmlns:q=\"{namespace}\"><q:body/></q:document>").into_bytes(),
        );
        Package {
            total_uncompressed_bytes: parts.values().map(|bytes| bytes.len() as u64).sum(),
            parts,
        }
    }

    fn replace(package: &mut Package, part: &str, from: &str, to: &str) {
        let value = String::from_utf8(package.parts[part].clone()).expect("fixture UTF-8");
        assert!(value.contains(from), "fixture replacement must match");
        package
            .parts
            .insert(part.to_owned(), value.replace(from, to).into_bytes());
    }

    fn relationships(entries: &str) -> Vec<u8> {
        format!("<Relationships xmlns=\"{RELATIONSHIP_NAMESPACE}\">{entries}</Relationships>")
            .into_bytes()
    }

    #[test]
    fn discovers_relocated_main_part_with_alternate_prefix_and_sorted_inventory() {
        let package = fixture("content/main.xml", false);
        let result = inspect_package(&package).expect("valid relocated main document");
        assert_eq!(result.main_document_part, "content/main.xml");
        assert_eq!(result.conformance, DocxConformance::Transitional);
        assert_eq!(result.relationship_count, 1);
        assert_eq!(result.external_relationship_count, 0);
        assert_eq!(
            result.total_uncompressed_bytes,
            package.total_uncompressed_bytes
        );
        assert_eq!(result.parts.len(), 2);
        assert_eq!(result.parts[0].name, ROOT_RELATIONSHIPS);
        assert_eq!(result.parts[1].name, "content/main.xml");
        assert_eq!(
            result.parts[1].uncompressed_bytes,
            package.parts["content/main.xml"].len() as u64
        );
    }

    #[test]
    fn strict_namespace_family_must_match_relationship_family() {
        let mut package = fixture("document.xml", true);
        assert_eq!(
            inspect_package(&package)
                .expect("Strict package")
                .conformance,
            DocxConformance::Strict
        );
        replace(
            &mut package,
            "document.xml",
            STRICT_DOCUMENT_NAMESPACE,
            TRANSITIONAL_DOCUMENT_NAMESPACE,
        );
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::InvalidPackage { .. })
        ));
    }

    #[test]
    fn inventory_does_not_interpret_or_discard_unknown_document_features() {
        let mut package = fixture("document.xml", false);
        replace(
            &mut package,
            "document.xml",
            "<q:body/>",
            "<q:body><q:tbl/><unknown xmlns=\"urn:future\">é 👩🏽‍🚀 e&#x301;</unknown></q:body>",
        );
        let original = package.parts["document.xml"].clone();
        inspect_package(&package).expect("read-only inventory permits unknown semantics");
        assert_eq!(package.parts["document.xml"], original);
    }

    #[test]
    fn main_document_requires_exactly_one_direct_body() {
        for replacement in ["", "<q:body/><q:body/>", "<q:other><q:body/></q:other>"] {
            let mut package = fixture("document.xml", false);
            replace(&mut package, "document.xml", "<q:body/>", replacement);
            assert!(matches!(
                inspect_package(&package),
                Err(DocxError::InvalidPackage { .. })
            ));
        }
    }

    #[test]
    fn requires_both_control_items_and_a_main_relationship() {
        for absent in [CONTENT_TYPES, ROOT_RELATIONSHIPS] {
            let mut package = fixture("document.xml", false);
            package.parts.remove(absent);
            assert!(matches!(
                inspect_package(&package),
                Err(DocxError::InvalidPackage { .. })
            ));
        }
        let mut package = fixture("document.xml", false);
        package
            .parts
            .insert(ROOT_RELATIONSHIPS.to_owned(), relationships(""));
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::InvalidPackage { .. })
        ));
    }

    #[test]
    fn every_part_requires_a_type_and_overrides_require_an_existing_part() {
        let mut package = fixture("document.xml", false);
        package.parts.insert("blob.bin".to_owned(), vec![0]);
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::InvalidPackage { .. })
        ));
        package.parts.remove("blob.bin");
        replace(&mut package, CONTENT_TYPES, "/document.xml", "/absent.xml");
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::InvalidPackage { .. })
        ));
    }

    #[test]
    fn rejects_case_equivalent_defaults_and_overrides() {
        for duplicate in [
            "<Default Extension=\"XML\" ContentType=\"application/xml\"/>",
            "<Override PartName=\"/DOCUMENT.XML\" ContentType=\"application/xml\"/>",
        ] {
            let mut package = fixture("document.xml", false);
            replace(
                &mut package,
                CONTENT_TYPES,
                "</Types>",
                &format!("{duplicate}</Types>"),
            );
            assert!(matches!(
                inspect_package(&package),
                Err(DocxError::InvalidPackage { .. })
            ));
        }
    }

    #[test]
    fn oversized_default_mime_type_is_rejected_before_inventory_amplification() {
        let mut package = fixture("document.xml", false);
        let oversized = format!("application/{}", "a".repeat(1024 * 1024));
        replace(
            &mut package,
            CONTENT_TYPES,
            "</Types>",
            &format!("<Default Extension=\"bin\" ContentType=\"{oversized}\"/></Types>"),
        );
        for index in 0..252 {
            package.parts.insert(format!("part{index}.bin"), Vec::new());
        }
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::LimitExceeded {
                resource: "mime_content_type_bytes",
                limit: 255,
                actual,
            }) if actual == oversized.len() as u64
        ));
    }

    #[test]
    fn maximum_length_mime_names_are_accepted_and_component_overflow_is_rejected() {
        let mut package = fixture("document.xml", false);
        let boundary = format!("{}/{}", "a".repeat(127), "b".repeat(127));
        replace(
            &mut package,
            CONTENT_TYPES,
            "</Types>",
            &format!("<Default Extension=\"bin\" ContentType=\"{boundary}\"/></Types>"),
        );
        package.parts.insert("part.bin".to_owned(), Vec::new());
        let inventory = inspect_package(&package).expect("127-byte MIME name boundary");
        assert_eq!(
            inventory
                .parts
                .iter()
                .find(|part| part.name == "part.bin")
                .expect("binary part")
                .content_type,
            boundary
        );
        for value in [
            format!("{}/b", "a".repeat(128)),
            format!("a/{}", "b".repeat(128)),
        ] {
            assert!(matches!(
                validate_content_type(&value),
                Err(DocxError::LimitExceeded {
                    resource: "mime_name_bytes",
                    limit: 127,
                    actual: 128,
                })
            ));
        }
        for value in ["!application/test", "application/.test"] {
            assert!(matches!(
                validate_content_type(value),
                Err(DocxError::InvalidPackage { .. })
            ));
        }
    }

    #[test]
    fn rejects_unknown_or_namespaced_metadata_attributes_and_content() {
        for extra in [
            " Mystery=\"value\"",
            " xmlns:a=\"urn:future\" a:Target=\"other.xml\"",
        ] {
            let mut package = fixture("document.xml", false);
            replace(
                &mut package,
                ROOT_RELATIONSHIPS,
                " Id=\"main\"",
                &format!("{extra} Id=\"main\""),
            );
            assert!(matches!(
                inspect_package(&package),
                Err(DocxError::InvalidPackage { .. })
            ));
        }
        let mut package = fixture("document.xml", false);
        replace(
            &mut package,
            ROOT_RELATIONSHIPS,
            "</Relationships>",
            "unexpected</Relationships>",
        );
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::InvalidPackage { .. })
        ));
    }

    #[test]
    fn validates_all_internal_relationships_and_resolves_dot_segments() {
        let mut package = fixture("content/main.xml", false);
        package
            .parts
            .insert("content/styles.xml".to_owned(), b"<styles/>".to_vec());
        package.parts.insert(
            "content/_rels/main.xml.rels".to_owned(),
            relationships("<Relationship Id=\"styles\" Type=\"urn:styles\" Target=\"../content/./STYLES.xml\"/>"),
        );
        assert_eq!(
            inspect_package(&package)
                .expect("relative target")
                .relationship_count,
            2
        );
        replace(
            &mut package,
            "content/_rels/main.xml.rels",
            "STYLES.xml",
            "absent.xml",
        );
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::InvalidPackage { .. })
        ));
    }

    #[test]
    fn resolves_package_absolute_targets_from_root_and_part_relationships() {
        let mut package = fixture("documents/text/main.xml", true);
        // Keep the independently documented Strict type literal in this fixture
        // rather than deriving the expected URI from the implementation constant.
        package.parts.insert(
            ROOT_RELATIONSHIPS.to_owned(),
            relationships("<Relationship Id=\"main\" Type=\"http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument\" Target=\"/documents/text/main.xml\"/>"),
        );
        package
            .parts
            .insert("styles.xml".to_owned(), b"<styles/>".to_vec());
        package.parts.insert(
            "documents/text/_rels/main.xml.rels".to_owned(),
            relationships(
                "<Relationship Id=\"styles\" Type=\"urn:styles\" Target=\"/styles.xml\"/>",
            ),
        );
        let result = inspect_package(&package).expect("package-absolute targets");
        assert_eq!(result.main_document_part, "documents/text/main.xml");
        assert_eq!(result.relationship_count, 2);
    }

    #[test]
    fn rejects_relationship_part_with_missing_source_or_invalid_location() {
        for name in [
            "content/_rels/absent.xml.rels",
            "loose.rels",
            "content/wrong/main.xml.rels",
        ] {
            let mut package = fixture("content/main.xml", false);
            package.parts.insert(name.to_owned(), relationships(""));
            assert!(matches!(
                inspect_package(&package),
                Err(DocxError::InvalidPackage { .. })
            ));
        }
    }

    #[test]
    fn inventories_external_targets_without_fetching_or_requiring_a_package_part() {
        let mut package = fixture("document.xml", false);
        replace(
            &mut package,
            ROOT_RELATIONSHIPS,
            "</Relationships>",
            "<Relationship Id=\"external\" Type=\"urn:link\" Target=\"https://example.invalid/missing%20file?query#fragment\" TargetMode=\"External\"/></Relationships>",
        );
        let result = inspect_package(&package).expect("external target is only inventoried");
        assert_eq!(result.relationship_count, 2);
        assert_eq!(result.external_relationship_count, 1);
    }

    #[test]
    fn rejects_external_or_duplicate_main_relationships() {
        let mut package = fixture("document.xml", false);
        replace(
            &mut package,
            ROOT_RELATIONSHIPS,
            " Id=\"main\"",
            " TargetMode=\"External\" Id=\"main\"",
        );
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::InvalidPackage { .. })
        ));
        let mut package = fixture("document.xml", false);
        replace(
            &mut package,
            ROOT_RELATIONSHIPS,
            "</Relationships>",
            &format!(
                "<Relationship Id=\"other\" Type=\"{STRICT_DOCUMENT_RELATIONSHIP}\" Target=\"document.xml\"/></Relationships>"
            ),
        );
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::InvalidPackage { .. })
        ));
    }

    #[test]
    fn rejects_invalid_relationship_ids_modes_and_types() {
        for (from, to) in [
            ("Id=\"main\"", "Id=\"1main\""),
            ("Id=\"main\"", "Id=\"with:colon\""),
            ("Id=\"main\"", "Id=\"main\" TargetMode=\"external\""),
            (TRANSITIONAL_DOCUMENT_RELATIONSHIP, "relative/type"),
            (
                TRANSITIONAL_DOCUMENT_RELATIONSHIP,
                "https://example.invalid/%GG",
            ),
            (
                TRANSITIONAL_DOCUMENT_RELATIONSHIP,
                "https://example.invalid/with space",
            ),
        ] {
            let mut package = fixture("document.xml", false);
            replace(&mut package, ROOT_RELATIONSHIPS, from, to);
            assert!(matches!(
                inspect_package(&package),
                Err(DocxError::InvalidPackage { .. })
            ));
        }
        let mut package = fixture("document.xml", false);
        replace(
            &mut package,
            ROOT_RELATIONSHIPS,
            "Id=\"main\"",
            "Id=\"idé\"",
        );
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::Unsupported { .. })
        ));
    }

    #[test]
    fn rejects_duplicate_relationship_identifiers() {
        let mut package = fixture("document.xml", false);
        replace(
            &mut package,
            ROOT_RELATIONSHIPS,
            "</Relationships>",
            "<Relationship Id=\"main\" Type=\"urn:another\" Target=\"document.xml\"/></Relationships>",
        );
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::InvalidPackage { .. })
        ));
    }

    #[test]
    fn rejects_escaping_directory_and_control_targets() {
        for target in [
            "../document.xml",
            "/../document.xml",
            "word/..",
            "./",
            "_rels/.rels",
            "/_rels/.rels",
            CONTENT_TYPES,
        ] {
            let mut package = fixture("document.xml", false);
            replace(
                &mut package,
                ROOT_RELATIONSHIPS,
                "Target=\"document.xml\"",
                &format!("Target=\"{target}\""),
            );
            assert!(matches!(
                inspect_package(&package),
                Err(DocxError::InvalidPackage { .. })
            ));
        }
    }

    #[test]
    fn unsupported_internal_uri_spellings_are_explicit() {
        for target in [
            "//example.invalid/document.xml",
            "document%2exml",
            "document.xml?x",
            "document.xml#part",
            "file:document.xml",
            "dócument.xml",
        ] {
            let mut package = fixture("document.xml", false);
            replace(
                &mut package,
                ROOT_RELATIONSHIPS,
                "Target=\"document.xml\"",
                &format!("Target=\"{target}\""),
            );
            assert!(matches!(
                inspect_package(&package),
                Err(DocxError::Unsupported { .. })
            ));
        }
    }

    #[test]
    fn validates_xml_in_other_parts_even_when_the_feature_is_unknown() {
        let mut package = fixture("document.xml", false);
        package
            .parts
            .insert("unknown.xml".to_owned(), b"<broken>".to_vec());
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::InvalidXml { .. })
        ));
        package.parts.remove("unknown.xml");
        package
            .parts
            .insert("unknown.dat".to_owned(), b"<broken>".to_vec());
        replace(
            &mut package,
            CONTENT_TYPES,
            "</Types>",
            "<Default Extension=\"dat\" ContentType=\"application/example+xml\"/></Types>",
        );
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::InvalidXml { .. })
        ));
    }

    #[test]
    fn rejects_relationship_content_type_mismatches() {
        let mut package = fixture("document.xml", false);
        replace(
            &mut package,
            CONTENT_TYPES,
            RELATIONSHIP_CONTENT_TYPE,
            "application/xml",
        );
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::InvalidPackage { .. })
        ));
    }

    #[test]
    fn macro_main_types_known_parts_and_relationships_are_unsupported() {
        let mut package = fixture("document.xml", false);
        replace(
            &mut package,
            CONTENT_TYPES,
            DOCUMENT_CONTENT_TYPE,
            "application/vnd.ms-word.document.macroEnabled.main+xml",
        );
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::Unsupported { .. })
        ));
        let mut package = fixture("document.xml", false);
        package
            .parts
            .insert("word/vbaData.xml".to_owned(), b"<vba/>".to_vec());
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::Unsupported { .. })
        ));
        let mut package = fixture("document.xml", false);
        replace(
            &mut package,
            ROOT_RELATIONSHIPS,
            TRANSITIONAL_DOCUMENT_RELATIONSHIP,
            "http://schemas.microsoft.com/office/2006/relationships/vbaProject",
        );
        assert!(matches!(
            inspect_package(&package),
            Err(DocxError::Unsupported { .. })
        ));
    }
}
