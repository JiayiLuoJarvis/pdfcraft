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
//! # Fonts
//!
//! Pass owned face bytes as [`FontBytes`]. Paths are not accepted. A font
//! collection is rejected. Measurement and drawing use the same face.
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
//! | [`FontBytes`] | Owned TrueType / OpenType face bytes |
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
mod cues;
mod display;
mod error;
mod family;
mod graph;
mod ids;
mod layout;
mod measure;
mod model;
mod page;
mod paging;
mod prepare;
mod render;
#[cfg(test)]
mod test_font;
mod units;

pub use error::{Error, ErrorCode};
pub use model::{
    document_title, ChartOptions, ChartRequest, Edge, FontBytes, Node, NodeType, MAX_MARGIN_MM,
    MAX_NODES, MAX_NODE_WIDTH_MM, MIN_NODE_WIDTH_MM,
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
/// use pdfcraft_orgchart::{validate_request, ChartRequest, Edge, Node};
///
/// let request = ChartRequest::new(
///     "root",
///     vec![Node::new("holder", "甲"), Node::new("root", "主体")],
///     vec![Edge::new("holder", "root")],
/// );
/// validate_request(&request)?;
/// # Ok::<(), pdfcraft_orgchart::Error>(())
/// ```
pub fn validate_request(request: &ChartRequest) -> Result<(), Error> {
    prepare::prepare_request(request).map(|_| ())
}

/// Draw `request` with `font`.
///
/// The request is folded, then validated again, so a fold that grows past
/// `max_nodes` fails here. Page headers come from the chart role, not from
/// [`ChartOptions::title`]. The PDF has no document info dictionary from this
/// crate.
///
/// # Errors
///
/// Returns [`Error`] when the chart, the font, or the layout is illegal.
/// [`Error::code`] is the stable value.
pub fn generate_pdf(request: &ChartRequest, font: &FontBytes) -> Result<Vec<u8>, Error> {
    let prepared = prepare::prepare_request(request)?;
    let graph = graph::validate(&prepared)?;
    let shown = display::display_of(&graph);
    let margin = graph.options.margin_mm.unwrap_or(units::MARGIN_MM);
    let node_width = graph.options.node_width_mm.unwrap_or(units::NODE_WIDTH_MM);
    let geometry = units::PageGeometry::portrait(margin);
    let mut metrics = measure::FontMetrics::open(font)?;
    let subject_protocol = graph.subject_protocol_id.clone();
    let boxes = {
        let inboxes: Vec<measure::InboxNode<'_>> = graph
            .nodes
            .iter()
            .zip(shown.nodes())
            .map(|(node, content)| measure::InboxNode {
                id: node.id,
                protocol_id: &node.protocol_id,
                lines: &content.lines,
                is_company: display::company_fill_rec(node),
            })
            .collect();
        measure::build_node_boxes(
            &inboxes,
            &mut metrics,
            node_width,
            &subject_protocol,
            geometry.printable_height_mm(),
        )?
    };
    let pages = paging::paginate(graph, &boxes, geometry, &mut metrics)?;
    for page in &pages {
        measure::Measurer::ensure_glyphs(&mut metrics, &page.main_title)?;
        measure::Measurer::ensure_glyphs(&mut metrics, &page.sub_title)?;
        for fact in &page.ratio_facts {
            measure::Measurer::ensure_glyphs(&mut metrics, &fact.text)?;
        }
    }
    let count = pages.len();
    for index in 1..=count {
        measure::Measurer::ensure_glyphs(&mut metrics, &captions::footer(index, count))?;
    }
    render::render(&pages, &mut metrics)
}
