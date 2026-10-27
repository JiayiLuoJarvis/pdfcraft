//! Fill `AcroForm` widgets in an existing PDF and flatten the result.
//!
//! [`list_fields`] returns one value per widget annotation. [`fill_and_flatten`]
//! draws text, checks, and choices, then removes every widget annotation and
//! the catalog `AcroForm` entry. An empty field map still returns a PDF with
//! the form removed.
//!
//! This crate does not depend on `pdfcraft-pdf` and does not use `krilla`.
//!
//! # Pipeline
//!
//! ```text
//! PDF bytes → list_fields → FieldInfo…
//! PDF bytes + HashMap<name, FieldValue> + FillOptions → fill_and_flatten → PDF bytes
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
//! Pass a TrueType face with a `glyf` table on [`FillOptions::font`] as
//! [`FontBytes`]. Checkbox marks do not need a font. Empty text strings do not
//! either. `FontBytes.index` selects one face in a TrueType collection before
//! embed. Successful text embed uses a `glyf` subset that keeps the original
//! glyph ids. OTF remains out of scope.
//!
//! # Modules
//!
//! Public items are re-exported at the crate root. Layout and PDF open helpers
//! stay private.
//!
//! | Item | Role |
//! | --- | --- |
//! | [`list_fields`] | One [`FieldInfo`] per widget |
//! | [`fill_and_flatten`] | Draw values and strip the form |
//! | [`FieldValue`] / [`FillOptions`] / [`FontBytes`] | Fill inputs |
//! | [`HAlign`] / [`VAlign`] / [`MissingName`] / [`AbsoluteLine`] | Placement and strokes |
//! | [`Error`] / [`ErrorKind`] | Failures |
//!
//! # Example
//!
//! ```no_run
//! use std::collections::HashMap;
//!
//! use pdfcraft_forms::{
//!     fill_and_flatten, list_fields, FieldValue, FillOptions, FontBytes, Result,
//! };
//!
//! # fn main() -> Result<()> {
//! let pdf = std::fs::read("form.pdf").expect("pdf");
//! let _widgets = list_fields(&pdf)?;
//!
//! let font = std::fs::read("face.ttf").expect("font");
//! let mut fields = HashMap::new();
//! fields.insert("employee_name".into(), FieldValue::Text("Ada".into()));
//! fields.insert("agree".into(), FieldValue::CheckBox(true)); // checked
//!
//! let filled = fill_and_flatten(
//!     &pdf,
//!     &fields,
//!     &FillOptions {
//!         font: Some(FontBytes {
//!             bytes: &font,
//!             index: 0,
//!         }),
//!         ..FillOptions::default()
//!     },
//! )?;
//! assert!(filled.starts_with(b"%PDF"));
//! # Ok(())
//! # }
//! ```
//!
//! A longer walkthrough ships in `docs/guide.md` next to this crate’s README.

#![forbid(unsafe_code)]

mod content;
mod error;
mod fields;
mod fill;
mod font;
mod inspect;
mod layout;
mod subset;

#[doc(inline)]
pub use error::{ArgumentReason, Error, ErrorKind, FlattenReason, FontReason};
#[doc(inline)]
pub use fields::{
    AbsoluteLine, FieldInfo, FieldType, FieldValue, FillOptions, FontBytes, HAlign, MissingName,
    VAlign,
};
#[doc(inline)]
pub use fill::fill_and_flatten;
#[doc(inline)]
pub use inspect::list_fields;

/// A forms result.
pub type Result<T> = std::result::Result<T, Error>;
