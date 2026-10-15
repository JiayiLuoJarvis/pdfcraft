//! Layout engine: split/draw flowables, check nested content, paginate, decorate.

pub mod atomic;
pub mod canvas;
pub mod column_width;
pub mod columns;
pub mod decorate;
#[cfg(test)]
mod decorate_tests;
pub mod flow;
pub mod frame;
pub mod hline;
pub mod image;
pub mod line_run;
pub mod list;
pub mod nesting;
#[cfg(test)]
mod node_tests;
pub mod paginate;
pub mod paragraph;
pub mod style;
pub mod table;
#[cfg(test)]
mod test_util;
