//! Public document construction surface (Rust DSL).
//!
//! [`document`] runs the caller's closure once to assemble the body flow.
//! Header, footer, behind, and front closures run again on every page after
//! pagination, so only they see page numbers.

// Submodules share the former monolith import surface via `use super::*`.
#![allow(clippy::wildcard_imports)]

use std::num::{NonZeroU16, NonZeroU32};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::canvas::{Canvas, DecodedImage, ImageFormat, LinkTarget, RecordedPage, RecordingCanvas};
use crate::error::{Error, Result};
use crate::fonts::{FontId, FontSource, FontStore};
use crate::geom::{
    Color, Ink, IntoNonNegative, IntoPoint, IntoPositive, IntoRect, Length, Margins, NonNegative,
    PageSize, Path, Pen, Point, Positive, Size,
};
use crate::layout::columns::{default_column_gap, ColumnWidth, ColumnsNode, Ratio};
use crate::layout::decorate::{
    BandContent, Decorations, LayerContent, LayerItem, LayerText, PageInfo,
};
use crate::layout::flow::{FlowNode, KeepTogetherNode, LayoutCtx, SpacerNode};
use crate::layout::hline::HLineNode;
use crate::layout::image::ImageNode;
use crate::layout::list::{ListItem, ListKind, ListNode, ListSpec, MarkerKind};
use crate::layout::paginate::{paginate_sections, PaginateConfig, SectionJob};
use crate::layout::paragraph::{BlankWidth, Face, InlineBlank, InlineItem, TextNode, TextRun};
use crate::layout::style::{Style, TextAlign};
use crate::layout::table as layout_table;
use crate::layout::table::{TableNode, TableSpec};

#[cfg(feature = "backend-krilla")]
use crate::backend::ImageTable;

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
mod section;
mod style_props;
mod table;
mod token;

#[cfg(test)]
mod tests;

pub use band::Band;
pub use columns::Columns;
pub use document_ty::{document, Document, Rendered};
pub use flow::Flow;
pub use flow_writer::{FlowWriter, IntoFlowItem, PlacedImage};
pub use hline::HLine;
pub use image::{Image, ImageBox};
pub use layer::{Layer, Shapes, TextAt};
pub use list::{List, Numbering};
pub use page::Page;
pub use paragraph::{Blank, Paragraph};
pub use section::Section;
pub use style_props::TextProps;
pub use table::{Border, Cell, Table, Track};

pub(crate) use band::band_content;
pub(crate) use columns::reject_auto_with_ratio;
pub(crate) use document_ty::default_style;
pub(crate) use flow::{assemble_list, lower_paragraph, numbering_kind};
pub(crate) use layer::layer_content;
pub(crate) use section::{BandHook, LayerHook, Sections};
pub(crate) use token::DocToken;
