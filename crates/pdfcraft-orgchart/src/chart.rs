//! The public pipeline: protocol check and PDF drawing.

use crate::captions;
use crate::content;
use crate::error::Error;
use crate::font::{FontBytes, FontMetrics, Measurer};
use crate::graph;
use crate::page::{build_node_boxes, InboxNode};
use crate::paginate;
use crate::render;
use crate::request::ChartRequest;
use crate::units;

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
    graph::prepare_request(request).map(|_| ())
}

/// Draw `request` with `font`.
///
/// The request is folded, then validated again, so a fold that grows past
/// `max_nodes` fails here. Page headers come from the chart role, not from
/// [`ChartOptions::title`](crate::ChartOptions::title). The PDF has no document
/// info dictionary from this crate.
///
/// # Errors
///
/// Returns [`Error`] when the chart, the font, or the layout is illegal.
/// [`Error::code`] is the stable value.
pub fn generate_pdf(request: &ChartRequest, font: &FontBytes) -> Result<Vec<u8>, Error> {
    let prepared = graph::prepare_request(request)?;
    let graph = graph::validate(&prepared)?;
    let shown = content::display_of(&graph);
    let margin = graph.options.margin_mm.unwrap_or(units::MARGIN_MM);
    let node_width = graph.options.node_width_mm.unwrap_or(units::NODE_WIDTH_MM);
    let geometry = units::PageGeometry::portrait(margin);
    let mut metrics = FontMetrics::open(font)?;
    let subject_protocol = graph.subject_protocol_id.clone();
    let boxes = {
        let inboxes: Vec<InboxNode<'_>> = graph
            .nodes
            .iter()
            .zip(shown.nodes())
            .map(|(node, content)| InboxNode {
                id: node.id,
                protocol_id: &node.protocol_id,
                lines: &content.lines,
                is_company: content::company_fill_rec(node),
            })
            .collect();
        build_node_boxes(
            &inboxes,
            &mut metrics,
            node_width,
            &subject_protocol,
            geometry.printable_height_mm(),
        )?
    };
    let pages = paginate::paginate(graph, &boxes, geometry, &mut metrics)?;
    for page in &pages {
        Measurer::ensure_glyphs(&mut metrics, &page.main_title)?;
        Measurer::ensure_glyphs(&mut metrics, &page.sub_title)?;
        for fact in &page.ratio_facts {
            Measurer::ensure_glyphs(&mut metrics, &fact.text)?;
        }
    }
    let count = pages.len();
    for index in 1..=count {
        Measurer::ensure_glyphs(&mut metrics, &captions::footer(index, count))?;
    }
    render::render(&pages, &mut metrics)
}
