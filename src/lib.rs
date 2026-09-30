//! Quire: PDF document layout on top of a sealed krilla backend.
//!
//! Public surface grows only after items in `docs/提案书.md` are confirmed.
//! This crate currently exports nothing beyond the module tree placeholders.

#![forbid(unsafe_code)]

pub mod error;
pub mod geom;
pub mod limits;

pub mod canvas;
pub mod document;
pub mod fonts;

pub mod text;

pub mod layout;

#[cfg(feature = "backend-krilla")]
pub(crate) mod backend;
