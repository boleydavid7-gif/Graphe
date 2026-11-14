//! Strict RTF projection over the borrowed, byte-oriented rclip tokenizer.

use std::collections::BTreeMap;

use office_core::{
    Alignment, Block, CharacterFormat, Document, DocumentMetadata, HalfPoints, Id, Indentation,
    LineSpacing, NamedStyle, PageMargins, PageOrientation, PageSize, Paragraph, ParagraphFormat,
    Section, SectionProperties, TextColor, TextRun, Twips, Underline,
};
use rclip_rtf::{Codepage, ControlSymbol, Token, Tokenizer};

use crate::{DocumentFormat, FormatDiagnostic, FormatError, ImportOutcome};

mod export;
pub(crate) use export::export_document;

const MAX_TOKENS: usize = 200_000;
const MAX_DEPTH: usize = 64;
const MAX_PARAGRAPHS: usize = 20_000;
const MAX_TABLE_ENTRIES: usize = 4096;

#[derive(Clone, Copy)]
struct Item<'a> {
    token: Token<'a>,
    close: usize,
    offset: usize,
}

fn invalid(reason: impl Into<String>) -> FormatError {
    FormatError::new("invalid_rtf", reason)
}

fn limit() -> FormatError {
    FormatError::new(
        "rtf_limit",
        "RTF exceeds the supported structural resource limit.",
    )
}

fn lex(bytes: &[u8]) -> Result<Vec<Item<'_>>, FormatError> {
    if !bytes.starts_with(b"{\\rtf1") {
        return Err(invalid("RTF must begin with a version 1 document header."));
    }
    let mut lexer = Tokenizer::new(bytes);
    let mut items: Vec<Item<'_>> = Vec::new();
    let mut groups = Vec::new();
    let mut previous = 0;
    let mut closed = false;
    while let Some(token) = lexer.next() {
        let token = token.map_err(|error| invalid(format!("Malformed RTF token: {error}")))?;
        let start = lexer.token_offset();
        let end = lexer.pos();
        // The clipboard lexer ignores NULs. Native-file import cannot silently
        // do so; NUL in an explicitly bounded binary payload is separate data.
        if bytes[previous..start].contains(&0) {
            return Err(invalid(
                "NUL outside an RTF binary payload is not supported.",
            ));
        }
        previous = end;
        if closed {
            if matches!(token, Token::Text(text) if text.bytes().all(|b| matches!(b, b' ' | b'\t')))
            {
                continue;
            }
            return Err(invalid("Unexpected data after the RTF document group."));
        }
        if items.len() >= MAX_TOKENS {
            return Err(limit());
        }
        if let Token::ControlWord { name, param } = token {
            let suffix = &bytes[start + 1 + name.len()..end];
            let raw = suffix.strip_suffix(b" ").unwrap_or(suffix);
            // The tokenizer intentionally saturates large numbers. The Writer
            // adapter must reject rather than reinterpret those property values.
            if !raw.is_empty() {
                let parsed = std::str::from_utf8(raw)
                    .ok()
                    .and_then(|text| text.parse::<i32>().ok());
                if parsed != param {
                    return Err(invalid("RTF numeric parameter is out of range."));
                }
            }
            if name == "bin" && !param.is_some_and(|n| n >= 0) {
                return Err(invalid("RTF binary payload requires a nonnegative length."));
            }
        }
        let index = items.len();
        items.push(Item {
            token,
            close: index,
            offset: start,
        });
        match token {
            Token::GroupStart => {
                if groups.len() >= MAX_DEPTH {
                    return Err(limit());
                }
                groups.push(index);
            }
            Token::GroupEnd => {
                let open = groups
                    .pop()
                    .ok_or_else(|| invalid("Unmatched RTF closing group."))?;
                items[open].close = index;
                closed = groups.is_empty();
            }
            _ if groups.is_empty() => {
                return Err(invalid("RTF content is outside its document group."));
            }
            _ => {}
        }
    }
    if bytes[previous..].contains(&0) {
        return Err(invalid(
            "NUL outside an RTF binary payload is not supported.",
        ));
    }
    if !closed || !groups.is_empty() || items.len() < 3 {
        return Err(invalid("RTF document group is incomplete."));
    }
    if !matches!(
        items[1].token,
        Token::ControlWord {
            name: "rtf",
            param: Some(1)
        }
    ) {
        return Err(invalid("Unsupported RTF version."));
    }
    Ok(items)
}

#[derive(Clone)]
struct State {
    character: CharacterFormat,
    paragraph: ParagraphFormat,
    style: Option<i32>,
    uc: usize,
    codepage: Codepage,
    font: Option<i32>,
    sl: Option<i32>,
    sl_multiple: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            character: CharacterFormat::default(),
            paragraph: ParagraphFormat::default(),
            style: None,
            uc: 1,
            codepage: Codepage::Windows1252,
            font: None,
            sl: None,
            sl_multiple: false,
        }
    }
}

#[derive(Clone)]
struct Font {
    name: String,
    charset: i32,
}

#[derive(Clone, Default)]
struct Geometry {
    width: Option<i32>,
    height: Option<i32>,
    top: Option<i32>,
    right: Option<i32>,
    bottom: Option<i32>,
    left: Option<i32>,
    header: Option<i32>,
    footer: Option<i32>,
    gutter: Option<i32>,
    orientation: Option<PageOrientation>,
}

impl Geometry {
    fn properties(&self) -> Option<SectionProperties> {
        let size = self.width.is_some() || self.height.is_some() || self.orientation.is_some();
        let margins = [
            self.top,
            self.right,
            self.bottom,
            self.left,
            self.header,
            self.footer,
            self.gutter,
        ]
        .iter()
        .any(Option::is_some);
        (size || margins).then(|| SectionProperties {
            page_size: size.then(|| PageSize {
                width: Twips(self.width.unwrap_or(12_240)),
                height: Twips(self.height.unwrap_or(15_840)),
                orientation: self.orientation,
            }),
            page_margins: margins.then(|| PageMargins {
                top: Twips(self.top.unwrap_or(1440)),
                right: Twips(self.right.unwrap_or(1440)),
                bottom: Twips(self.bottom.unwrap_or(1440)),
                left: Twips(self.left.unwrap_or(1440)),
                header: Twips(self.header.unwrap_or(720)),
                footer: Twips(self.footer.unwrap_or(720)),
                gutter: Twips(self.gutter.unwrap_or(0)),
            }),
        })
    }
}

struct Parser<'a> {
    items: Vec<Item<'a>>,
    id: Id,
    fonts: BTreeMap<i32, Font>,
    colors: Vec<Option<TextColor>>,
    styles: BTreeMap<i32, NamedStyle>,
    style_defaults: BTreeMap<i32, (bool, bool, bool)>,
    default_font: Option<i32>,
    title: String,
    title_seen: bool,
    runs: Vec<TextRun>,
    blocks: Vec<Block>,
    sections: Vec<Section>,
    geometry: Geometry,
    defaults: Geometry,
    paragraph_number: usize,
    paragraph_pending: bool,
    report: Vec<FormatDiagnostic>,
    skip: usize,
    pending_high: Option<(u16, CharacterFormat)>,
}

impl<'a> Parser<'a> {
    fn new(items: Vec<Item<'a>>, id: Id) -> Self {
        Self {
            items,
            id,
            fonts: BTreeMap::new(),
            colors: Vec::new(),
            styles: BTreeMap::new(),
            style_defaults: BTreeMap::new(),
            default_font: None,
            title: String::new(),
            title_seen: false,
            runs: Vec::new(),
            blocks: Vec::new(),
            sections: Vec::new(),
            geometry: Geometry::default(),
            defaults: Geometry::default(),
            paragraph_number: 0,
            paragraph_pending: false,
            report: Vec::new(),
            skip: 0,
            pending_high: None,
        }
    }

    fn unsupported(&mut self, at: usize, feature: impl Into<String>) {
        let mut feature = feature.into();
        if feature.len() > 1024 {
            let mut end = 1021;
            while !feature.is_char_boundary(end) {
                end -= 1;
            }
            feature.truncate(end);
            feature.push('…');
        }
        if self.report.len() < 128 {
            let offset = self.items.get(at).map_or(0, |item| item.offset);
            self.report.push(FormatDiagnostic::new(
                "unsupported_rtf",
                "rtf",
                format!("byte:{offset}"),
                feature,
            ));
        }
    }

    fn id_for(&self, kind: &str, number: impl std::fmt::Display) -> Result<Id, FormatError> {
        Ok(Id::new(format!(
            "{}:rtf:{kind}:{number}",
            self.id.as_str()
        ))?)
    }

    fn destination(&self, open: usize) -> (Option<&'a str>, usize, bool) {
        let mut at = open + 1;
        let ignored = matches!(
            self.items.get(at).map(|i| i.token),
            Some(Token::ControlSymbol(ControlSymbol::Ignorable))
        );
        if ignored {
            at += 1;
        }
        match self.items.get(at).map(|item| item.token) {
            Some(Token::ControlWord { name, .. }) => (Some(name), at, ignored),
            _ => (None, at, ignored),
        }
    }

    fn body(&mut self, start: usize, end: usize, state: &mut State) -> Result<(), FormatError> {
        let mut at = start;
        while at < end {
            let token = self.items[at].token;
            if matches!(
                token,
                Token::Text(_)
                    | Token::RawByte(_)
                    | Token::ControlSymbol(_)
                    | Token::ControlWord { name: "u", .. }
            ) {
                let selected = state.font.or_else(|| {
                    (!self.style_flags(state.style).0).then_some(self.default_font.unwrap_or(0))
                });
                if selected
                    .and_then(|number| self.fonts.get(&number))
                    .is_some_and(|font| !matches!(font.charset, 0 | 1))
                {
                    self.unsupported(
                        at,
                        "The selected/default RTF font character set is not supported.",
                    );
                }
            }
            match token {
                Token::GroupStart => {
                    // A structural scope boundary terminates ANSI fallback,
                    // rather than allowing it to consume a later group's text.
                    self.skip = 0;
                    let close = self.items[at].close;
                    let (destination, first, ignorable) = self.destination(at);
                    match destination {
                        Some("fonttbl") => self.font_table(first + 1, close, state)?,
                        Some("colortbl") => self.color_table(first + 1, close)?,
                        Some("stylesheet") => self.style_table(first + 1, close, state)?,
                        Some("info") => self.information(first + 1, close, state)?,
                        Some("generator") => {
                            for current in first + 1..close {
                                if !matches!(
                                    self.items[current].token,
                                    Token::Text(_)
                                        | Token::RawByte(_)
                                        | Token::ControlSymbol(
                                            ControlSymbol::Literal(_) | ControlSymbol::HexByte(_)
                                        )
                                        | Token::ControlWord {
                                            name: "u" | "uc",
                                            ..
                                        }
                                ) {
                                    self.unsupported(current,"Unsupported nested generator metadata is retained in the original.");
                                }
                            }
                        }
                        Some(name) if ignorable || is_destination(name) => self.unsupported(
                            at,
                            format!("Unsupported RTF destination \\{name}; original retained."),
                        ),
                        _ => {
                            let mut nested = state.clone();
                            self.body(at + 1, close, &mut nested)?;
                        }
                    }
                    at = close + 1;
                    continue;
                }
                Token::GroupEnd => return Err(invalid("Unexpected closing RTF group.")),
                Token::Text(text) => {
                    let skipped = self.skip.min(text.len());
                    self.skip -= skipped;
                    let text = &text[skipped..];
                    if text
                        .bytes()
                        .any(|byte| byte < 32 && byte != b'\t' || byte == 127)
                    {
                        return Err(invalid("Unsupported control byte in RTF text."));
                    }
                    if !text.is_empty() {
                        if self.pending_high.is_some() {
                            return Err(invalid("Unpaired RTF UTF-16 high surrogate."));
                        }
                        self.append_text(text, self.current_format(state))?;
                    }
                }
                Token::RawByte(byte) | Token::ControlSymbol(ControlSymbol::HexByte(byte)) => {
                    if self.skip > 0 {
                        self.skip -= 1;
                    } else if let Some(scalar) = state.codepage.decode(byte) {
                        self.scalar(scalar, state)?;
                    } else {
                        self.unsupported(
                            at,
                            "RTF byte cannot be decoded losslessly in this code page.",
                        );
                    }
                }
                Token::ControlSymbol(symbol) => {
                    if self.skip > 0 {
                        self.skip -= 1;
                    } else {
                        match symbol {
                            ControlSymbol::Literal(scalar) => self.scalar(scalar, state)?,
                            ControlSymbol::NonBreakingSpace => self.scalar('\u{a0}', state)?,
                            ControlSymbol::NonBreakingHyphen => self.scalar('\u{2011}', state)?,
                            ControlSymbol::OptionalHyphen => self.scalar('\u{ad}', state)?,
                            ControlSymbol::EmbeddedParagraph => {
                                self.finish_paragraph(state, true)?
                            }
                            _ => self.unsupported(at, "Unsupported RTF control symbol."),
                        }
                    }
                }
                Token::Binary(_) => {
                    self.unsupported(at, "RTF binary content is preserved but not editable.")
                }
                Token::ControlWord { name, param } => {
                    if self.skip > 0 {
                        self.skip -= 1;
                        // The binary payload belongs to its control token and
                        // therefore consumes no additional fallback character.
                        if name == "bin"
                            && matches!(
                                self.items.get(at + 1).map(|i| i.token),
                                Some(Token::Binary(_))
                            )
                        {
                            at += 1;
                        }
                    } else if name == "u" {
                        self.unicode(required(param, name)?, state)?;
                        self.skip = state.uc;
                    } else if name == "uc" {
                        let count = required(param, name)?;
                        if !(0..=32).contains(&count) {
                            return Err(invalid(
                                "RTF Unicode fallback count must be between 0 and 32.",
                            ));
                        }
                        state.uc = count as usize;
                    } else if !self.header(name, param, state, at)?
                        && !self.geometry_property(name, param, at)?
                        && !self.property(name, param, state, at)?
                    {
                        match name {
                            "par" => self.finish_paragraph(state, true)?,
                            "sect" => {
                                if self.paragraph_pending || !self.runs.is_empty() {
                                    self.finish_paragraph(state, false)?;
                                }
                                self.finish_section()?;
                            }
                            "line" => self.scalar('\n', state)?,
                            "tab" => self.scalar('\t', state)?,
                            "page" => {
                                if self.runs.iter().any(|run| !run.text.is_empty()) {
                                    self.unsupported(at, "A page break inside a paragraph is not representable by the current model.");
                                } else {
                                    state.paragraph.page_break_before = Some(true);
                                    self.paragraph_pending = true;
                                }
                            }
                            "emdash" => self.scalar('\u{2014}', state)?,
                            "endash" => self.scalar('\u{2013}', state)?,
                            "lquote" => self.scalar('\u{2018}', state)?,
                            "rquote" => self.scalar('\u{2019}', state)?,
                            "ldblquote" => self.scalar('\u{201c}', state)?,
                            "rdblquote" => self.scalar('\u{201d}', state)?,
                            "bullet" => self.scalar('\u{2022}', state)?,
                            "enspace" => self.scalar('\u{2002}', state)?,
                            "emspace" => self.scalar('\u{2003}', state)?,
                            "qmspace" => self.scalar('\u{2005}', state)?,
                            _ => self.unsupported(
                                at,
                                format!("Unsupported RTF property \\{name}; original retained."),
                            ),
                        }
                    }
                }
            }
            at += 1;
        }
        self.skip = 0;
        Ok(())
    }

    fn header(
        &mut self,
        name: &str,
        param: Option<i32>,
        state: &mut State,
        at: usize,
    ) -> Result<bool, FormatError> {
        match name {
            "rtf" if param == Some(1) => {}
            "ansi" => state.codepage = Codepage::Windows1252,
            "ansicpg" => {
                let value = u16::try_from(required(param, name)?)
                    .map_err(|_| invalid("RTF code page is out of range."))?;
                state.codepage = Codepage::from_ansicpg(value);
                if !state.codepage.is_supported() {
                    self.unsupported(at, format!("RTF code page {value} is not supported."));
                }
            }
            "deff" => self.default_font = Some(nonnegative(param, name)?),
            // Producer/display hints do not alter the supported semantic model.
            "viewkind" | "viewscale" | "viewzk" | "fet" | "fromtext" | "nouicompat" => {}
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn property(
        &mut self,
        name: &str,
        param: Option<i32>,
        state: &mut State,
        at: usize,
    ) -> Result<bool, FormatError> {
        match name {
            "b" => state.character.bold = Some(param != Some(0)),
            "i" => state.character.italic = Some(param != Some(0)),
            "ul" => {
                state.character.underline = Some(if param == Some(0) {
                    Underline::None
                } else {
                    Underline::Single
                })
            }
            "ulnone" => state.character.underline = Some(Underline::None),
            "uldb" => {
                state.character.underline = Some(if param == Some(0) {
                    Underline::None
                } else {
                    Underline::Double
                })
            }
            "fs" => {
                let size = positive(param, name)?;
                state.character.font_size = Some(HalfPoints(size as u32));
            }
            "f" => {
                let number = nonnegative(param, name)?;
                let font = self
                    .fonts
                    .get(&number)
                    .ok_or_else(|| invalid("RTF text references an undefined font."))?;
                state.character.font_family = Some(font.name.clone());
                state.font = Some(number);
                if !matches!(font.charset, 0 | 1) {
                    self.unsupported(
                        at,
                        "The selected RTF font uses an unsupported character set.",
                    );
                }
            }
            "cf" => {
                let number = nonnegative(param, name)? as usize;
                let color = self
                    .colors
                    .get(number)
                    .copied()
                    .or_else(|| (number == 0).then_some(None))
                    .ok_or_else(|| invalid("RTF text references an undefined color."))?;
                state.character.color = Some(color.unwrap_or(TextColor::Auto {}));
            }
            "plain" => {
                state.character = CharacterFormat {
                    bold: Some(false),
                    italic: Some(false),
                    underline: Some(Underline::None),
                    font_family: self.default_family(),
                    font_size: Some(HalfPoints(24)),
                    color: Some(TextColor::Auto {}),
                };
                state.font = self.default_font;
            }
            "pard" => {
                state.paragraph = ParagraphFormat::default();
                state.style = None;
                state.sl = None;
                state.sl_multiple = false;
            }
            "s" => {
                let number = nonnegative(param, name)?;
                if !self.styles.contains_key(&number) {
                    return Err(invalid("RTF paragraph references an undefined style."));
                }
                state.style = Some(number);
                state.character = CharacterFormat::default();
                state.font = None;
            }
            "ql" => state.paragraph.alignment = Some(Alignment::Left),
            "qc" => state.paragraph.alignment = Some(Alignment::Center),
            "qr" => state.paragraph.alignment = Some(Alignment::Right),
            "qj" => state.paragraph.alignment = Some(Alignment::Justify),
            "sb" => state.paragraph.spacing_before = Some(Twips(nonnegative(param, name)?)),
            "sa" => state.paragraph.spacing_after = Some(Twips(nonnegative(param, name)?)),
            "li" => {
                state
                    .paragraph
                    .indentation
                    .get_or_insert_with(Indentation::default)
                    .left = Some(Twips(required(param, name)?))
            }
            "ri" => {
                state
                    .paragraph
                    .indentation
                    .get_or_insert_with(Indentation::default)
                    .right = Some(Twips(required(param, name)?))
            }
            "fi" => {
                state
                    .paragraph
                    .indentation
                    .get_or_insert_with(Indentation::default)
                    .first_line = Some(Twips(required(param, name)?))
            }
            "sl" => {
                let value = required(param, name)?;
                if value == i32::MIN {
                    return Err(invalid("RTF line spacing must be representable."));
                }
                state.sl = Some(value);
            }
            "slmult" => {
                let value = required(param, name)?;
                if !matches!(value, 0 | 1) {
                    return Err(invalid("RTF line-spacing multiplier must be zero or one."));
                }
                state.sl_multiple = value == 1;
            }
            "pagebb" => state.paragraph.page_break_before = Some(param != Some(0)),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn geometry_property(
        &mut self,
        name: &str,
        param: Option<i32>,
        at: usize,
    ) -> Result<bool, FormatError> {
        let document_property = matches!(
            name,
            "paperw" | "paperh" | "margt" | "margr" | "margb" | "margl" | "gutter" | "landscape"
        );
        if document_property && (self.paragraph_number > 0 || !self.runs.is_empty()) {
            self.unsupported(
                at,
                "RTF document geometry changes after body text are not supported.",
            );
        }
        let target = match name {
            "paperw" | "pgwsxn" => Some(&mut self.geometry.width),
            "paperh" | "pghsxn" => Some(&mut self.geometry.height),
            "margt" | "margtsxn" => Some(&mut self.geometry.top),
            "margr" | "margrsxn" => Some(&mut self.geometry.right),
            "margb" | "margbsxn" => Some(&mut self.geometry.bottom),
            "margl" | "marglsxn" => Some(&mut self.geometry.left),
            "headery" => Some(&mut self.geometry.header),
            "footery" => Some(&mut self.geometry.footer),
            "gutter" | "guttersxn" => Some(&mut self.geometry.gutter),
            _ => None,
        };
        if let Some(target) = target {
            let value = if matches!(name, "paperw" | "paperh" | "pgwsxn" | "pghsxn") {
                positive(param, name)?
            } else {
                nonnegative(param, name)?
            };
            *target = Some(value);
        } else {
            match name {
                "landscape" | "lndscpsxn" => {
                    self.geometry.orientation = Some(if param == Some(0) {
                        PageOrientation::Portrait
                    } else {
                        PageOrientation::Landscape
                    })
                }
                "sectd" => self.geometry = self.defaults.clone(),
                _ => return Ok(false),
            }
        }
        if document_property {
            self.defaults = self.geometry.clone();
        }
        Ok(true)
    }

    fn default_family(&self) -> Option<String> {
        self.fonts
            .get(&self.default_font.unwrap_or(0))
            .map(|font| font.name.clone())
            .or_else(|| self.fonts.is_empty().then(|| "Times New Roman".into()))
    }

    fn current_format(&self, state: &State) -> CharacterFormat {
        let mut format = state.character.clone();
        if format.font_family.is_none() && !self.style_flags(state.style).0 {
            format.font_family = self.default_family();
        }
        format
    }

    fn style_flags(&self, index: Option<i32>) -> (bool, bool, bool) {
        index
            .and_then(|index| self.style_defaults.get(&index))
            .copied()
            .unwrap_or_default()
    }

    fn unicode(&mut self, value: i32, state: &State) -> Result<(), FormatError> {
        let unit = i16::try_from(value)
            .map_err(|_| invalid("RTF Unicode value must be a signed UTF-16 code unit."))?
            as u16;
        if (0xd800..=0xdbff).contains(&unit) {
            if self.pending_high.is_some() {
                return Err(invalid("Unpaired RTF UTF-16 high surrogate."));
            }
            self.pending_high = Some((unit, self.current_format(state)));
        } else if (0xdc00..=0xdfff).contains(&unit) {
            let (high, format) = self
                .pending_high
                .take()
                .ok_or_else(|| invalid("Unpaired RTF UTF-16 low surrogate."))?;
            if format != self.current_format(state) {
                return Err(invalid(
                    "RTF formatting changes inside a UTF-16 surrogate pair.",
                ));
            }
            let value = 0x1_0000 + ((u32::from(high) - 0xd800) << 10) + u32::from(unit) - 0xdc00;
            self.append(
                char::from_u32(value).ok_or_else(|| invalid("Invalid RTF Unicode scalar."))?,
                format,
            )?;
        } else {
            self.scalar(
                char::from_u32(u32::from(unit))
                    .ok_or_else(|| invalid("Invalid RTF Unicode scalar."))?,
                state,
            )?;
        }
        Ok(())
    }

    fn scalar(&mut self, scalar: char, state: &State) -> Result<(), FormatError> {
        if self.pending_high.is_some() {
            return Err(invalid("Unpaired RTF UTF-16 high surrogate."));
        }
        if scalar.is_control() && !matches!(scalar, '\n' | '\t') {
            return Err(invalid("Unsupported control character in RTF text."));
        }
        self.append(scalar, self.current_format(state))
    }

    fn append(&mut self, scalar: char, format: CharacterFormat) -> Result<(), FormatError> {
        self.append_text(scalar.encode_utf8(&mut [0; 4]), format)
    }

    fn append_text(&mut self, text: &str, format: CharacterFormat) -> Result<(), FormatError> {
        if let Some(previous) = self.runs.last_mut().filter(|run| run.format == format) {
            previous.text.push_str(text);
        } else {
            if self.runs.len() >= MAX_TOKENS {
                return Err(limit());
            }
            self.runs.push(TextRun {
                text: text.into(),
                format,
            });
        }
        self.paragraph_pending = true;
        Ok(())
    }

    fn finish_paragraph(&mut self, state: &State, explicit: bool) -> Result<(), FormatError> {
        if self.pending_high.is_some() {
            return Err(invalid("Unpaired RTF UTF-16 high surrogate."));
        }
        if self.paragraph_number >= MAX_PARAGRAPHS {
            return Err(limit());
        }
        let mut format = state.paragraph.clone();
        if let Some(spacing) = state.sl {
            format.line_spacing = Some(if spacing == 0 {
                LineSpacing::Multiple { value: 240 }
            } else if state.sl_multiple {
                LineSpacing::Multiple {
                    value: u32::try_from(spacing)
                        .map_err(|_| invalid("RTF line multiple must be positive."))?,
                }
            } else if spacing < 0 {
                LineSpacing::Exact {
                    height: Twips(-spacing),
                }
            } else {
                LineSpacing::AtLeast {
                    height: Twips(spacing),
                }
            });
        }
        // Source RTF defaults differ from Writer's documented layout defaults.
        if format.spacing_after.is_none() && !self.style_flags(state.style).1 {
            format.spacing_after = Some(Twips(0));
        }
        if format.line_spacing.is_none() && !self.style_flags(state.style).2 {
            format.line_spacing = Some(LineSpacing::Multiple { value: 240 });
        }
        let id = self.id_for("paragraph", self.paragraph_number)?;
        let mut paragraph = Paragraph::new(id, std::mem::take(&mut self.runs)).with_format(format);
        if let Some(style) = state.style {
            paragraph = paragraph.with_style(self.id_for("style", style)?);
        }
        self.blocks.push(Block::Paragraph(paragraph));
        self.paragraph_number += 1;
        self.paragraph_pending = !explicit;
        Ok(())
    }

    fn finish_section(&mut self) -> Result<(), FormatError> {
        let mut section = Section::new(
            self.id_for("section", self.sections.len())?,
            std::mem::take(&mut self.blocks),
        );
        if let Some(properties) = self.geometry.properties() {
            section = section.with_properties(properties);
        }
        self.sections.push(section);
        self.paragraph_pending = false;
        Ok(())
    }

    fn font_table(
        &mut self,
        start: usize,
        end: usize,
        inherited: &State,
    ) -> Result<(), FormatError> {
        let mut at = start;
        while at < end {
            if matches!(self.items[at].token, Token::GroupStart) {
                let close = self.items[at].close;
                self.font_entry(at + 1, close, inherited)?;
                at = close + 1;
            } else if matches!(self.items[at].token, Token::ControlWord { name: "f", .. }) {
                let mut next = at + 1;
                while next < end {
                    if matches!(self.items[next].token, Token::GroupStart) {
                        next = self.items[next].close + 1;
                        continue;
                    }
                    if matches!(self.items[next].token, Token::ControlWord { name: "f", .. }) {
                        break;
                    }
                    next += 1;
                }
                self.font_entry(at, next, inherited)?;
                at = next;
            } else if matches!(self.items[at].token, Token::Text(text) if text.trim().is_empty()) {
                at += 1;
            } else {
                return Err(invalid("Malformed RTF font table."));
            }
        }
        Ok(())
    }

    fn font_entry(
        &mut self,
        start: usize,
        end: usize,
        inherited: &State,
    ) -> Result<(), FormatError> {
        let mut index = None;
        let mut charset = 0;
        let mut text = String::new();
        let mut terminated = false;
        let mut state = State {
            codepage: inherited.codepage,
            uc: inherited.uc,
            ..Default::default()
        };
        let mut at = start;
        while at < end {
            if self.skip > 0 && matches!(self.items[at].token, Token::ControlWord { .. }) {
                self.skip -= 1;
                at += 1;
                continue;
            }
            match self.items[at].token {
                Token::ControlWord { name: "f", param } => index = Some(nonnegative(param, "f")?),
                Token::ControlWord {
                    name: "fcharset",
                    param,
                } => charset = nonnegative(param, "fcharset")?,
                Token::ControlWord {
                    name:
                        "fnil" | "froman" | "fswiss" | "fmodern" | "fscript" | "fdecor" | "ftech"
                        | "fbidi" | "fprq",
                    ..
                } => {}
                Token::GroupStart => {
                    self.unsupported(at, "RTF alternate/embedded font metadata is not supported.");
                    at = self.items[at].close;
                }
                token => {
                    self.table_text(token, &mut state, &mut text, &mut terminated, at, true)?
                }
            }
            at += 1;
        }
        if !terminated || text.trim().is_empty() {
            return Err(invalid(
                "RTF font entry must end in a nonempty name and semicolon.",
            ));
        }
        if text.len() > 256 {
            return Err(limit());
        }
        let index = index.ok_or_else(|| invalid("RTF font entry has no index."))?;
        if self.fonts.len() >= MAX_TABLE_ENTRIES {
            return Err(limit());
        }
        if self
            .fonts
            .insert(
                index,
                Font {
                    name: text.trim().into(),
                    charset,
                },
            )
            .is_some()
        {
            return Err(invalid("Duplicate RTF font index."));
        }
        self.skip = 0;
        if self.pending_high.is_some() {
            return Err(invalid("Unpaired Unicode surrogate in RTF font name."));
        }
        Ok(())
    }

    fn cache_style_defaults(&mut self) -> Result<(), FormatError> {
        // Validate the complete styles graph before using cached inheritance
        // flags. Native model validation remains the source of graph invariants.
        Document::from_parts(
            self.id.clone(),
            DocumentMetadata::default(),
            Vec::new(),
            self.styles.values().cloned().collect(),
        )?;
        let parents: BTreeMap<&str, &NamedStyle> = self
            .styles
            .values()
            .map(|style| (style.id.as_str(), style))
            .collect();
        for (number, style) in &self.styles {
            let mut current = Some(style);
            let mut flags = (false, false, false);
            let mut depth = 0;
            while let Some(style) = current {
                if depth >= MAX_DEPTH {
                    return Err(limit());
                }
                flags.0 |= style.character_format.font_family.is_some();
                flags.1 |= style.paragraph_format.spacing_after.is_some();
                flags.2 |= style.paragraph_format.line_spacing.is_some();
                depth += 1;
                current = style
                    .based_on
                    .as_ref()
                    .and_then(|id| parents.get(id.as_str()).copied());
            }
            self.style_defaults.insert(*number, flags);
        }
        Ok(())
    }

    fn color_table(&mut self, start: usize, end: usize) -> Result<(), FormatError> {
        let mut channels = [0u8; 3];
        let mut specified = false;
        for at in start..end {
            match self.items[at].token {
                Token::ControlWord { name, param } if matches!(name, "red" | "green" | "blue") => {
                    let value = u8::try_from(required(param, name)?)
                        .map_err(|_| invalid("RTF color channel is out of range."))?;
                    channels[match name {
                        "red" => 0,
                        "green" => 1,
                        _ => 2,
                    }] = value;
                    specified = true;
                }
                Token::Text(text) => {
                    for byte in text.bytes() {
                        if byte == b';' {
                            if self.colors.len() >= MAX_TABLE_ENTRIES {
                                return Err(limit());
                            }
                            self.colors.push(specified.then_some(TextColor::Rgb {
                                red: channels[0],
                                green: channels[1],
                                blue: channels[2],
                            }));
                            channels = [0; 3];
                            specified = false;
                        } else if !byte.is_ascii_whitespace() {
                            return Err(invalid("Unexpected RTF color table text."));
                        }
                    }
                }
                _ => return Err(invalid("Malformed RTF color table.")),
            }
        }
        if specified {
            return Err(invalid("RTF color entry is missing a semicolon."));
        }
        Ok(())
    }

    fn style_table(
        &mut self,
        start: usize,
        end: usize,
        inherited: &State,
    ) -> Result<(), FormatError> {
        let mut at = start;
        while at < end {
            if !matches!(self.items[at].token, Token::GroupStart) {
                return Err(invalid("Malformed RTF stylesheet."));
            }
            let close = self.items[at].close;
            let mut state = State {
                codepage: inherited.codepage,
                uc: inherited.uc,
                ..Default::default()
            };
            let mut index = None;
            let mut parent = None;
            let mut next_style = None;
            let mut text = String::new();
            let mut terminated = false;
            for current in at + 1..close {
                if self.skip > 0 && matches!(self.items[current].token, Token::ControlWord { .. }) {
                    self.skip -= 1;
                    continue;
                }
                match self.items[current].token {
                    Token::ControlWord { name: "s", param } => {
                        index = Some(nonnegative(param, "s")?)
                    }
                    Token::ControlWord {
                        name: "sbasedon",
                        param,
                    } => {
                        let base = nonnegative(param, "sbasedon")?;
                        if base != 222 {
                            parent = Some(base);
                        }
                    }
                    Token::ControlWord {
                        name: "snext",
                        param,
                    } => next_style = Some(nonnegative(param, "snext")?),
                    Token::ControlWord { name, param } if name != "u" && name != "uc" => {
                        if !self.property(name, param, &mut state, current)? {
                            self.unsupported(
                                current,
                                format!("Unsupported RTF style property \\{name}."),
                            );
                        }
                    }
                    token => self.table_text(
                        token,
                        &mut state,
                        &mut text,
                        &mut terminated,
                        current,
                        true,
                    )?,
                }
            }
            if !terminated || text.trim().is_empty() {
                return Err(invalid(
                    "RTF paragraph style requires a name and semicolon.",
                ));
            }
            if text.len() > 1024 {
                return Err(limit());
            }
            let Some(index) = index else {
                self.unsupported(at, "RTF character/table/list styles are not supported.");
                at = close + 1;
                continue;
            };
            if next_style.is_some_and(|next| next != index) {
                self.unsupported(
                    at,
                    "RTF next-paragraph style behavior is not represented by the current model.",
                );
            }
            if self.styles.len() >= MAX_TABLE_ENTRIES {
                return Err(limit());
            }
            let mut paragraph_format = state.paragraph;
            if let Some(spacing) = state.sl {
                paragraph_format.line_spacing = Some(if spacing == 0 {
                    LineSpacing::Multiple { value: 240 }
                } else if state.sl_multiple {
                    LineSpacing::Multiple {
                        value: u32::try_from(spacing)
                            .map_err(|_| invalid("RTF line multiple must be positive."))?,
                    }
                } else if spacing < 0 {
                    LineSpacing::Exact {
                        height: Twips(-spacing),
                    }
                } else {
                    LineSpacing::AtLeast {
                        height: Twips(spacing),
                    }
                });
            }
            let style = NamedStyle {
                id: self.id_for("style", index)?,
                name: text.trim().into(),
                based_on: parent
                    .map(|number| self.id_for("style", number))
                    .transpose()?,
                paragraph_format,
                character_format: state.character,
            };
            if self.styles.insert(index, style).is_some() {
                return Err(invalid("Duplicate RTF style index."));
            }
            at = close + 1;
        }
        self.skip = 0;
        if self.pending_high.is_some() {
            return Err(invalid("Unpaired Unicode surrogate in RTF style name."));
        }
        self.cache_style_defaults()
    }

    fn table_text(
        &mut self,
        token: Token<'_>,
        state: &mut State,
        text: &mut String,
        terminated: &mut bool,
        at: usize,
        delimited: bool,
    ) -> Result<(), FormatError> {
        if self.skip > 0 && matches!(token, Token::ControlWord { .. }) {
            self.skip -= 1;
            return Ok(());
        }
        match token {
            Token::Text(value) => {
                for scalar in value.chars() {
                    if self.skip > 0 {
                        self.skip -= 1;
                        continue;
                    }
                    if scalar == ';' && delimited {
                        *terminated = true;
                    } else if !*terminated {
                        if self.pending_high.is_some() {
                            return Err(invalid("Unpaired Unicode surrogate in RTF label."));
                        }
                        text.push(scalar);
                    } else if !scalar.is_ascii_whitespace() {
                        return Err(invalid("Unexpected text after an RTF table name."));
                    }
                }
            }
            Token::ControlSymbol(ControlSymbol::HexByte(byte)) | Token::RawByte(byte) => {
                if self.skip > 0 {
                    self.skip -= 1;
                } else {
                    if self.pending_high.is_some() {
                        return Err(invalid("Unpaired Unicode surrogate in RTF label."));
                    }
                    text.push(state.codepage.decode(byte).ok_or_else(|| {
                        invalid("RTF table name uses an unsupported code-page byte.")
                    })?);
                }
            }
            Token::ControlSymbol(ControlSymbol::Literal(scalar)) => {
                if self.skip > 0 {
                    self.skip -= 1;
                } else {
                    if self.pending_high.is_some() {
                        return Err(invalid("Unpaired Unicode surrogate in RTF label."));
                    }
                    text.push(scalar);
                }
            }
            Token::ControlWord { name: "uc", param } => {
                let value = nonnegative(param, "uc")?;
                if value > 32 {
                    return Err(invalid("RTF fallback count is out of range."));
                }
                state.uc = value as usize;
            }
            Token::ControlWord { name: "u", param } => {
                let value = i16::try_from(required(param, "u")?)
                    .map_err(|_| invalid("RTF table Unicode value is out of range."))?
                    as u16;
                // Names and titles can contain supplementary Unicode, too.
                if (0xd800..=0xdbff).contains(&value) {
                    if self.pending_high.is_some() {
                        return Err(invalid("Unpaired table-name surrogate."));
                    }
                    self.pending_high = Some((value, CharacterFormat::default()));
                } else if (0xdc00..=0xdfff).contains(&value) {
                    let (high, _) = self
                        .pending_high
                        .take()
                        .ok_or_else(|| invalid("Unpaired table-name surrogate."))?;
                    text.push(
                        char::from_u32(
                            0x10000 + ((u32::from(high) - 0xd800) << 10) + u32::from(value)
                                - 0xdc00,
                        )
                        .ok_or_else(|| invalid("Invalid table-name Unicode."))?,
                    );
                } else {
                    if self.pending_high.is_some() {
                        return Err(invalid("Unpaired table-name surrogate."));
                    }
                    text.push(
                        char::from_u32(u32::from(value))
                            .ok_or_else(|| invalid("Invalid table-name Unicode."))?,
                    );
                }
                self.skip = state.uc;
            }
            _ => self.unsupported(at, "Unsupported RTF table-name metadata."),
        }
        Ok(())
    }

    fn information(
        &mut self,
        start: usize,
        end: usize,
        inherited: &State,
    ) -> Result<(), FormatError> {
        let mut at = start;
        while at < end {
            if !matches!(self.items[at].token, Token::GroupStart) {
                return Err(invalid("Malformed RTF information group."));
            }
            let close = self.items[at].close;
            let (destination, first, _) = self.destination(at);
            match destination {
                Some("title") => {
                    if self.title_seen {
                        return Err(invalid("Duplicate RTF title metadata."));
                    }
                    self.title_seen = true;
                    let mut text = String::new();
                    let mut terminated = false;
                    let mut state = State {
                        codepage: inherited.codepage,
                        uc: inherited.uc,
                        ..Default::default()
                    };
                    for current in first + 1..close {
                        self.table_text(
                            self.items[current].token,
                            &mut state,
                            &mut text,
                            &mut terminated,
                            current,
                            false,
                        )?;
                    }
                    if text.len() > 16_384 {
                        return Err(limit());
                    }
                    self.skip = 0;
                    if self.pending_high.is_some() {
                        return Err(invalid("Unpaired Unicode surrogate in RTF title."));
                    }
                    self.title = text;
                }
                Some(
                    "creatim" | "revtim" | "printim" | "buptim" | "version" | "edmins" | "nofpages"
                    | "nofwords" | "nofchars" | "nofcharsws" | "vern",
                ) => {
                    for current in first + 1..close {
                        match self.items[current].token {
                            Token::ControlWord {name:"yr"|"mo"|"dy"|"hr"|"min"|"sec",param:Some(value)} if value>=0 => {},
                            Token::Text(value) if value.trim().is_empty()=>{},
                            _=>self.unsupported(current,"Unsupported nested information metadata is retained in the original."),
                        }
                    }
                }
                _ => self.unsupported(
                    at,
                    "Unsupported RTF document metadata is retained in the original.",
                ),
            }
            at = close + 1;
        }
        Ok(())
    }
}

fn required(value: Option<i32>, name: &str) -> Result<i32, FormatError> {
    value.ok_or_else(|| invalid(format!("RTF \\{name} requires a numeric parameter.")))
}
fn nonnegative(value: Option<i32>, name: &str) -> Result<i32, FormatError> {
    let n = required(value, name)?;
    if n < 0 {
        Err(invalid(format!("RTF \\{name} must be nonnegative.")))
    } else {
        Ok(n)
    }
}
fn positive(value: Option<i32>, name: &str) -> Result<i32, FormatError> {
    let n = required(value, name)?;
    if n <= 0 {
        Err(invalid(format!("RTF \\{name} must be positive.")))
    } else {
        Ok(n)
    }
}

fn is_destination(name: &str) -> bool {
    matches!(
        name,
        "pict"
            | "object"
            | "field"
            | "fldinst"
            | "fldrslt"
            | "header"
            | "headerl"
            | "headerr"
            | "headerf"
            | "footer"
            | "footerl"
            | "footerr"
            | "footerf"
            | "footnote"
            | "annotation"
            | "listtable"
            | "listoverridetable"
            | "shp"
            | "shpinst"
            | "shprslt"
            | "upr"
            | "ud"
            | "xmlopen"
            | "datastore"
            | "filetbl"
            | "revtbl"
            | "rsidtbl"
            | "themedata"
            | "colorschememapping"
            | "datafield"
            | "docvar"
            | "bkmkstart"
            | "bkmkend"
    )
}

pub(crate) fn import_document(
    bytes: &[u8],
    id: Id,
    filename: &str,
) -> Result<ImportOutcome, FormatError> {
    let mut parser = Parser::new(lex(bytes)?, id);
    let end = parser.items[0].close;
    let mut state = State::default();
    parser.body(1, end, &mut state)?;
    if (parser.default_font.is_some() || !parser.fonts.is_empty())
        && !parser.fonts.contains_key(&parser.default_font.unwrap_or(0))
    {
        return Err(invalid("RTF default font is not defined."));
    }
    if parser.pending_high.is_some() {
        return Err(invalid("RTF ended with an unpaired Unicode surrogate."));
    }
    if parser.skip > 0 {
        return Err(invalid("RTF ended before its Unicode fallback characters."));
    }
    if parser.paragraph_pending
        || !parser.runs.is_empty()
        || (parser.blocks.is_empty() && parser.sections.is_empty())
    {
        parser.finish_paragraph(&state, false)?;
    }
    if !parser.blocks.is_empty() || parser.sections.is_empty() {
        parser.finish_section()?;
    }
    if !parser.report.is_empty() {
        return Ok(ImportOutcome::Preserved {
            original: crate::original_source(DocumentFormat::Rtf, bytes, filename)?
                .ok_or_else(|| invalid("RTF source is empty."))?,
            report: parser.report,
        });
    }
    let document = Document::from_parts(
        parser.id,
        DocumentMetadata {
            title: parser.title,
        },
        parser.sections,
        parser.styles.into_values().collect(),
    )?;
    Ok(ImportOutcome::Editable(crate::retain_source(
        DocumentFormat::Rtf,
        document,
        bytes,
        filename,
    )?))
}
