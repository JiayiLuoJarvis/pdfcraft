//! Read `AcroForm` widgets from an existing PDF.
//!
//! [`list_fields`] returns one value per widget annotation. The crate does not
//! depend on `quire-pdf` and does not use `krilla`.

#![forbid(unsafe_code)]

mod error;
mod fields;
mod inspect;

pub use error::{Error, ErrorKind};
pub use fields::{
    FieldInfo, FieldType, FieldValue, FillOptions, FontBytes, MissingName, DEFAULT_FONT_SIZE,
    FONT_SIZE_STEP, LINE_HEIGHT_FACTOR, MIN_FONT_SIZE, PAD_BOTTOM, PAD_TOP, PAD_X,
};
pub use inspect::list_fields;

/// A forms result.
pub type Result<T> = std::result::Result<T, Error>;
