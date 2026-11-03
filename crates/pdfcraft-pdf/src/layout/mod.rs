//! Layout engine: split/draw flowables, check nested content, paginate, decorate.

pub(crate) mod anchor;
pub(crate) mod atomic;
pub(crate) mod canvas;
pub(crate) mod column_width;
pub(crate) mod columns;
pub(crate) mod decorate;
#[cfg(test)]
mod decorate_tests;
pub(crate) mod flow;
pub(crate) mod frame;
pub(crate) mod hline;
pub(crate) mod image;
pub(crate) mod justify;
pub(crate) mod line_run;
pub(crate) mod list;
pub(crate) mod nesting;
#[cfg(test)]
mod node_tests;
pub(crate) mod paginate;
pub(crate) mod paragraph;
pub(crate) mod style;
pub(crate) mod table;
#[cfg(test)]
mod test_util;
