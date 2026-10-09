//! Private PDF backend.
//!
//! Only this module tree may `use krilla::…`. Compiled under feature
//! `backend-krilla`.

#![allow(dead_code)]

pub(crate) mod convert;
pub(crate) mod emit;
pub(crate) mod images;
pub(crate) mod krilla_canvas;

pub(crate) use images::ImageTable;
pub(crate) use krilla_canvas::KrillaCanvas;
