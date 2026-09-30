//! Private PDF backend.
//!
//! Only this module may `use krilla::...`. Compiled under feature `backend-krilla`
//! once that dependency is introduced.

pub(crate) mod convert;
pub(crate) mod krilla_canvas;
