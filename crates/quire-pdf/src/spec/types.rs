//! Public value types used by [`super::DocumentSpec`].

use crate::canvas::{DocumentInfo, ImageRegistry, RecordedPage, RecordingCanvas};
use crate::error::{ColumnWidthsReason, Error, ErrorKind, Fallible};
use crate::fonts::{FontId, FontStore};
use crate::geom::{Length, Positive};
use crate::layout::column_width::{ColumnWidth, Ratio as LayoutRatio};
use crate::layout::list::MarkerKind;
use crate::layout::style::{Style, TextAlign};

/// Marker numbering of an ordered list. Each label ends with `.`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Numbering {
    /// 1. 2. 3.
    Decimal,
    /// A. B. C., up to Z.
    UpperAlpha,
    /// a. b. c., up to z.
    LowerAlpha,
    /// I. II. III., up to 3999.
    UpperRoman,
    /// i. ii. iii., up to 3999.
    LowerRoman,
}

pub(crate) fn numbering_kind(numbering: Numbering) -> MarkerKind {
    match numbering {
        Numbering::Decimal => MarkerKind::Decimal,
        Numbering::UpperAlpha => MarkerKind::UpperAlpha,
        Numbering::LowerAlpha => MarkerKind::LowerAlpha,
        Numbering::UpperRoman => MarkerKind::UpperRoman,
        Numbering::LowerRoman => MarkerKind::LowerRoman,
    }
}

/// Positive finite weight for a [`Track::Ratio`] column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ratio(f32);

impl Ratio {
    /// Builds a weight.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::ColumnWidths`] with [`ColumnWidthsReason::NonFiniteRatio`] or
    /// [`ColumnWidthsReason::NonPositiveRatio`].
    pub fn new(weight: f32) -> std::result::Result<Self, Error> {
        LayoutRatio::new(weight)?;
        Ok(Self(weight))
    }

    pub(crate) fn get(self) -> f32 {
        self.0
    }
}

/// A table or column track width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Track {
    /// A positive length.
    Fixed(Length<Positive>),
    /// A positive finite weight from [`Ratio::new`].
    Ratio(Ratio),
    /// Remaining width after Fixed tracks. At most one Auto per set of tracks.
    Auto,
}

impl Track {
    pub(crate) fn into_width(self) -> Fallible<ColumnWidth> {
        match self {
            Self::Fixed(width) => Ok(ColumnWidth::Fixed(width)),
            Self::Ratio(weight) => Ok(ColumnWidth::Ratio(LayoutRatio::new(weight.get())?)),
            Self::Auto => Ok(ColumnWidth::Auto),
        }
    }
}

pub(crate) fn reject_auto_with_ratio(widths: &[ColumnWidth]) -> Fallible<()> {
    let auto = widths.iter().any(|w| matches!(w, ColumnWidth::Auto));
    let ratio = widths.iter().any(|w| matches!(w, ColumnWidth::Ratio(_)));
    if auto && ratio {
        Err(Error::from(ErrorKind::ColumnWidths {
            reason: ColumnWidthsReason::AutoWithRatio,
        }))
    } else {
        Ok(())
    }
}

pub(crate) fn default_style() -> Style {
    Style {
        font: FontId::from_raw(0),
        font_size: Positive::pt(12.0).expect("12 pt is positive"),
        line_height_factor: 1.2,
        color: crate::geom::Color::rgb(0, 0, 0),
        paragraph_spacing: crate::geom::NonNegative::ZERO,
        first_line_indent: crate::geom::NonNegative::ZERO,
        align: TextAlign::Start,
    }
}

/// A laid-out document.
#[must_use]
pub struct Rendered {
    pub(crate) recording: RecordingCanvas,
    #[cfg_attr(
        not(feature = "backend-krilla"),
        allow(dead_code, reason = "only to_pdf reads fonts when the backend is on")
    )]
    pub(crate) fonts: FontStore,
    #[cfg_attr(
        not(feature = "backend-krilla"),
        allow(dead_code, reason = "only to_pdf reads images when the backend is on")
    )]
    pub(crate) images: Box<dyn ImageRegistry>,
    #[cfg_attr(
        not(feature = "backend-krilla"),
        allow(dead_code, reason = "only to_pdf reads info when the backend is on")
    )]
    pub(crate) info: DocumentInfo,
}

impl Rendered {
    /// Every page's drawing commands, in paint order.
    #[must_use]
    pub fn pages(&self) -> Vec<RecordedPage> {
        self.recording.pages()
    }
}
