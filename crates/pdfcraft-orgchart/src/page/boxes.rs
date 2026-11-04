//! Node boxes, subject promotion, and continuation-cue slot sizes.
//!
//! Wrapped rows are measured here. Coordinates are assigned later by layout.

use crate::captions::{cont_cue_see, CueSide, ATTACHMENT_PREFIX};
use crate::content::LineKind;
use crate::content::LogicalLine;
use crate::error::{Error, ErrorCode};
use crate::font::{wrap_text, Measurer};
use crate::graph::NodeId;
use crate::graph::OVERFLOW_ID_PREFIX;
use crate::units::{
    node_height_limit_mm, CONT_CUE_LINE_COUNT, CONT_CUE_LINE_GAP_MM, CONT_CUE_MIN_WIDTH_MM,
    CONT_CUE_PAD_MM, CONT_CUE_PAGE_DIGITS, CONT_CUE_TEXT_PT, DETAIL_FONT_PT, NAME_FONT_PT,
    PADDING_X_MM, PADDING_Y_MM, SUBJECT_NAME_FONT_PT, SUBJECT_PADDING_Y_MM, SUBJECT_WIDTH_FACTOR,
};

/// Ordinary node width used when chart options leave the width unset.
#[cfg(test)]
#[must_use]
pub(crate) fn default_node_width_mm() -> f64 {
    crate::units::NODE_WIDTH_MM
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
}

/// Measured box. Coordinates are not assigned here.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NodeBox {
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
    /// Gray fill. See [`crate::content::company_fill_rec`].
    pub is_company: bool,
}

/// One node's in-box lines, before wrapping.
pub(crate) struct InboxNode<'a> {
    /// Arena id.
    pub id: NodeId,
    /// Protocol id for error messages and the box.
    pub protocol_id: &'a str,
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

/// Build one box per inbox node.
///
/// The subject is wider, up to 80 mm. Height is compared with the portrait
/// node-height limit of `printable_height_mm`.
///
/// # Errors
///
/// Returns [`ErrorCode::NodeTooTall`], or a measurer error.
pub(crate) fn build_node_boxes(
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
            (node_width_mm * SUBJECT_WIDTH_FACTOR).min(crate::request::MAX_NODE_WIDTH_MM)
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
pub(crate) fn overflow_box(
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
pub(crate) fn promote_to_page_subject(
    box_: &NodeBox,
    node_width_mm: f64,
    measurer: &mut impl Measurer,
) -> Result<NodeBox, Error> {
    let width_mm = (node_width_mm * SUBJECT_WIDTH_FACTOR).min(crate::request::MAX_NODE_WIDTH_MM);
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
pub(crate) fn cont_cue_height_mm(measurer: &mut impl Measurer) -> f64 {
    CONT_CUE_LINE_COUNT * measurer.line_height_mm(CONT_CUE_TEXT_PT)
        + (CONT_CUE_LINE_COUNT - 1.0) * CONT_CUE_LINE_GAP_MM
        + 2.0 * CONT_CUE_PAD_MM
}

/// A continuation cue's measured slot, before placement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CueAnchor {
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
pub(crate) fn sized_cue_anchors(
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
#[path = "boxes_tests.rs"]
mod boxes_tests;
