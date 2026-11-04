//! Equity and outbound-investment charts.
//!
//! Build a [`ChartRequest`], compile it with [`Chart::new`], and draw with
//! [`Chart::to_pdf`]. Shareholders sit above the subject. Outbound investments
//! sit below.
//!
//! This crate does not depend on `pdfcraft-pdf` or `pdfcraft-forms`.
//!
//! # Pipeline
//!
//! ```text
//! ChartRequest → Chart::new → Chart
//! Font::new
//! Chart::to_pdf(&Font) → PDF bytes
//! Chart::title → String (caller stores PDF info)
//! ```
//!
//! [`Chart::new`] checks the protocol, folds `top_n`, and checks the folded
//! chart again. It does not open a font. [`Chart::to_pdf`] measures with
//! skrifa, paginates, and draws with krilla. It does **not** write a document
//! info dictionary. Use [`Chart::title`] if you need `/Title`.
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
//! [`Font::new`] parses one face. [`Font::from_shared`] reuses an
//! `Arc<Vec<u8>>`. Paths are not accepted. A font collection is rejected.
//! Measurement and drawing use the same face. The same [`Font`] can draw more
//! than one chart.
//!
//! # Modules
//!
//! Public items are re-exported at the crate root. Layout and paging stay
//! private.
//!
//! | Item | Role |
//! | --- | --- |
//! | [`ChartRequest`] / [`ChartStyle`] | Chart values, fold limit, and size caps |
//! | [`Node`] / [`Edge`] / [`PaidIn`] / [`NodeType`] | Parties, holding edges, and paid-in capital |
//! | [`Font`] | One parsed TrueType / OpenType face |
//! | [`Chart`] | Compiled chart. [`Chart::new`], [`Chart::to_pdf`], [`Chart::title`] |
//! | [`Error`] / [`ErrorCode`] / [`ErrorSubject`] | Stable failure codes and what they point at |
//!
//! # Example
//!
//! ```no_run
//! use pdfcraft_orgchart::{Chart, ChartRequest, Edge, Font, Node};
//!
//! # fn main() -> Result<(), pdfcraft_orgchart::Error> {
//! let font = Font::new(std::fs::read("face.ttf").expect("font"))?;
//! let request = ChartRequest::new(
//!     "root",
//!     vec![
//!         Node::new("holder", "甲股东"),
//!         Node::new("root", "主体公司"),
//!         Node::new("invest", "乙投资"),
//!     ],
//!     vec![Edge::new("holder", "root"), Edge::new("root", "invest")],
//! );
//! let chart = Chart::new(request)?;
//! let _title = chart.title();
//! let pdf = chart.to_pdf(&font)?;
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
pub use chart::Chart;
#[doc(inline)]
pub use error::{Error, ErrorCode, ErrorSubject};
#[doc(inline)]
pub use font::Font;
#[doc(inline)]
pub use request::{ChartRequest, ChartStyle, Edge, Node, NodeType, PaidIn};
