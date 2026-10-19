//! Crate-internal paragraph and glyph styles for flow layout.

use crate::fonts::{EmOffset, FontId};
use crate::geom::{Color, Finite, Length, NonNegative, Positive};

/// Horizontal alignment within the current frame width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextAlign {
    Start,
    Center,
    End,
    /// Paragraph remainder stretch. Non-paragraph placers treat as Start.
    Justify,
}

/// Paragraph frame. Glyphs read [`GlyphStyle`], not these fields.
///
/// `font` is the face for a line with no glyphs: the first text run's resolved
/// face, including that run's bold or italic. `None` when the node has no text
/// run, so those lines read no font. `font_size` sizes em blanks and fontless
/// lines. `color` is the default blank color.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Style {
    pub(crate) font: Option<FontId>,
    pub(crate) font_size: Length<Positive>,
    pub(crate) line_height_factor: f32,
    pub(crate) color: Color,
    pub(crate) paragraph_spacing: Length<NonNegative>,
    pub(crate) first_line_indent: Length<NonNegative>,
    pub(crate) align: TextAlign,
}

/// One run's underline or strikethrough at its font size.
///
/// Thickness is [`Decoration::THICKNESS`]. The centerline is below the baseline, y down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Decoration {
    centerline: Length<Finite>,
}

impl Decoration {
    /// Stroke thickness for every underline and strikethrough.
    pub(crate) const THICKNESS: Length<Positive> = Length::from_trusted_pt(1.0);

    /// Centerline below the baseline, y down: `-offset.em() * size + THICKNESS / 2`.
    ///
    /// `None` when that value is not finite.
    pub(crate) fn at(offset: EmOffset, size: Length<Positive>) -> Option<Self> {
        let center = -offset.em() * size.as_pt() + Self::THICKNESS.as_pt() / 2.0;
        let centerline = Finite::pt(center).ok()?;
        Some(Self { centerline })
    }

    /// Stroke centerline below the baseline, y down.
    pub(crate) fn centerline(self) -> Length<Finite> {
        self.centerline
    }
}

/// Fully resolved run style. Built by the spec lowering cascade only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GlyphStyle {
    pub(crate) font: FontId,
    pub(crate) font_size: Length<Positive>,
    pub(crate) color: Color,
    /// Extra advance after each cluster whose next line atom is also text.
    pub(crate) tracking: Length<Finite>,
    pub(crate) underline: Option<Decoration>,
    pub(crate) strikethrough: Option<Decoration>,
}

#[cfg(test)]
impl GlyphStyle {
    pub(crate) fn plain(style: &Style) -> Self {
        Self {
            font: style.font.expect("test style has a face"),
            font_size: style.font_size,
            color: style.color,
            tracking: Finite::ZERO,
            underline: None,
            strikethrough: None,
        }
    }
}

impl Style {
    #[cfg(test)]
    pub(crate) fn new(
        font: FontId,
        font_size: Length<Positive>,
        line_height_factor: f32,
        color: Color,
        paragraph_spacing: Length<NonNegative>,
        align: TextAlign,
    ) -> crate::error::Fallible<Self> {
        if !line_height_factor.is_finite() || line_height_factor <= 0.0 {
            return Err(crate::error::GeometryKind::LineHeightFactor {
                factor: line_height_factor,
            }
            .into());
        }
        Ok(Self {
            font: Some(font),
            font_size,
            line_height_factor,
            color,
            paragraph_spacing,
            first_line_indent: NonNegative::ZERO,
            align,
        })
    }
}
