//! Crate-private lowering from values to flow nodes.
//!
//! Callers build a [`crate::DocumentSpec`] and pass it to [`crate::layout_spec`].
//! Page-time header, footer, and layer closures stay inside this module.

// Submodules share the former monolith import surface via `use super::*`.
#![allow(clippy::wildcard_imports)]

use std::num::{NonZeroU16, NonZeroU32};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::canvas::{
    DecodedImage, ImageFormat, ImageRegistry, LinkTarget, RecordedPage, RecordingCanvas,
    RejectingImageRegistry,
};
use crate::error::{ColumnWidthsReason, Error, ErrorBag, ErrorKind, Fallible, GeometryKind};
use crate::fonts::{FontId, FontStore};
use crate::geom::{
    Color, Ink, IntoNonNegative, IntoPoint, IntoPositive, IntoRect, Length, Margins, NonNegative,
    PageSize, Path, Pen, Point, Positive, Size,
};
use crate::layout::columns::{default_column_gap, ColumnWidth, ColumnsNode, Ratio as LayoutRatio};
use crate::layout::decorate::{LayerItem, LayerText, PageInfo};
use crate::layout::flow::{FlowNode, SpacerNode};
use crate::layout::hline::HLineNode;
use crate::layout::image::ImageNode;
use crate::layout::list::{ListItem, ListKind, ListNode, ListSpec, MarkerKind};
use crate::layout::paragraph::{BlankWidth, Face, InlineBlank, InlineItem, TextNode, TextRun};
use crate::layout::style::{Style, TextAlign};
use crate::layout::table as layout_table;
use crate::layout::table::{TableNode, TableSpec};
use crate::limits::Limits;

mod band;
mod columns;
mod document_ty;
mod flow;
mod flow_writer;
mod hline;
mod image;
mod layer;
mod list;
mod page;
mod paragraph;
mod table;
mod token;

#[cfg(test)]
mod tests;

pub use document_ty::{Document, Rendered};
pub use image::Image;
pub use list::Numbering;
pub use table::{Ratio, Track};

pub(crate) use band::Band;
pub(crate) use columns::Columns;
pub(crate) use flow::Flow;
pub(crate) use flow_writer::{IntoFlowItem, PlacedImage, Spacer};
pub(crate) use hline::HLine;
pub(crate) use image::ImageBox;
pub(crate) use layer::Layer;
pub(crate) use list::{List, ListBlock};
pub(crate) use page::Page;
pub(crate) use paragraph::Paragraph;
pub(crate) use table::{Cell, Table};

pub(crate) use columns::reject_auto_with_ratio;
pub(crate) use document_ty::default_style;
pub(crate) use flow::{lower_paragraph, numbering_kind};
pub(crate) use token::DocToken;
