//! Fill `AcroForm` widgets in an existing PDF and flatten the result.
//!
//! [`Form::open`] reads the PDF and lists one [`FieldInfo`] per widget.
//! [`Fill::new`] checks a [`FillRequest`] and lays out text. [`Fill::to_pdf`]
//! draws the values, then removes every widget annotation and the catalog
//! `AcroForm` entry. An empty request still returns a PDF with the form removed.
//!
//! This crate does not depend on `pdfcraft-pdf` and does not use `krilla`.
//!
//! # Pipeline
//!
//! ```text
//! PDF bytes → Form::open → FieldInfo…
//! Form + FillRequest + optional Font → Fill::new → Fill::to_pdf → PDF bytes
//! ```
//!
//! # Coordinates
//!
//! Drawing uses PDF user space. The origin is the bottom-left of the page and
//! **y increases upward**. [`AbsoluteLine`] coordinates use the same space.
//!
//! This differs from `pdfcraft-pdf` (top-left origin, y downward).
//!
//! # Fonts
//!
//! Pass a TrueType face with a `glyf` table as [`Font`]. [`Font::new`] reads one
//! `sfnt`. [`Font::from_collection`] reads one face from a `ttcf`. Checkbox
//! marks do not need a font. Empty text strings do not either. Successful text
//! embed uses a `glyf` subset that keeps the original glyph ids. OTF remains
//! out of scope.
//!
//! # Modules
//!
//! Public items are re-exported at the crate root. Layout and PDF open helpers
//! stay private.
//!
//! | Item | Role |
//! | --- | --- |
//! | [`Form`] | Open a PDF and list widgets |
//! | [`Fill`] | Check a request and write PDF bytes |
//! | [`Font`] | One TrueType face |
//! | [`FillRequest`] / [`FieldFill`] / [`FieldRule`] | Values, rules, and placement |
//! | [`Error`] / [`ErrorKind`] | Failures |
//!
//! # Example
//!
//! ```no_run
//! use std::collections::BTreeMap;
//!
//! use pdfcraft_forms::{
//!     FieldFill, FieldRule, FieldStyle, Fill, FillRequest, FillStyle, Font, Form, HAlign, Result,
//! };
//!
//! # fn main() -> Result<()> {
//! let pdf = std::fs::read("form.pdf").expect("pdf");
//! let form = Form::open(&pdf)?;
//! let _widgets = form.fields();
//!
//! let font_bytes = std::fs::read("face.ttf").expect("font");
//! let font = Font::new(font_bytes)?;
//! let name = FieldFill {
//!     style: FieldStyle {
//!         h_align: Some(HAlign::Center),
//!         ..FieldStyle::default()
//!     },
//!     ..FieldFill::text("Ada")
//! };
//! let request = FillRequest {
//!     style: FillStyle {
//!         font_size: 14.0,
//!         ..FillStyle::default()
//!     },
//!     rules: BTreeMap::from([("employee_name".to_owned(), FieldRule::new(2.0))]),
//!     ..FillRequest::new(BTreeMap::from([
//!         ("employee_name".to_owned(), name),
//!         ("agree".to_owned(), FieldFill::check(true)),
//!     ]))
//! };
//! let filled = Fill::new(&form, request, Some(&font))?.to_pdf()?;
//! assert!(filled.starts_with(b"%PDF"));
//! # Ok(())
//! # }
//! ```
//!
//! Stripping the form without drawing a value:
//!
//! ```no_run
//! use pdfcraft_forms::{Fill, FillRequest, Form, Result};
//!
//! # fn main() -> Result<()> {
//! let pdf = std::fs::read("form.pdf").expect("pdf");
//! let form = Form::open(&pdf)?;
//! let filled = Fill::new(&form, FillRequest::default(), None)?.to_pdf()?;
//! assert!(filled.starts_with(b"%PDF"));
//! # Ok(())
//! # }
//! ```
//!
//! A longer walkthrough ships in `docs/guide.md` next to this crate’s README.
//!
//! [`FieldValue::CheckBox`]`(true)` draws the check. `false` draws nothing.
//! A [`FieldStyle`] on a checkbox is checked and does not change the check mark.

#![forbid(unsafe_code)]

mod error;
mod fill;
mod font;
mod geom;
mod layout;
mod paint;
mod pdf;
mod request;
mod sfnt;
mod subset;

#[doc(inline)]
pub use error::{ArgumentReason, Error, ErrorKind, FlattenReason, FontReason};
#[doc(inline)]
pub use fill::{Fill, Form};
#[doc(inline)]
pub use font::Font;
#[doc(inline)]
pub use request::{
    AbsoluteLine, FieldFill, FieldInfo, FieldRule, FieldStyle, FieldType, FieldValue, FillRequest,
    FillStyle, HAlign, MissingName, StrokeStyle, VAlign,
};

/// A forms result.
pub type Result<T> = std::result::Result<T, Error>;
