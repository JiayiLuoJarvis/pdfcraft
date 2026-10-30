//! Post-process existing PDF bytes.
//!
//! This crate writes `/Info`, encrypts with AES-256 (revision 6), merges page
//! lists, and runs ordered [`Pipeline`] steps. Input and output are PDF bytes
//! in memory.
//!
//! It does not depend on `pdfcraft-pdf`, `pdfcraft-forms`, or
//! `pdfcraft-orgchart`. Watermark, input limits, HTTP mapping, and Web
//! finalize orchestration are out of scope for this stack.
//!
//! # Processors
//!
//! | Type | Role |
//! | --- | --- |
//! | [`SetMetadata`] | Write selected `/Info` fields (Creator/Producer allowed) |
//! | [`Encrypt`] | AES-256 encrypt with permission presets |
//! | [`StampAndEncrypt`] | Metadata then encrypt in one open/save |
//! | [`merge_pdfs`] | Concatenate pages from several PDFs |
//! | [`Pipeline`] | Run ordered steps on one buffer |
//!
//! # Errors
//!
//! Failures are [`Error`] with an [`ErrorKind`]. Dependency errors are available
//! through [`std::error::Error::source`].

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

mod combo;
mod encryption;
mod error;
mod merge;
mod metadata;
mod pages;
mod pipeline;
mod split;
mod thumbnail;

#[doc(inline)]
pub use combo::StampAndEncrypt;
#[doc(inline)]
pub use encryption::{Encrypt, EncryptionConfig, EncryptionPreset};
#[doc(inline)]
pub use error::{Error, ErrorKind};
#[doc(inline)]
pub use merge::merge_pdfs;
#[doc(inline)]
pub use metadata::SetMetadata;
#[doc(inline)]
pub use pipeline::{Pipeline, Step};
#[doc(inline)]
pub use split::split_pdfs;
#[doc(inline)]
pub use thumbnail::{first_page_jpeg, ThumbnailOptions};

/// Crate result alias.
pub type Result<T> = std::result::Result<T, Error>;
