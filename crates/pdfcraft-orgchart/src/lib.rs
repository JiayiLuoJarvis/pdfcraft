//! Equity and outbound-investment charts.
//!
//! Build a [`ChartRequest`] (subject, nodes, holding edges, options). Call
//! [`validate_request`] to check the protocol, [`document_title`] for a string
//! you can store as PDF document info, and [`generate_pdf`] to draw the chart.
//! Shareholders sit above the subject. Outbound investments sit below.
//!
//! This crate does not depend on `pdfcraft-pdf` or `pdfcraft-forms`.
//!
//! # Pipeline
//!
//! ```text
//! ChartRequest + FontBytes → validate_request (optional)
//!                         → generate_pdf → PDF bytes
//! ChartRequest → document_title → String (caller stores PDF info)
//! ```
//!
//! [`generate_pdf`] folds (`top_n`), validates again, measures with skrifa,
//! paginates, and draws with krilla. It does **not** write a document info
//! dictionary. Use [`document_title`] yourself if you need `/Title`.
//!
//! # Drawing
//!
//! Ratio labels sit 1.2 mm to the right of a stub that hugs the box. A node
//! with no hugging stub is unlabeled, and so is the page subject. Shareholder
//! labels sit just below the box. Investment labels sit just above it, using
//! the descender plus 0.4 mm rather than a full line height. Node and
//! continuation-box strokes are black. Edges and continuation leaders are
//! 0.2 gray. The main title starts at the page margin. The subtitle is 1.5 mm
//! below that line. A header line that does not fit ends with `…`.
//!
//! # Fonts
//!
//! Pass face bytes as [`FontBytes`]. [`FontBytes::new`] wraps owned bytes.
//! [`FontBytes::from_shared`] reuses an `Arc<Vec<u8>>`. Paths are not accepted.
//! A font collection is rejected. Measurement and drawing use the same face.
//!
//! # Modules
//!
//! Public items are re-exported at the crate root. Layout and paging stay
//! private.
//!
//! | Item | Role |
//! | --- | --- |
//! | [`ChartRequest`] / [`ChartOptions`] | Chart values and caps |
//! | [`Node`] / [`Edge`] / [`NodeType`] | Parties and holding edges |
//! | [`FontBytes`] | Shared TrueType / OpenType face bytes |
//! | [`validate_request`] | Protocol check (no font open) |
//! | [`document_title`] | Title string for caller-owned PDF info |
//! | [`generate_pdf`] | Draw chart → PDF bytes |
//! | [`Error`] / [`ErrorCode`] | Stable failure codes |
//!
//! # Example
//!
//! ```no_run
//! use pdfcraft_orgchart::{
//!     document_title, generate_pdf, validate_request, ChartRequest, Edge, FontBytes, Node,
//! };
//!
//! # fn main() -> Result<(), pdfcraft_orgchart::Error> {
//! let font = FontBytes::new(std::fs::read("face.ttf").expect("font"));
//! let request = ChartRequest::new(
//!     "root",
//!     vec![
//!         Node::new("holder", "甲股东"),
//!         Node::new("root", "主体公司"),
//!         Node::new("invest", "乙投资"),
//!     ],
//!     vec![Edge::new("holder", "root"), Edge::new("root", "invest")],
//! );
//! validate_request(&request)?;
//! let _title = document_title(&request)?;
//! let pdf = generate_pdf(&request, &font)?;
//! assert!(pdf.starts_with(b"%PDF"));
//! # Ok(())
//! # }
//! ```
//!
//! A longer walkthrough ships in `docs/guide.md` next to this crate’s README.

#![forbid(unsafe_code)]

mod captions;
mod chart;
mod content;
mod error;
mod font;
mod graph;
mod layout;
mod page;
mod paginate;
mod render;
mod request;
#[cfg(test)]
mod test_font;
mod units;

#[doc(inline)]
pub use chart::{generate_pdf, validate_request};
#[doc(inline)]
pub use error::{Error, ErrorCode};
#[doc(inline)]
pub use font::FontBytes;
#[doc(inline)]
pub use request::{
    document_title, ChartOptions, ChartRequest, Edge, Node, NodeType, MAX_MARGIN_MM, MAX_NODES,
    MAX_NODE_WIDTH_MM, MIN_NODE_WIDTH_MM,
};
