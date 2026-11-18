//! Bounded immutable raster assets. Original bytes persist; render caches do not.

use crate::{Id, Twips, ValidationError};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, io::Cursor, sync::Arc};
use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
use zune_jpeg::JpegDecoder;

/// Maximum compressed bytes in one raster asset.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum width or height, before and after orientation.
pub const MAX_IMAGE_DIMENSION: u32 = 8192;
/// Maximum decoded pixels per asset.
pub const MAX_IMAGE_PIXELS: u64 = 16_000_000;
/// Maximum image block count per document.
pub const MAX_DOCUMENT_IMAGES: usize = 32;
/// Maximum original raster bytes per document.
pub const MAX_DOCUMENT_IMAGE_BYTES: usize = 16 * 1024 * 1024;
/// Maximum aggregate decoded pixels per document (64 MB RGBA).
pub const MAX_DOCUMENT_IMAGE_PIXELS: u64 = 16_000_000;
/// Maximum UTF-8 alternative-text bytes per image.
pub const MAX_IMAGE_ALT_TEXT_BYTES: usize = 8192;
const MAX_EXIF_BYTES: usize = 256 * 1024;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Supported inert raster encodings; active/vector formats are not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RasterFormat {
    /// Portable Network Graphics, one static frame.
    Png,
    /// JPEG, including bounded progressive scans.
    Jpeg,
}
impl RasterFormat {
    /// Returns the canonical media type.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }
}

/// Preliminary dimensions from a bounded raster header, before pixel allocation.
///
/// Dimensions are unoriented; their product is invariant under EXIF rotation.
/// A header does not prove that the complete image is valid: construct an
/// [`ImageAsset`] before publishing editable content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RasterHeader {
    /// Encoding discovered from actual bytes.
    pub format: RasterFormat,
    /// Source width in pixels before orientation.
    pub width_px: u32,
    /// Source height in pixels before orientation.
    pub height_px: u32,
}

/// Structured failure from raster validation or normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// Only PNG and JPEG are supported.
    UnsupportedFormat,
    /// Syntax, checksum, metadata, or raster data is malformed.
    InvalidData {
        /// Bounded explanatory message.
        reason: String,
    },
    /// A documented image resource limit would be exceeded.
    LimitExceeded {
        /// Resource name.
        resource: &'static str,
        /// Maximum supported value.
        limit: u64,
        /// Rejected value.
        actual: u64,
    },
    /// Animated images require a separately defined model.
    UnsupportedAnimation,
    /// A valid but ambiguous metadata arrangement cannot be normalized safely.
    UnsupportedMetadata {
        /// The unsupported metadata arrangement.
        reason: &'static str,
    },
}
impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat => f.write_str("only static PNG and JPEG images are supported"),
            Self::InvalidData { reason } => write!(f, "invalid image: {reason}"),
            Self::LimitExceeded {
                resource,
                limit,
                actual,
            } => write!(f, "image {resource} exceeds {limit}: {actual}"),
            Self::UnsupportedAnimation => f.write_str("animated PNG images are unsupported"),
            Self::UnsupportedMetadata { reason } => {
                write!(f, "unsupported image metadata: {reason}")
            }
        }
    }
}
impl std::error::Error for ImageError {}
fn invalid(reason: impl ToString) -> ImageError {
    ImageError::InvalidData {
        reason: reason.to_string(),
    }
}
fn limit(resource: &'static str, maximum: u64, actual: u64) -> ImageError {
    ImageError::LimitExceeded {
        resource,
        limit: maximum,
        actual,
    }
}
fn pixels(width: u32, height: u32) -> Result<u64, ImageError> {
    if width == 0 || height == 0 {
        return Err(invalid("dimensions must be positive"));
    }
    if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return Err(limit(
            "dimension",
            u64::from(MAX_IMAGE_DIMENSION),
            u64::from(width.max(height)),
        ));
    }
    let count = u64::from(width) * u64::from(height);
    if count > MAX_IMAGE_PIXELS {
        return Err(limit("pixels", MAX_IMAGE_PIXELS, count));
    }
    Ok(count)
}

struct AssetData {
    format: RasterFormat,
    original: Arc<[u8]>,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    png: Vec<u8>,
    orientation: u16,
    profile: bool,
    approximation: bool,
}
/// Immutable original PNG/JPEG plus a shared, derived orientation-safe raster.
///
/// Decode happens once at construction/deserialization. Cloning/undo share the
/// original and render buffers; ordinary validation never decodes image bytes.
#[derive(Clone)]
pub struct ImageAsset(Arc<AssetData>);
impl fmt::Debug for ImageAsset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageAsset")
            .field("format", &self.format())
            .field("bytes", &self.original_bytes().len())
            .field("width", &self.width_px())
            .field("height", &self.height_px())
            .finish_non_exhaustive()
    }
}
impl PartialEq for ImageAsset {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
            || (self.format() == other.format() && self.original_bytes() == other.original_bytes())
    }
}
impl Eq for ImageAsset {}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetWire {
    format: RasterFormat,
    bytes_base64: String,
}
impl Serialize for ImageAsset {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        AssetWire {
            format: self.format(),
            bytes_base64: STANDARD.encode(self.original_bytes()),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ImageAsset {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = AssetWire::deserialize(deserializer)?;
        let bytes = decode_base64(&wire.bytes_base64).map_err(serde::de::Error::custom)?;
        let asset = Self::from_bytes(&bytes).map_err(serde::de::Error::custom)?;
        if asset.format() != wire.format {
            return Err(serde::de::Error::custom(
                "raster format does not match bytes",
            ));
        }
        Ok(asset)
    }
}
impl ImageAsset {
    /// Reads bounded preliminary headers for aggregate importer resource checks.
    pub fn inspect_header(bytes: &[u8]) -> Result<RasterHeader, ImageError> {
        let (format, width_px, height_px) = inspect_dimensions(bytes)?;
        Ok(RasterHeader {
            format,
            width_px,
            height_px,
        })
    }
    /// Validates bounded original bytes and constructs the shared render cache.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ImageError> {
        let (format, _, _) = inspect_dimensions(bytes)?;
        let (width, height, rgba, orientation, profile, approximation) = match format {
            RasterFormat::Png => decode_png(bytes)?,
            RasterFormat::Jpeg => decode_jpeg(bytes)?,
        };
        let (width, height, rgba) = orient(width, height, rgba, orientation);
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Fast);
            encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
            let mut writer = encoder.write_header().map_err(invalid)?;
            writer.write_image_data(&rgba).map_err(invalid)?;
            writer.finish().map_err(invalid)?;
        }
        Ok(Self(Arc::new(AssetData {
            format,
            original: Arc::from(bytes),
            width,
            height,
            rgba,
            png,
            orientation,
            profile,
            approximation,
        })))
    }
    /// Original encoding, checked against its file signature and complete decode.
    #[must_use]
    pub fn format(&self) -> RasterFormat {
        self.0.format
    }
    /// Original media type, never supplied by the browser.
    #[must_use]
    pub fn media_type(&self) -> &'static str {
        self.format().media_type()
    }
    /// Exact original file bytes, including retained inert metadata.
    #[must_use]
    pub fn original_bytes(&self) -> &[u8] {
        &self.0.original
    }
    /// Oriented display width in pixels.
    #[must_use]
    pub fn width_px(&self) -> u32 {
        self.0.width
    }
    /// Oriented display height in pixels.
    #[must_use]
    pub fn height_px(&self) -> u32 {
        self.0.height
    }
    /// Canonical static 8-bit RGBA PNG, with orientation applied by Rust.
    #[must_use]
    pub fn render_png(&self) -> &[u8] {
        &self.0.png
    }
    /// Oriented row-major RGBA pixels used by native PDF and browser preview.
    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.0.rgba
    }
    /// Whether the source declares an embedded ICC profile (not color-managed).
    #[must_use]
    pub fn has_color_profile(&self) -> bool {
        self.0.profile
    }
    /// Whether EXIF orientation required changing pixel coordinates.
    #[must_use]
    pub fn has_orientation_transform(&self) -> bool {
        self.0.orientation != 1
    }
    /// Whether preview normalizes high bit depth or source color interpretation.
    #[must_use]
    pub fn has_color_approximation(&self) -> bool {
        self.0.approximation
    }
}

pub(crate) fn decode_base64(value: &str) -> Result<Vec<u8>, ImageError> {
    if value.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4 {
        return Err(limit(
            "encoded bytes",
            (MAX_IMAGE_BYTES.div_ceil(3) * 4) as u64,
            value.len() as u64,
        ));
    }
    let bytes = STANDARD.decode(value).map_err(invalid)?;
    if STANDARD.encode(&bytes) != value {
        return Err(invalid("image base64 must use canonical padded encoding"));
    }
    Ok(bytes)
}
/// Checks actual raster headers without allocating decoded pixel buffers.
pub(crate) fn inspect_dimensions(bytes: &[u8]) -> Result<(RasterFormat, u32, u32), ImageError> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(limit("bytes", MAX_IMAGE_BYTES as u64, bytes.len() as u64));
    }
    let (format, width, height) = if bytes.starts_with(PNG_SIGNATURE) {
        let mut decoder = png_decoder(bytes);
        let info = decoder.read_header_info().map_err(invalid)?;
        (RasterFormat::Png, info.width, info.height)
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        jpeg_metadata(bytes)?;
        let mut decoder = jpeg_decoder(bytes);
        decoder.decode_headers().map_err(invalid)?;
        let (width, height) = decoder
            .dimensions()
            .ok_or_else(|| invalid("JPEG dimensions are missing"))?;
        (
            RasterFormat::Jpeg,
            u32::try_from(width).map_err(invalid)?,
            u32::try_from(height).map_err(invalid)?,
        )
    } else {
        return Err(ImageError::UnsupportedFormat);
    };
    pixels(width, height)?;
    Ok((format, width, height))
}
fn png_decoder(bytes: &[u8]) -> png::Decoder<Cursor<&[u8]>> {
    let mut options = png::DecodeOptions::default();
    options.set_ignore_checksums(false);
    options.set_skip_ancillary_crc_failures(false);
    let mut decoder = png::Decoder::new_with_options(Cursor::new(bytes), options);
    decoder.set_limits(png::Limits {
        bytes: MAX_IMAGE_PIXELS as usize * 4 + 1024 * 1024,
    });
    decoder.set_ignore_text_chunk(true);
    // Never inflate potentially hostile ICC metadata. Exact bytes remain retained.
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    decoder
}
fn jpeg_decoder(bytes: &[u8]) -> JpegDecoder<ZCursor<&[u8]>> {
    let options = DecoderOptions::new_safe()
        .set_strict_mode(true)
        .set_max_width(MAX_IMAGE_DIMENSION as usize)
        .set_max_height(MAX_IMAGE_DIMENSION as usize)
        .jpeg_set_max_scans(100)
        .jpeg_set_out_colorspace(ColorSpace::RGBA);
    JpegDecoder::new_with_options(ZCursor::new(bytes), options)
}
type Decoded = (u32, u32, Vec<u8>, u16, bool, bool);
fn decode_png(bytes: &[u8]) -> Result<Decoded, ImageError> {
    let (profile, exif, color_hint) = png_metadata(bytes)?;
    let mut reader = png_decoder(bytes).read_info().map_err(invalid)?;
    if reader.info().animation_control.is_some() {
        return Err(ImageError::UnsupportedAnimation);
    }
    let orientation = orientation(exif)?;
    let approximation = profile
        || color_hint
        || reader.info().bit_depth == png::BitDepth::Sixteen
        || (reader.info().srgb.is_none()
            && reader
                .info()
                .gama_chunk
                .is_some_and(|g| g.into_scaled() != 45455));
    let count = pixels(reader.info().width, reader.info().height)? as usize;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| invalid("PNG output size overflow"))?;
    if size > count * 4 {
        return Err(limit("decoded bytes", (count * 4) as u64, size as u64));
    }
    let mut output = vec![0; size];
    let frame = reader.next_frame(&mut output).map_err(invalid)?;
    reader.finish().map_err(invalid)?;
    let mut rgba = Vec::with_capacity(count * 4);
    let source = &output[..frame.buffer_size()];
    if source.len() != count * frame.color_type.samples() {
        return Err(invalid("PNG decoded channel count differs from dimensions"));
    }
    match frame.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(source),
        png::ColorType::Rgb => {
            for p in source.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for &g in source {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for p in source.as_chunks::<2>().0 {
                rgba.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
            }
        }
        png::ColorType::Indexed => return Err(invalid("palette was not expanded")),
    }
    if rgba.len() != count * 4 {
        return Err(invalid("PNG decoded byte count differs from dimensions"));
    }
    Ok((
        frame.width,
        frame.height,
        rgba,
        orientation,
        profile,
        approximation,
    ))
}
fn decode_jpeg(bytes: &[u8]) -> Result<Decoded, ImageError> {
    let (exif, profile) = jpeg_metadata(bytes)?;
    let mut decoder = jpeg_decoder(bytes);
    decoder.decode_headers().map_err(invalid)?;
    let (width, height) = decoder
        .dimensions()
        .ok_or_else(|| invalid("JPEG dimensions are missing"))?;
    let count = pixels(width as u32, height as u32)? as usize;
    let expected = count * 4;
    if decoder.output_buffer_size() != Some(expected) {
        return Err(invalid(
            "JPEG output byte count differs from RGBA dimensions",
        ));
    }
    let orientation = orientation(exif)?;
    let approximation = profile
        || matches!(
            decoder.input_colorspace(),
            Some(ColorSpace::CMYK | ColorSpace::YCCK)
        );
    let rgba = decoder.decode().map_err(invalid)?;
    if rgba.len() != expected {
        return Err(invalid("JPEG decoded byte count differs from dimensions"));
    }
    Ok((
        width as u32,
        height as u32,
        rgba,
        orientation,
        profile,
        approximation,
    ))
}

// This is bounded JPEG container framing, not entropy or TIFF decoding. The
// mature decoder remains responsible for interpreting the compressed raster.
// Inspect every marker so later metadata cannot change another consumer's view
// of the exact original. FF00 stuffing and restart markers remain scan bytes.
fn jpeg_metadata(bytes: &[u8]) -> Result<(Option<&[u8]>, bool), ImageError> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return Err(invalid("JPEG start marker is missing"));
    }
    let mut at = 2;
    let mut in_scan = false;
    let mut seen_scan = false;
    let mut exif = None;
    let mut profile = false;
    loop {
        if in_scan {
            while bytes.get(at).is_some_and(|b| *b != 0xff) {
                at += 1;
            }
        }
        if bytes.get(at) != Some(&0xff) {
            return Err(invalid("JPEG marker or end marker is missing"));
        }
        while bytes.get(at) == Some(&0xff) {
            at += 1;
        }
        let marker = *bytes
            .get(at)
            .ok_or_else(|| invalid("truncated JPEG marker"))?;
        at += 1;
        if in_scan && (marker == 0 || (0xd0..=0xd7).contains(&marker)) {
            continue;
        }
        in_scan = false;
        if marker == 0xd9 {
            if !seen_scan || at != bytes.len() {
                return Err(invalid(
                    "JPEG end marker precedes scans or has trailing bytes",
                ));
            }
            return Ok((exif, profile));
        }
        if marker == 0 || marker == 0xd8 || (0xd0..=0xd7).contains(&marker) {
            return Err(invalid("unexpected JPEG standalone marker"));
        }
        if marker == 1 {
            continue;
        }
        let length = bytes
            .get(at..at + 2)
            .ok_or_else(|| invalid("truncated JPEG marker length"))?;
        let size = usize::from(u16::from_be_bytes([length[0], length[1]]));
        if size < 2 {
            return Err(invalid(
                "JPEG marker length is smaller than its length field",
            ));
        }
        let end = at
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| invalid("JPEG marker exceeds file"))?;
        let payload = &bytes[at + 2..end];
        if seen_scan && ((0xe0..=0xef).contains(&marker) || marker == 0xfe) {
            return Err(ImageError::UnsupportedMetadata {
                reason: "metadata after the first JPEG scan",
            });
        }
        if marker == 0xe1 && payload.starts_with(b"Exif\0\0") {
            if exif.is_some() {
                return Err(invalid("duplicate JPEG EXIF segments"));
            }
            let raw = &payload[6..];
            if raw.len() > MAX_EXIF_BYTES {
                return Err(limit("EXIF bytes", MAX_EXIF_BYTES as u64, raw.len() as u64));
            }
            exif = Some(raw);
        }
        profile |= marker == 0xe2 && payload.starts_with(b"ICC_PROFILE\0");
        at = end;
        if marker == 0xda {
            seen_scan = true;
            in_scan = true;
        }
    }
}
fn orientation(raw: Option<&[u8]>) -> Result<u16, ImageError> {
    let Some(raw) = raw else { return Ok(1) };
    if raw.len() > MAX_EXIF_BYTES {
        return Err(limit("EXIF bytes", MAX_EXIF_BYTES as u64, raw.len() as u64));
    }
    let exif = exif::Reader::new()
        .read_raw(raw.to_vec())
        .map_err(invalid)?;
    let mut fields = exif
        .fields()
        .filter(|field| field.tag == exif::Tag::Orientation && field.ifd_num == exif::In::PRIMARY);
    let Some(field) = fields.next() else {
        return Ok(1);
    };
    if fields.next().is_some() {
        return Err(invalid("duplicate EXIF orientation"));
    }
    match &field.value {
        exif::Value::Short(values) if values.len() == 1 && (1..=8).contains(&values[0]) => {
            Ok(values[0])
        }
        _ => Err(invalid("EXIF orientation must be one SHORT in 1..=8")),
    }
}
fn png_metadata(bytes: &[u8]) -> Result<(bool, Option<&[u8]>, bool), ImageError> {
    let mut at = 8usize;
    let mut profile = false;
    let mut color_hint = false;
    let mut exif = None;
    let mut ended = false;
    while at < bytes.len() {
        let header = bytes
            .get(at..at + 8)
            .ok_or_else(|| invalid("truncated PNG chunk"))?;
        let size = u32::from_be_bytes(header[..4].try_into().expect("four bytes")) as usize;
        let end = at
            .checked_add(12)
            .and_then(|n| n.checked_add(size))
            .filter(|&n| n <= bytes.len())
            .ok_or_else(|| invalid("PNG chunk exceeds file"))?;
        match &header[4..8] {
            b"acTL" | b"fcTL" | b"fdAT" => return Err(ImageError::UnsupportedAnimation),
            b"iCCP" => profile = true,
            b"cICP" | b"mDCV" | b"cLLI" => color_hint = true,
            b"cHRM" => {
                let standard = [31270_u32, 32900, 64000, 33000, 30000, 60000, 15000, 6000];
                let canonical: Vec<u8> = standard.into_iter().flat_map(u32::to_be_bytes).collect();
                color_hint |= bytes[at + 8..end - 4] != canonical;
            }
            b"eXIf" => {
                if exif.is_some() {
                    return Err(invalid("duplicate PNG EXIF"));
                }
                if size > MAX_EXIF_BYTES {
                    return Err(limit("EXIF bytes", MAX_EXIF_BYTES as u64, size as u64));
                }
                exif = Some(&bytes[at + 8..end - 4]);
            }
            b"IEND" => {
                if size != 0 || end != bytes.len() {
                    return Err(invalid("PNG has bytes after its end marker"));
                }
                ended = true;
            }
            _ => {}
        }
        at = end;
    }
    if !ended {
        return Err(invalid("PNG end marker is missing"));
    }
    Ok((profile, exif, color_hint))
}
fn orient(width: u32, height: u32, rgba: Vec<u8>, orientation: u16) -> (u32, u32, Vec<u8>) {
    if orientation == 1 {
        return (width, height, rgba);
    }
    let (out_width, out_height) = if orientation >= 5 {
        (height, width)
    } else {
        (width, height)
    };
    let mut output = vec![0; rgba.len()];
    for y in 0..height {
        for x in 0..width {
            let (dx, dy) = match orientation {
                2 => (width - 1 - x, y),
                3 => (width - 1 - x, height - 1 - y),
                4 => (x, height - 1 - y),
                5 => (y, x),
                6 => (height - 1 - y, x),
                7 => (height - 1 - y, width - 1 - x),
                8 => (y, width - 1 - x),
                _ => unreachable!(),
            };
            let source = ((u64::from(y) * u64::from(width) + u64::from(x)) * 4) as usize;
            let dest = ((u64::from(dy) * u64::from(out_width) + u64::from(dx)) * 4) as usize;
            output[dest..dest + 4].copy_from_slice(&rgba[source..source + 4]);
        }
    }
    (out_width, out_height, output)
}

/// Horizontal image placement within the page body.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageAlignment {
    /// Align to the body left edge.
    #[default]
    Left,
    /// Center within the body.
    Center,
    /// Align to the body right edge.
    Right,
}
/// An in-flow body image. It is not a text run or a table-cell object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    id: Id,
    asset: ImageAsset,
    width: Twips,
    height: Twips,
    alt_text: String,
    alignment: ImageAlignment,
}
impl Image {
    /// Constructs an image block; document validation checks display dimensions and metadata.
    #[must_use]
    pub fn new(id: Id, asset: ImageAsset, width: Twips, height: Twips) -> Self {
        Self {
            id,
            asset,
            width,
            height,
            alt_text: String::new(),
            alignment: ImageAlignment::Left,
        }
    }
    /// Sets alternative text without changing the raster.
    #[must_use]
    pub fn with_alt_text(mut self, alt_text: impl Into<String>) -> Self {
        self.alt_text = alt_text.into();
        self
    }
    /// Sets horizontal placement without changing the raster.
    #[must_use]
    pub fn with_alignment(mut self, alignment: ImageAlignment) -> Self {
        self.alignment = alignment;
        self
    }
    /// Stable block identifier.
    #[must_use]
    pub fn id(&self) -> &Id {
        &self.id
    }
    /// Immutable original and derived raster.
    #[must_use]
    pub fn asset(&self) -> &ImageAsset {
        &self.asset
    }
    /// Physical display width in twips.
    #[must_use]
    pub fn width(&self) -> Twips {
        self.width
    }
    /// Physical display height in twips.
    #[must_use]
    pub fn height(&self) -> Twips {
        self.height
    }
    /// Alternative text, preserved exactly as UTF-8.
    #[must_use]
    pub fn alt_text(&self) -> &str {
        &self.alt_text
    }
    /// Horizontal placement.
    #[must_use]
    pub fn alignment(&self) -> ImageAlignment {
        self.alignment
    }
    pub(crate) fn validate(&self, path: &str) -> Result<(), ValidationError> {
        if self.width.0 <= 0 || self.height.0 <= 0 {
            return Err(ValidationError::InvalidFormatting {
                path: path.into(),
                reason: "image display dimensions must be positive supported twips".into(),
            });
        }
        if self.alt_text.len() > MAX_IMAGE_ALT_TEXT_BYTES
            || self
                .alt_text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(ValidationError::InvalidFormatting {
                path: format!("{path}.alt_text"),
                reason: "image alternative text is limited to 8192 UTF-8 bytes; only LF and TAB controls are allowed"
                    .into(),
            });
        }
        Ok(())
    }
}

// Parse the strict structural budget before any ImageAsset full deserializer can
// allocate a raster. A Value intermediate would erase duplicate budget fields.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetDocument {
    #[serde(rename = "schema_version")]
    _version: u64,
    #[serde(rename = "id")]
    _id: serde::de::IgnoredAny,
    #[serde(rename = "metadata")]
    _metadata: serde::de::IgnoredAny,
    #[serde(rename = "styles")]
    _styles: serde::de::IgnoredAny,
    sections: Vec<BudgetSection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetSection {
    #[serde(rename = "id")]
    _id: serde::de::IgnoredAny,
    #[serde(rename = "properties")]
    _properties: Option<serde::de::IgnoredAny>,
    blocks: Vec<BudgetBlock>,
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "content",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum BudgetBlock {
    Paragraph(serde::de::IgnoredAny),
    Table(serde::de::IgnoredAny),
    Image(BudgetImage),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetImage {
    #[serde(rename = "id")]
    _id: serde::de::IgnoredAny,
    #[serde(rename = "width")]
    _width: Twips,
    #[serde(rename = "height")]
    _height: Twips,
    #[serde(rename = "alt_text")]
    _alt: String,
    #[serde(rename = "alignment")]
    _alignment: ImageAlignment,
    asset: AssetWire,
}
pub(crate) fn preflight_document(json: &str) -> Result<(), crate::DocumentError> {
    let input: BudgetDocument = serde_json::from_str(json)?;
    let mut count = 0_usize;
    let mut total_bytes = 0_usize;
    let mut total_pixels = 0_u64;
    for section in input.sections {
        for block in section.blocks {
            match block {
                BudgetBlock::Paragraph(value) | BudgetBlock::Table(value) => {
                    let _ = value;
                }
                BudgetBlock::Image(image) => {
                    count += 1;
                    if count > MAX_DOCUMENT_IMAGES {
                        return Err(limit(
                            "document count",
                            MAX_DOCUMENT_IMAGES as u64,
                            count as u64,
                        )
                        .into());
                    }
                    let bytes = decode_base64(&image.asset.bytes_base64)?;
                    total_bytes = total_bytes.saturating_add(bytes.len());
                    if total_bytes > MAX_DOCUMENT_IMAGE_BYTES {
                        return Err(limit(
                            "document bytes",
                            MAX_DOCUMENT_IMAGE_BYTES as u64,
                            total_bytes as u64,
                        )
                        .into());
                    }
                    let (format, width, height) = inspect_dimensions(&bytes)?;
                    if format != image.asset.format {
                        return Err(invalid("raster format does not match bytes").into());
                    }
                    total_pixels =
                        total_pixels.saturating_add(u64::from(width) * u64::from(height));
                    if total_pixels > MAX_DOCUMENT_IMAGE_PIXELS {
                        return Err(limit(
                            "document pixels",
                            MAX_DOCUMENT_IMAGE_PIXELS,
                            total_pixels,
                        )
                        .into());
                    }
                }
            }
        }
    }
    Ok(())
}
