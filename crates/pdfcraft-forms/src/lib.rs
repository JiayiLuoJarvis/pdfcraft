//! Fill `AcroForm` widgets in an existing PDF and flatten the result.
//!
//! [`list_fields`] returns one value per widget annotation. [`fill_and_flatten`]
//! draws text, checks, and choices in PDF user space, then removes every widget
//! annotation and the catalog `AcroForm` entry. The origin is the bottom-left
//! of the page and y increases upward.
//!
//! This crate does not depend on `pdfcraft-pdf` and does not use `krilla`.

#![forbid(unsafe_code)]

mod content;
mod error;
mod fields;
mod fill;
mod font;
mod inspect;
mod layout;

pub use error::{ArgumentReason, Error, ErrorKind, FlattenReason, FontReason};
pub use fields::{
    AbsoluteLine, FieldInfo, FieldType, FieldValue, FillOptions, FontBytes, HAlign, MissingName,
    VAlign,
};
pub use fill::fill_and_flatten;
pub use inspect::list_fields;

/// A forms result.
pub type Result<T> = std::result::Result<T, Error>;
