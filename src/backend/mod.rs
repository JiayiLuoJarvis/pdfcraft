//! Private PDF backend.
//!
//! Only this module tree may `use krilla::…`. Compiled under feature
//! `backend-krilla`.
//!
//! `KrillaCanvas` has no production caller until `to_pdf` lands; unit tests
//! are the only users this knife.

#![allow(dead_code)]

pub(crate) mod convert;
pub(crate) mod emit;
pub(crate) mod krilla_canvas;

#[allow(unused_imports)] // re-export for the future to_pdf caller and backend tests
pub(crate) use krilla_canvas::KrillaCanvas;
