//! Glyph advances and node-box heights.
//!
//! Width and a later draw step both go through [`FontMetrics::glyph_run`].
//! Line height uses the hhea ascender and descender, plus a 2 pt gap.

use std::collections::HashMap;
use std::sync::Arc;

use read_fonts::TableProvider;
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

use crate::captions::{cont_cue_see, CueSide, ATTACHMENT_PREFIX};
use crate::display::LineKind;
use crate::display::LogicalLine;
use crate::error::{Error, ErrorCode};
use crate::graph::OVERFLOW_ID_PREFIX;
use crate::ids::NodeId;
use crate::model::FontBytes;
use crate::units::{
    node_height_limit_mm, pt_to_mm, CONT_CUE_LINE_COUNT, CONT_CUE_LINE_GAP_MM,
    CONT_CUE_MIN_WIDTH_MM, CONT_CUE_PAD_MM, CONT_CUE_PAGE_DIGITS, CONT_CUE_TEXT_PT, DETAIL_FONT_PT,
    LINE_GAP_PT, NAME_FONT_PT, PADDING_X_MM, PADDING_Y_MM, SUBJECT_NAME_FONT_PT,
    SUBJECT_PADDING_Y_MM, SUBJECT_WIDTH_FACTOR,
};

/// Ordinary node width used when chart options leave the width unset.
#[cfg(test)]
#[must_use]
pub fn default_node_width_mm() -> f64 {
    crate::units::NODE_WIDTH_MM
}

/// Width, line height, and glyph coverage for wrapping.
pub trait Measurer {
    /// Width of `text` at `size_pt`, in mm.
    ///
    /// # Errors
    ///
    /// A real font returns [`ErrorCode::MissingGlyph`] when `text` needs a glyph it lacks.
    fn string_width_mm(&mut self, text: &str, size_pt: f64) -> Result<f64, Error>;

    /// Line slot height at `size_pt`, in mm.
    fn line_height_mm(&mut self, size_pt: f64) -> f64;

    /// Reject text that the font cannot draw.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::MissingGlyph`] when a character has no glyph.
    fn ensure_glyphs(&mut self, text: &str) -> Result<(), Error>;
}

/// One glyph produced by the same charmap and advance path drawing will use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphUse {
    /// Glyph id in the face.
    pub glyph_id: u32,
    /// Advance in font units, before size scaling.
    pub advance_units: f32,
}

/// One face, opened from memory.
#[derive(Debug, Clone)]
pub struct FontMetrics {
    bytes: Arc<Vec<u8>>,
    upem: f64,
    ascent: f64,
    descent: f64,
    glyphs: HashMap<char, GlyphUse>,
}

impl FontMetrics {
    /// Open one TrueType or OpenType face.
    ///
    /// A `ttcf` collection is [`ErrorCode::FontError`]. So is a face whose hhea
    /// ascent is not positive or whose hhea descent is not negative.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::FontError`] when the bytes are not one usable face.
    pub fn open(font: &FontBytes) -> Result<Self, Error> {
        if font.data.starts_with(b"ttcf") {
            return Err(Error::new(ErrorCode::FontError, "TTC not supported"));
        }
        if font.data.is_empty() {
            return Err(Error::new(ErrorCode::FontError, "font bytes are empty"));
        }
        let face = parse_face(&font.data)?;
        let head = face.head().map_err(|error| {
            Error::new(
                ErrorCode::FontError,
                format!("failed to read head: {error}"),
            )
        })?;
        let hhea = face.hhea().map_err(|error| {
            Error::new(
                ErrorCode::FontError,
                format!("failed to read hhea: {error}"),
            )
        })?;
        let upem = f64::from(head.units_per_em());
        let ascent = f64::from(hhea.ascender().to_i16());
        let descent = f64::from(hhea.descender().to_i16());
        if !(upem.is_finite() && upem > 0.0) {
            return Err(Error::new(
                ErrorCode::FontError,
                format!("head.unitsPerEm={upem} must be finite and > 0"),
            ));
        }
        if !(ascent.is_finite() && ascent > 0.0 && descent.is_finite() && descent < 0.0) {
            return Err(Error::new(
                ErrorCode::FontError,
                format!(
                    "hhea ascent={ascent} descent={descent} must be finite, ascent > 0 and descent < 0"
                ),
            ));
        }
        let bytes = Arc::clone(&font.data);
        Ok(Self {
            bytes,
            upem,
            ascent,
            descent: -descent,
            glyphs: HashMap::new(),
        })
    }

    /// Charmap hits and unscaled advances for `text`.
    ///
    /// Uncached characters parse the face once. Later calls reuse those glyph ids.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::MissingGlyph`] or [`ErrorCode::FontError`].
    pub fn glyph_run(&mut self, text: &str) -> Result<Vec<GlyphUse>, Error> {
        if text.chars().any(|ch| !self.glyphs.contains_key(&ch)) {
            let face = parse_face(&self.bytes)?;
            let charmap = face.charmap();
            let glyph_metrics = face.glyph_metrics(Size::unscaled(), LocationRef::default());
            for ch in text.chars() {
                if self.glyphs.contains_key(&ch) {
                    continue;
                }
                let Some(gid) = charmap.map(ch) else {
                    return Err(missing_glyph(text));
                };
                let advance_units = glyph_metrics.advance_width(gid).ok_or_else(|| {
                    Error::new(
                        ErrorCode::FontError,
                        format!("no advance for glyph {}", gid.to_u32()),
                    )
                })?;
                self.glyphs.insert(
                    ch,
                    GlyphUse {
                        glyph_id: gid.to_u32(),
                        advance_units,
                    },
                );
            }
        }
        Ok(text.chars().map(|ch| self.glyphs[&ch]).collect())
    }

    /// Sum of advances at `size_pt`, in mm.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::MissingGlyph`] when `text` is not covered.
    pub fn string_width_mm(&mut self, text: &str, size_pt: f64) -> Result<f64, Error> {
        let units: f64 = self
            .glyph_run(text)?
            .iter()
            .map(|glyph| f64::from(glyph.advance_units))
            .sum();
        Ok(pt_to_mm(units / self.upem * size_pt))
    }

    /// Distance from the baseline to the ascender, in mm.
    #[must_use]
    pub fn ascent_mm(&self, size_pt: f64) -> f64 {
        pt_to_mm(self.ascent / self.upem * size_pt)
    }

    /// Face bytes passed to krilla. The same buffer skrifa measured.
    #[must_use]
    pub(crate) fn bytes(&self) -> &Arc<Vec<u8>> {
        &self.bytes
    }

    /// Units per em from `head`.
    #[must_use]
    pub(crate) fn units_per_em(&self) -> f64 {
        self.upem
    }

    /// Ascender plus descender, scaled, plus a 2 pt gap.
    #[must_use]
    pub fn line_height_mm(&self, size_pt: f64) -> f64 {
        pt_to_mm((self.ascent + self.descent) / self.upem * size_pt + LINE_GAP_PT)
    }
}

impl Measurer for FontMetrics {
    fn string_width_mm(&mut self, text: &str, size_pt: f64) -> Result<f64, Error> {
        Self::string_width_mm(self, text, size_pt)
    }

    fn line_height_mm(&mut self, size_pt: f64) -> f64 {
        Self::line_height_mm(self, size_pt)
    }

    fn ensure_glyphs(&mut self, text: &str) -> Result<(), Error> {
        self.glyph_run(text).map(|_| ())
    }
}

fn parse_face(bytes: &[u8]) -> Result<FontRef<'_>, Error> {
    FontRef::from_index(bytes, 0).map_err(|error| {
        Error::new(
            ErrorCode::FontError,
            format!("failed to load font: {error}"),
        )
    })
}

fn missing_glyph(text: &str) -> Error {
    let prefix: String = text.chars().take(20).collect();
    Error::new(ErrorCode::MissingGlyph, format!("missing glyph: {prefix}"))
}

/// One wrapped row inside a node box.
#[derive(Debug, Clone, PartialEq)]
pub struct WrappedLine {
    /// Name, legal person, or detail.
    pub kind: LineKind,
    /// The slice of the source line that fits this row.
    pub text: String,
    /// Point size used to measure `text`.
    pub font_size_pt: f64,
}

/// Measured box. Coordinates are not assigned here.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeBox {
    /// Layout identity from the graph arena (request nodes or overflow).
    pub id: NodeId,
    /// Caller id, or the synthetic `__overflow__:` / `__other__:` id.
    pub protocol_id: String,
    /// Dashed stroke. Set only for a layout overflow box.
    pub is_overflow: bool,
    /// Box width in mm.
    pub width_mm: f64,
    /// Box height in mm.
    pub height_mm: f64,
    /// Wrapped rows, top to bottom.
    pub lines: Vec<WrappedLine>,
    /// Page subject, including a promoted annex root.
    pub is_subject: bool,
    /// Gray fill. See [`crate::display::company_fill_rec`].
    pub is_company: bool,
}

/// One node's in-box lines, before wrapping.
pub struct InboxNode<'a> {
    /// Arena id.
    pub id: NodeId,
    /// Protocol id for error messages and the box.
    pub protocol_id: &'a str,
    /// Lines from [`crate::display::inbox_lines`].
    pub lines: &'a [LogicalLine],
    /// Gray fill. See [`crate::display::company_fill`].
    pub is_company: bool,
}

/// Wrap `text`. ASCII runs stay together until a space. Other characters break one by one.
///
/// `allow_wrap == false` returns the whole string even when it is wider than `max_width_mm`.
///
/// # Errors
///
/// Returns the measurer's glyph or font error.
pub fn wrap_text(
    text: &str,
    measurer: &mut impl Measurer,
    size_pt: f64,
    max_width_mm: f64,
    allow_wrap: bool,
) -> Result<Vec<String>, Error> {
    measurer.ensure_glyphs(text)?;
    if !allow_wrap || measurer.string_width_mm(text, size_pt)? <= max_width_mm {
        return Ok(vec![text.to_owned()]);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    while index < chars.len() {
        let (mut candidate, mut take) = next_chunk(&chars, index);
        if measurer.string_width_mm(&candidate, size_pt)? > max_width_mm {
            candidate = chars[index].to_string();
            take = 1;
        }
        let combined = format!("{current}{candidate}");
        if measurer.string_width_mm(&combined, size_pt)? <= max_width_mm {
            current = combined;
            index += take;
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        } else {
            out.push(candidate);
            index += take;
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    Ok(out)
}

fn next_chunk(chars: &[char], index: usize) -> (String, usize) {
    if chars[index].is_ascii() {
        let mut end = chars.len();
        for (offset, ch) in chars[index..].iter().enumerate() {
            if !ch.is_ascii() || *ch == ' ' {
                end = index + offset;
                break;
            }
        }
        if end < chars.len() && chars[end] == ' ' {
            end += 1;
        }
        let end = end.max(index + 1);
        (chars[index..end].iter().collect(), end - index)
    } else {
        (chars[index].to_string(), 1)
    }
}

fn min_node_height_mm(is_subject: bool, measurer: &mut impl Measurer) -> f64 {
    if is_subject {
        2.0 * SUBJECT_PADDING_Y_MM + 2.0 * measurer.line_height_mm(SUBJECT_NAME_FONT_PT)
    } else {
        2.0 * PADDING_Y_MM + 2.0 * measurer.line_height_mm(NAME_FONT_PT)
    }
}

fn name_size(is_subject: bool) -> f64 {
    if is_subject {
        SUBJECT_NAME_FONT_PT
    } else {
        NAME_FONT_PT
    }
}

fn line_size(kind: LineKind, is_subject: bool) -> f64 {
    match kind {
        LineKind::Name => name_size(is_subject),
        LineKind::LegalPerson | LineKind::Detail => DETAIL_FONT_PT,
    }
}

/// Build one box per inbox node.
///
/// The subject is wider, up to 80 mm. Height is compared with the portrait
/// node-height limit of `printable_height_mm`.
///
/// # Errors
///
/// Returns [`ErrorCode::NodeTooTall`], or a measurer error.
pub fn build_node_boxes(
    nodes: &[InboxNode<'_>],
    measurer: &mut impl Measurer,
    node_width_mm: f64,
    subject_id: &str,
    printable_height_mm: f64,
) -> Result<Vec<NodeBox>, Error> {
    let limit = node_height_limit_mm(printable_height_mm);
    let mut boxes = Vec::with_capacity(nodes.len());
    for inbox in nodes {
        let is_subject = inbox.protocol_id == subject_id;
        let width_mm = if is_subject {
            (node_width_mm * SUBJECT_WIDTH_FACTOR).min(crate::model::MAX_NODE_WIDTH_MM)
        } else {
            node_width_mm
        };
        let pad_y = if is_subject {
            SUBJECT_PADDING_Y_MM
        } else {
            PADDING_Y_MM
        };
        let max_content = width_mm - 2.0 * PADDING_X_MM;
        let mut lines = Vec::new();
        let mut height = 2.0 * pad_y;
        for line in inbox.lines {
            let size = line_size(line.kind, is_subject);
            for part in wrap_text(&line.text, measurer, size, max_content, true)? {
                height += measurer.line_height_mm(size);
                lines.push(WrappedLine {
                    kind: line.kind,
                    text: part,
                    font_size_pt: size,
                });
            }
        }
        height = height.max(min_node_height_mm(is_subject, measurer));
        if height > limit {
            return Err(Error::new(
                ErrorCode::NodeTooTall,
                format!(
                    "node {} height_mm={height} exceeds max_node_height={limit}",
                    inbox.protocol_id
                ),
            ));
        }
        boxes.push(NodeBox {
            id: inbox.id,
            protocol_id: inbox.protocol_id.to_owned(),
            is_overflow: false,
            width_mm,
            height_mm: height,
            lines,
            is_subject,
            is_company: inbox.is_company,
        });
    }
    Ok(boxes)
}

/// Synthetic overflow box. The second row is an empty name slot, filled after page numbers exist.
///
/// # Errors
///
/// Returns a measurer error.
pub fn overflow_box(
    node_id: NodeId,
    protocol_id: &str,
    name: &str,
    measurer: &mut impl Measurer,
    node_width_mm: f64,
) -> Result<NodeBox, Error> {
    let max_content = node_width_mm - 2.0 * PADDING_X_MM;
    let mut lines = Vec::new();
    for part in wrap_text(name, measurer, NAME_FONT_PT, max_content, true)? {
        lines.push(WrappedLine {
            kind: LineKind::Name,
            text: part,
            font_size_pt: NAME_FONT_PT,
        });
    }
    lines.push(WrappedLine {
        kind: LineKind::Name,
        text: String::new(),
        font_size_pt: NAME_FONT_PT,
    });
    let line_h = measurer.line_height_mm(NAME_FONT_PT);
    Ok(NodeBox {
        id: node_id,
        protocol_id: protocol_id.to_owned(),
        is_overflow: true,
        width_mm: node_width_mm,
        height_mm: (2.0 * PADDING_Y_MM
            + f64::from(u32::try_from(lines.len()).unwrap_or(u32::MAX)) * line_h)
            .max(min_node_height_mm(false, measurer)),
        lines,
        is_subject: false,
        is_company: true,
    })
}

/// Widen a box into page-subject chrome and rewrap its joined text.
///
/// Name rows use the subject size. Detail and legal-person rows keep their size.
/// This does not repeat the portrait height check.
///
/// # Errors
///
/// Returns a measurer error.
pub fn promote_to_page_subject(
    box_: &NodeBox,
    node_width_mm: f64,
    measurer: &mut impl Measurer,
) -> Result<NodeBox, Error> {
    let width_mm = (node_width_mm * SUBJECT_WIDTH_FACTOR).min(crate::model::MAX_NODE_WIDTH_MM);
    let max_content = width_mm - 2.0 * PADDING_X_MM;
    let segments = join_segments(&box_.lines);
    let mut lines = Vec::new();
    let mut height = 2.0 * SUBJECT_PADDING_Y_MM;
    for (kind, text, size) in segments {
        for part in wrap_text(&text, measurer, size, max_content, true)? {
            height += measurer.line_height_mm(size);
            lines.push(WrappedLine {
                kind,
                text: part,
                font_size_pt: size,
            });
        }
    }
    Ok(NodeBox {
        id: box_.id,
        protocol_id: box_.protocol_id.clone(),
        is_overflow: box_.is_overflow,
        width_mm,
        height_mm: height.max(min_node_height_mm(true, measurer)),
        lines,
        is_subject: true,
        is_company: box_.is_company,
    })
}

fn join_segments(lines: &[WrappedLine]) -> Vec<(LineKind, String, f64)> {
    let mut segments = Vec::new();
    let mut buf = String::new();
    let mut buf_size = SUBJECT_NAME_FONT_PT;
    let mut buf_kind: Option<LineKind> = None;
    for line in lines {
        let out_size = if line.kind == LineKind::Name {
            SUBJECT_NAME_FONT_PT
        } else {
            line.font_size_pt
        };
        match buf_kind {
            None => {
                buf.clone_from(&line.text);
                buf_size = out_size;
                buf_kind = Some(line.kind);
            }
            Some(kind) if kind == line.kind => buf.push_str(&line.text),
            Some(kind) => {
                segments.push((kind, std::mem::take(&mut buf), buf_size));
                buf.clone_from(&line.text);
                buf_size = out_size;
                buf_kind = Some(line.kind);
            }
        }
    }
    if let Some(kind) = buf_kind {
        segments.push((kind, buf, buf_size));
    }
    segments
}

/// Continuation-cue box height. Two lines, one gap, padding on both ends.
#[must_use]
pub fn cont_cue_height_mm(measurer: &mut impl Measurer) -> f64 {
    CONT_CUE_LINE_COUNT * measurer.line_height_mm(CONT_CUE_TEXT_PT)
        + (CONT_CUE_LINE_COUNT - 1.0) * CONT_CUE_LINE_GAP_MM
        + 2.0 * CONT_CUE_PAD_MM
}

/// A continuation cue's measured slot, before placement.
#[derive(Debug, Clone, PartialEq)]
pub struct CueAnchor {
    /// Node the cue points at.
    pub anchor_id: String,
    /// Shareholder or investment side.
    pub side: CueSide,
    /// Slot width in mm.
    pub width_mm: f64,
    /// Slot height in mm.
    pub height_mm: f64,
}

/// Measure cue slots. An overflow id cannot be an anchor.
///
/// # Errors
///
/// Returns [`ErrorCode::LayoutFailed`] when an anchor is missing or is an overflow id,
/// and measurer errors from the cue text.
pub fn sized_cue_anchors(
    anchors: &[(&str, CueSide)],
    boxes: &[NodeBox],
    measurer: &mut impl Measurer,
) -> Result<Vec<CueAnchor>, Error> {
    let height = cont_cue_height_mm(measurer);
    let mut digit = 0.0_f64;
    for ch in ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'] {
        digit = digit.max(measurer.string_width_mm(&ch.to_string(), CONT_CUE_TEXT_PT)?);
    }
    let attachment = measurer.string_width_mm(ATTACHMENT_PREFIX, CONT_CUE_TEXT_PT)?
        + CONT_CUE_PAGE_DIGITS * digit;
    let mut sized = Vec::with_capacity(anchors.len());
    for &(anchor_id, side) in anchors {
        if anchor_id.starts_with(OVERFLOW_ID_PREFIX) {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!("continuation cue cannot anchor an overflow id: {anchor_id}"),
            ));
        }
        let Some(box_) = boxes.iter().find(|box_| box_.protocol_id == anchor_id) else {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!("continuation cue anchor is not on the page: {anchor_id}"),
            ));
        };
        let see = cont_cue_see(side);
        let text = measurer
            .string_width_mm(see, CONT_CUE_TEXT_PT)?
            .max(attachment);
        let width = (text + 2.0 * CONT_CUE_PAD_MM)
            .max(box_.width_mm)
            .max(CONT_CUE_MIN_WIDTH_MM);
        if width <= 0.0 || height <= 0.0 {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                "cue anchor size must be positive",
            ));
        }
        sized.push(CueAnchor {
            anchor_id: anchor_id.to_owned(),
            side,
            width_mm: width,
            height_mm: height,
        });
    }
    Ok(sized)
}

#[cfg(test)]
#[path = "measure_tests.rs"]
mod measure_tests;
