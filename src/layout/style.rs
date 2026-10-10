//! Crate-internal paragraph style for flow layout.

#![cfg_attr(not(test), allow(dead_code))]

use crate::fonts::FontId;
use crate::geom::{Color, Length, NonNegative, Positive};

/// Horizontal alignment within the current frame width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextAlign {
    Start,
    Center,
    End,
}

/// Resolved style used by paragraph flow.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Style {
    pub(crate) font: FontId,
    pub(crate) font_size: Length<Positive>,
    pub(crate) line_height_factor: f32,
    pub(crate) color: Color,
    pub(crate) paragraph_spacing: Length<NonNegative>,
    pub(crate) first_line_indent: Length<NonNegative>,
    pub(crate) align: TextAlign,
}

impl Style {
    pub(crate) fn new(
        font: FontId,
        font_size: Length<Positive>,
        line_height_factor: f32,
        color: Color,
        paragraph_spacing: Length<NonNegative>,
        align: TextAlign,
    ) -> crate::error::Result<Self> {
        if !line_height_factor.is_finite() || line_height_factor <= 0.0 {
            return Err(crate::error::Error::InvalidGeometry);
        }
        Ok(Self {
            font,
            font_size,
            line_height_factor,
            color,
            paragraph_spacing,
            first_line_indent: NonNegative::ZERO,
            align,
        })
    }
}
