//! Lay out documents and emit vector PDF.
//!
//! Build a [`DocumentSpec`] from ordinary Rust values, call [`layout_spec`] to
//! measure, break, and paginate, then `to_pdf` to serialize. Fonts and images
//! live on [`spec::Resources`]. Batch caps live on [`DocumentSpec::limits`].
//!
//! # Pipeline
//!
//! ```text
//! Resources + DocumentSpec → layout_spec → Rendered → to_pdf → PDF bytes
//! ```
//!
//! - **Assemble time** — your `if` / `for` choose which [`spec::Block`] values
//!   enter the body. Page count is unknown.
//! - **Paint time** — headers, footers, and layer marks may use
//!   [`spec::TextRun`] / [`spec::PageWhen`]; the library fills page numbers when
//!   painting each page.
//!
//! # Coordinates
//!
//! PDF points (`1/72` inch), top-left origin, **y downward** (same as krilla
//! 0.8.2 `Surface`). Lengths are branded ([`Length`] with [`Finite`] /
//! [`NonNegative`] / [`Positive`]). Default page margins are zero; set them
//! explicitly.
//!
//! # Fonts and glyphs
//!
//! Register faces on a [`FontRegistry`] and pass it to [`spec::Resources::with_fonts`].
//! Each text run names a key in that registry. A character with no cmap entry
//! fails the document ([`ErrorKind::MissingGlyph`]). There is no fallback font
//! and no `.notdef` box. v1 targets horizontal Chinese and Latin (no bidi,
//! vertical writing, kerning, ligatures, or justified lines). Bold, italic, and
//! bold italic are separate registered faces. A run that needs a face its
//! family lacks fails the document. There is no synthetic bold or slant.
//!
//! # Modules
//!
//! | Module | Contents |
//! | --- | --- |
//! | [`spec`] | Document values and [`layout_spec`] |
//! | [`geom`] | Lengths, pages, paint, paths |
//! | [`error`] | [`Error`], [`Errors`], [`ErrorKind`], [`LimitKind`] |
//! | [`limits`] | [`Limits`] caps. [`LimitKind`] is defined in [`error`] and re-exported here |
//! | [`canvas`] | Recorded drawing commands |
//! | [`fonts`] | [`FontId`], [`FontRegistry`] |
//!
//! Layout and text engines are crate-private. The PDF backend is private and
//! compiled behind feature `backend-krilla` (default). The drawing surface is
//! crate-private too. Tests observe [`CanvasCommand`] on [`RecordingCanvas`].
//!
//! # Example
//!
//! ```no_run
//! use pdfcraft_pdf::spec::{
//!     Block, DocumentInfo, FontFace, FontFamily, FontRegistry, Resources, SectionSpec,
//! };
//! use pdfcraft_pdf::{layout_spec, to_pdf, DocumentSpec, Margins, PageSize, Result};
//!
//! # fn main() -> Result<()> {
//! let font = std::fs::read("path/to/face.ttf").expect("font");
//! let rendered = layout_spec(DocumentSpec {
//!     page_size: PageSize::A4,
//!     resources: Resources::with_fonts(FontRegistry::single(FontFamily {
//!         regular: FontFace::new(font, 0),
//!         bold: None,
//!         italic: None,
//!         bold_italic: None,
//!     })),
//!     limits: pdfcraft_pdf::Limits::default(),
//!     info: DocumentInfo::default(),
//!     sections: vec![SectionSpec {
//!         margins: Margins::try_uniform(48.0)?,
//!         body: vec![Block::paragraph((), "Hello from pdfcraft-pdf.")],
//!         ..SectionSpec::default()
//!     }],
//! })?;
//! let pdf = to_pdf(rendered)?;
//! assert!(pdf.starts_with(b"%PDF"));
//! # Ok(())
//! # }
//! ```
//!
//! # Further reading
//!
//! The full how-to (blocks, tables, bands, limits, errors, testing) ships in
//! `docs/guide.md` next to this crate’s README.

#![forbid(unsafe_code)]

pub mod error;
pub mod geom;
pub mod limits;

pub use error::{
    CellIndex, CellSpanReason, ColumnWidthsReason, DecorationKind, Error, ErrorKind, Errors,
    GeometryKind, Location, Region, Result,
};
pub use geom::{
    Color, Finite, Ink, IntoFinite, IntoNonNegative, IntoPoint, IntoPositive, IntoRect, Length,
    MarginPreset, Margins, NonNegative, Orientation, PageSize, Paint, Paper, Path, PathBuilder,
    PathVerb, Pen, Point, Positive, Rect, Size, Stroke, A4, LETTER, MM_TO_PT,
};
pub use limits::{LimitKind, Limits};

pub mod canvas;
pub mod fonts;
pub mod spec;

pub use spec::{layout_spec, DocumentSpec, Numbering, Ratio, Rendered, Track};

#[cfg(feature = "test-utils")]
pub use canvas::{pt_near, PT_ABS_TOLERANCE};
pub use canvas::{
    CanvasCommand, DecodedImage, GlyphPlacement, GlyphRun, ImageFormat, ImageId, LinkTarget,
    RecordedPage, RecordingCanvas,
};
pub use fonts::{FontFace, FontFamily, FontId, FontRegistry};

pub(crate) mod text;

pub(crate) mod layout;

#[cfg(feature = "backend-krilla")]
pub(crate) mod backend;

pub(crate) fn image_registry() -> Box<dyn canvas::ImageRegistry> {
    #[cfg(feature = "backend-krilla")]
    {
        Box::new(backend::ImageTable::default())
    }
    #[cfg(not(feature = "backend-krilla"))]
    {
        Box::new(canvas::RejectingImageRegistry)
    }
}

/// Writes the pages of `rendered` as PDF bytes.
///
/// [`DocumentSpec::info`](crate::spec::DocumentSpec::info) is included. Non-empty
/// title and author become the PDF title and author. Empty strings are omitted.
/// The creator is `pdfcraft`. Producer is not set.
///
/// JPEG, GIF, and WebP pixel data that passed header checks while the document
/// was laid out can still fail here. That [`ErrorKind::Backend`] value has no
/// page. PNG pixel data fails during [`layout_spec`] instead.
///
/// # Errors
///
/// Returns backend serialization failures as [`Errors`].
#[cfg(feature = "backend-krilla")]
pub fn to_pdf(rendered: Rendered) -> Result<Vec<u8>> {
    backend::write_pdf(
        &rendered.recording,
        rendered.fonts,
        rendered.images,
        &rendered.info,
    )
    .map_err(Errors::from)
}
