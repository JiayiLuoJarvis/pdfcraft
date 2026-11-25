//! Node boxes, subject promotion, and continuation-cue slot sizes.
//!
//! Wrapped rows are measured here. Coordinates are assigned later by layout.

use crate::captions::{cont_cue_see, CueSide, ATTACHMENT_PREFIX, OVERFLOW_DETAIL_PREFIX};
use crate::content::LineKind;
use crate::content::LogicalLine;
use crate::error::{Error, ErrorCode};
use crate::font::{wrap_text, Measurer};
use crate::graph::{NodeId, NodeOrigin};
use crate::request::NodeWidth;
use crate::units::{
    node_height_limit_mm, ADAPTIVE_SUBJECT_BASE_WIDTH_MM, ADAPTIVE_TIERS_MM,
    CONT_CUE_ANCHOR_MATCH_MAX_MM, CONT_CUE_LINE_COUNT, CONT_CUE_LINE_GAP_MM, CONT_CUE_MIN_WIDTH_MM,
    CONT_CUE_PAD_MM, CONT_CUE_PAGE_DIGITS, CONT_CUE_TEXT_PT, DETAIL_FONT_PT, DETAIL_LINE_GAP_PT,
    LINE_GAP_PT, NAME_FALLBACK_FONT_PT, NAME_FONT_PT, PADDING_X_MM, PADDING_Y_MM,
    SUBJECT_MAX_WIDTH_MM, SUBJECT_NAME_FONT_PT, SUBJECT_PADDING_Y_MM, SUBJECT_WIDTH_FACTOR,
};

/// One laid-out in-box row, relative to the content top.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LaidOutTextLine {
    /// Top of the line slot, mm below the content origin.
    pub y_mm: f64,
    /// Point size used to measure `text`.
    pub font_size_pt: f64,
    /// Glyphs to draw.
    pub text: String,
}

/// Vertical layout of wrapped in-box lines. Adaptive-layout spec §3.3.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BoxTextLayout {
    /// Rows in draw order.
    pub lines: Vec<LaidOutTextLine>,
    /// Distance from content top to the bottom of the last slot.
    pub content_height_mm: f64,
}

/// Ordinary node width used when chart options leave the width unset.
#[cfg(test)]
#[must_use]
pub(crate) fn default_node_width() -> NodeWidth {
    NodeWidth::Fixed(crate::units::NODE_WIDTH_MM)
}

/// Page-subject box width from the non-subject width policy.
#[must_use]
pub(crate) fn subject_width_mm(width: NodeWidth) -> f64 {
    let base = match width {
        NodeWidth::Adaptive => ADAPTIVE_SUBJECT_BASE_WIDTH_MM,
        NodeWidth::Fixed(node_width_mm) => node_width_mm,
    };
    (base * SUBJECT_WIDTH_FACTOR).min(SUBJECT_MAX_WIDTH_MM)
}

/// One wrapped row inside a node box.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WrappedLine {
    /// Name, legal person, or detail.
    pub kind: LineKind,
    /// The slice of the source line that fits this row.
    pub text: String,
    /// Point size used to measure `text`.
    pub font_size_pt: f64,
    /// Source [`LogicalLine`] index, for lossless promote rejoin.
    pub logical_line_index: usize,
}

/// Measured box. Coordinates are not assigned here.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NodeBox {
    /// Layout identity from the graph arena (request nodes or overflow).
    pub id: NodeId,
    /// Arena origin. An overflow box is drawn dashed.
    pub origin: NodeOrigin,
    /// Box width in mm.
    pub width_mm: f64,
    /// Box height in mm.
    pub height_mm: f64,
    /// Wrapped rows, top to bottom.
    pub lines: Vec<WrappedLine>,
    /// Page subject, including a promoted annex root.
    pub is_subject: bool,
    /// Gray fill. See [`crate::content::company_fill_rec`].
    pub is_company: bool,
}

/// One node's in-box lines, before wrapping.
pub(crate) struct InboxNode<'a> {
    /// Arena id.
    pub id: NodeId,
    /// Arena origin, copied onto the box.
    pub origin: NodeOrigin,
    /// Lines from [`crate::content::inbox_lines`].
    pub lines: &'a [LogicalLine],
    /// Gray fill. See [`crate::content::company_fill`].
    pub is_company: bool,
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

fn gap_pt_for(kind: LineKind) -> f64 {
    match kind {
        LineKind::Name => LINE_GAP_PT,
        LineKind::LegalPerson | LineKind::Detail => DETAIL_LINE_GAP_PT,
    }
}

/// Vertical slots for wrapped in-box lines.
pub(crate) fn layout_box_text(
    lines: &[WrappedLine],
    measurer: &mut impl Measurer,
) -> BoxTextLayout {
    let mut y = 0.0;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        out.push(LaidOutTextLine {
            y_mm: y,
            font_size_pt: line.font_size_pt,
            text: line.text.clone(),
        });
        y += measurer.line_height_with_gap_mm(line.font_size_pt, gap_pt_for(line.kind));
    }
    BoxTextLayout {
        lines: out,
        content_height_mm: y,
    }
}

fn snap_adaptive_width_mm(needed_box_w: f64) -> f64 {
    ADAPTIVE_TIERS_MM
        .iter()
        .copied()
        .find(|tier| *tier >= needed_box_w)
        .unwrap_or(ADAPTIVE_TIERS_MM[ADAPTIVE_TIERS_MM.len() - 1])
}

fn max_digit_width_mm(measurer: &mut impl Measurer, size_pt: f64) -> Result<f64, Error> {
    let mut digit = 0.0_f64;
    for ch in ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'] {
        digit = digit.max(measurer.string_width_mm(&ch.to_string(), size_pt)?);
    }
    Ok(digit)
}

fn overflow_detail_budget_mm(measurer: &mut impl Measurer) -> Result<f64, Error> {
    let prefix = measurer.string_width_mm(OVERFLOW_DETAIL_PREFIX, NAME_FONT_PT)?;
    Ok(prefix + CONT_CUE_PAGE_DIGITS * max_digit_width_mm(measurer, NAME_FONT_PT)?)
}

fn natural_line_width_mm(
    line: &LogicalLine,
    is_subject: bool,
    measurer: &mut impl Measurer,
) -> Result<f64, Error> {
    measurer.string_width_mm(&line.text, line_size(line.kind, is_subject))
}

fn ordinary_width_mm(
    inbox: &InboxNode<'_>,
    width: NodeWidth,
    measurer: &mut impl Measurer,
) -> Result<f64, Error> {
    match width {
        NodeWidth::Fixed(node_width_mm) => Ok(node_width_mm),
        NodeWidth::Adaptive => {
            let mut natural = 0.0_f64;
            for line in inbox.lines {
                natural = natural.max(natural_line_width_mm(line, false, measurer)?);
            }
            if inbox.origin.is_overflow() {
                natural = natural.max(overflow_detail_budget_mm(measurer)?);
            }
            Ok(snap_adaptive_width_mm(natural + 2.0 * PADDING_X_MM))
        }
    }
}

fn wrap_logical_line(
    line: &LogicalLine,
    logical_index: usize,
    is_subject: bool,
    allow_name_fallback: bool,
    max_content: f64,
    measurer: &mut impl Measurer,
) -> Result<Vec<WrappedLine>, Error> {
    let size = line_size(line.kind, is_subject);
    if line.kind == LineKind::Name && allow_name_fallback {
        let at_name = wrap_text(&line.text, measurer, NAME_FONT_PT, max_content, true)?;
        if at_name.len() >= 3 {
            let at_fallback = wrap_text(
                &line.text,
                measurer,
                NAME_FALLBACK_FONT_PT,
                max_content,
                true,
            )?;
            if at_fallback.len() < at_name.len() {
                return Ok(wrapped_parts(
                    line.kind,
                    at_fallback,
                    NAME_FALLBACK_FONT_PT,
                    logical_index,
                ));
            }
        }
        return Ok(wrapped_parts(
            line.kind,
            at_name,
            NAME_FONT_PT,
            logical_index,
        ));
    }
    let parts = wrap_text(&line.text, measurer, size, max_content, true)?;
    Ok(wrapped_parts(line.kind, parts, size, logical_index))
}

fn wrapped_parts(
    kind: LineKind,
    parts: Vec<String>,
    font_size_pt: f64,
    logical_line_index: usize,
) -> Vec<WrappedLine> {
    parts
        .into_iter()
        .map(|text| WrappedLine {
            kind,
            text,
            font_size_pt,
            logical_line_index,
        })
        .collect()
}

fn box_height_mm(lines: &[WrappedLine], is_subject: bool, measurer: &mut impl Measurer) -> f64 {
    let pad_y = if is_subject {
        SUBJECT_PADDING_Y_MM
    } else {
        PADDING_Y_MM
    };
    (2.0 * pad_y + layout_box_text(lines, measurer).content_height_mm)
        .max(min_node_height_mm(is_subject, measurer))
}

/// Build one box per inbox node.
///
/// The subject is wider, up to `SUBJECT_MAX_WIDTH_MM` (120 mm). Height is
/// compared with the portrait node-height limit of `printable_height_mm`.
///
/// # Errors
///
/// Returns [`ErrorCode::NodeTooTall`], or a measurer error.
pub(crate) fn build_node_boxes(
    nodes: &[InboxNode<'_>],
    measurer: &mut impl Measurer,
    width: NodeWidth,
    subject: NodeId,
    printable_height_mm: f64,
) -> Result<Vec<NodeBox>, Error> {
    let limit = node_height_limit_mm(printable_height_mm);
    let mut boxes = Vec::with_capacity(nodes.len());
    for inbox in nodes {
        let is_subject = inbox.id == subject;
        let width_mm = if is_subject {
            subject_width_mm(width)
        } else {
            ordinary_width_mm(inbox, width, measurer)?
        };
        let max_content = width_mm - 2.0 * PADDING_X_MM;
        let allow_name_fallback =
            matches!(width, NodeWidth::Adaptive) && !is_subject && !inbox.origin.is_overflow();
        let mut lines = Vec::new();
        for (logical_index, line) in inbox.lines.iter().enumerate() {
            lines.extend(wrap_logical_line(
                line,
                logical_index,
                is_subject,
                allow_name_fallback,
                max_content,
                measurer,
            )?);
        }
        let height = box_height_mm(&lines, is_subject, measurer);
        if height > limit {
            return Err(Error::new(
                ErrorCode::NodeTooTall,
                format!(
                    "node {} height_mm={height} exceeds max_node_height={limit}",
                    inbox.origin
                ),
            )
            .with_optional_subject(inbox.origin.error_subject()));
        }
        boxes.push(NodeBox {
            id: inbox.id,
            origin: inbox.origin.clone(),
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
pub(crate) fn overflow_box(
    node_id: NodeId,
    origin: NodeOrigin,
    name: &str,
    measurer: &mut impl Measurer,
    width: NodeWidth,
) -> Result<NodeBox, Error> {
    let width_mm = match width {
        NodeWidth::Fixed(node_width_mm) => node_width_mm,
        NodeWidth::Adaptive => {
            let name_w = measurer.string_width_mm(name, NAME_FONT_PT)?;
            let needed = name_w.max(overflow_detail_budget_mm(measurer)?) + 2.0 * PADDING_X_MM;
            snap_adaptive_width_mm(needed)
        }
    };
    let max_content = width_mm - 2.0 * PADDING_X_MM;
    let mut lines = wrap_logical_line(
        &LogicalLine {
            kind: LineKind::Name,
            text: name.to_owned(),
        },
        0,
        false,
        false,
        max_content,
        measurer,
    )?;
    lines.push(WrappedLine {
        kind: LineKind::Name,
        text: String::new(),
        font_size_pt: NAME_FONT_PT,
        logical_line_index: 1,
    });
    Ok(NodeBox {
        id: node_id,
        origin,
        width_mm,
        height_mm: box_height_mm(&lines, false, measurer),
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
pub(crate) fn promote_to_page_subject(
    box_: &NodeBox,
    width: NodeWidth,
    measurer: &mut impl Measurer,
) -> Result<NodeBox, Error> {
    let width_mm = subject_width_mm(width);
    let max_content = width_mm - 2.0 * PADDING_X_MM;
    let segments = join_segments(&box_.lines);
    let mut lines = Vec::new();
    for (logical_index, (kind, text, size)) in segments.into_iter().enumerate() {
        for part in wrap_text(&text, measurer, size, max_content, true)? {
            lines.push(WrappedLine {
                kind,
                text: part,
                font_size_pt: size,
                logical_line_index: logical_index,
            });
        }
    }
    Ok(NodeBox {
        id: box_.id,
        origin: box_.origin.clone(),
        width_mm,
        height_mm: box_height_mm(&lines, true, measurer),
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
    let mut buf_logical: Option<usize> = None;
    for line in lines {
        let out_size = if line.kind == LineKind::Name {
            SUBJECT_NAME_FONT_PT
        } else {
            line.font_size_pt
        };
        match buf_logical {
            None => {
                buf.clone_from(&line.text);
                buf_size = out_size;
                buf_kind = Some(line.kind);
                buf_logical = Some(line.logical_line_index);
            }
            Some(index) if index == line.logical_line_index => buf.push_str(&line.text),
            Some(_) => {
                if let Some(kind) = buf_kind {
                    segments.push((kind, std::mem::take(&mut buf), buf_size));
                }
                buf.clone_from(&line.text);
                buf_size = out_size;
                buf_kind = Some(line.kind);
                buf_logical = Some(line.logical_line_index);
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
pub(crate) fn cont_cue_height_mm(measurer: &mut impl Measurer) -> f64 {
    CONT_CUE_LINE_COUNT * measurer.line_height_mm(CONT_CUE_TEXT_PT)
        + (CONT_CUE_LINE_COUNT - 1.0) * CONT_CUE_LINE_GAP_MM
        + 2.0 * CONT_CUE_PAD_MM
}

/// A continuation cue's measured slot, before placement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CueAnchor {
    /// Node the cue points at.
    pub anchor_id: NodeId,
    /// Shareholder or investment side.
    pub side: CueSide,
    /// Slot width in mm.
    pub width_mm: f64,
    /// Slot height in mm.
    pub height_mm: f64,
}

/// Measure cue slots. An overflow box cannot be an anchor.
///
/// # Errors
///
/// Returns [`ErrorCode::LayoutFailed`] when an anchor is missing or is an overflow box,
/// and measurer errors from the cue text.
pub(crate) fn sized_cue_anchors(
    anchors: &[(NodeId, CueSide)],
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
        let Some(box_) = boxes.iter().find(|box_| box_.id == anchor_id) else {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!("continuation cue anchor is not on the page: node #{anchor_id}"),
            ));
        };
        if box_.origin.is_overflow() {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!(
                    "continuation cue cannot anchor an overflow box: {}",
                    box_.origin
                ),
            ));
        }
        let see = cont_cue_see(side);
        let text = measurer
            .string_width_mm(see, CONT_CUE_TEXT_PT)?
            .max(attachment);
        let width = (text + 2.0 * CONT_CUE_PAD_MM)
            .max(box_.width_mm.min(CONT_CUE_ANCHOR_MATCH_MAX_MM))
            .max(CONT_CUE_MIN_WIDTH_MM);
        if width <= 0.0 || height <= 0.0 {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                "cue anchor size must be positive",
            ));
        }
        sized.push(CueAnchor {
            anchor_id,
            side,
            width_mm: width,
            height_mm: height,
        });
    }
    Ok(sized)
}

#[cfg(test)]
#[path = "boxes_tests.rs"]
mod boxes_tests;
