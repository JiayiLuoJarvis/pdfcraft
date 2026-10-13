//! Equity ownership charts.
//!
//! [`validate_request`] checks a [`ChartRequest`], including a shareholder fold
//! when `top_n` is set. [`document_title`] builds the string a caller can store
//! as PDF document info. This crate does not draw a PDF yet, and it does not
//! depend on `quire-pdf` or `quire-forms`.

#![forbid(unsafe_code)]

#[doc(hidden)]
pub mod captions;
#[doc(hidden)]
pub mod display;
mod error;
mod graph;
mod model;
mod prepare;

pub use error::{Error, ErrorCode};
pub use model::{
    document_title, ChartOptions, ChartRequest, Edge, Node, NodeType, MAX_MARGIN_MM, MAX_NODES,
    MAX_NODE_WIDTH_MM, MIN_NODE_WIDTH_MM,
};

/// Check the chart protocol, including a `top_n` fold of shareholder children.
///
/// Calls the prepare step and drops the folded request. Does not open a font.
/// Does not validate the folded graph. A fold that adds a synthetic node can
/// succeed here even when the folded node list is over `max_nodes`.
///
/// # Errors
///
/// Returns [`Error`] when the original request is illegal, or when the fold
/// itself is illegal. [`Error::code`] is the stable value.
///
/// # Examples
///
/// ```
/// use quire_orgchart::{validate_request, ChartRequest, Edge, Node};
///
/// let request = ChartRequest::new(
///     "root",
///     vec![Node::new("holder", "甲"), Node::new("root", "主体")],
///     vec![Edge::new("holder", "root")],
/// );
/// validate_request(&request)?;
/// # Ok::<(), quire_orgchart::Error>(())
/// ```
pub fn validate_request(request: &ChartRequest) -> Result<(), Error> {
    prepare::prepare_request(request).map(|_| ())
}
