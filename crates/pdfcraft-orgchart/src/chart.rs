//! A compiled chart and the PDF drawn from it.

use crate::captions;
use crate::content;
use crate::error::Error;
use crate::font::{Font, FontMetrics, Measurer};
use crate::graph;
use crate::page::{build_node_boxes, InboxNode};
use crate::paginate;
use crate::render;
use crate::request::ChartRequest;

/// A chart that passed protocol checks and the `top_n` fold.
///
/// [`Chart::new`] consumes the request. On error the request is dropped. Clone
/// it first if a failed attempt must be edited and retried.
#[derive(Debug, Clone)]
#[must_use]
pub struct Chart {
    graph: graph::Graph,
}

impl Chart {
    /// Validate `request`, fold shareholders, then run the protocol rules again
    /// on the folded chart.
    ///
    /// A fold that grows past `max_nodes`, or that leaves a node with more than
    /// one layout parent, fails here. The font is not opened.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the request or the folded chart breaks a protocol
    /// rule. [`Error::code`](crate::Error::code) is the stable value.
    ///
    /// # Examples
    ///
    /// ```
    /// use pdfcraft_orgchart::{Chart, ChartRequest, Edge, Node};
    ///
    /// let request = ChartRequest::new(
    ///     "root",
    ///     vec![Node::new("holder", "甲"), Node::new("root", "主体")],
    ///     vec![Edge::new("holder", "root")],
    /// );
    /// let chart = Chart::new(request)?;
    /// assert_eq!(chart.subject_name(), "主体");
    /// # Ok::<(), pdfcraft_orgchart::Error>(())
    /// ```
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the request is dropped when compilation fails, so a retry clones first"
    )]
    pub fn new(request: ChartRequest) -> Result<Self, Error> {
        let mut graph = graph::prepare_graph(&request)?;
        graph::recheck(&mut graph)?;
        Ok(Self { graph })
    }

    /// Draw this chart with `font`.
    ///
    /// Does not change `self`. Two calls with the same font return the same
    /// bytes. Paging may append overflow nodes, so this clones the chart's
    /// arena first. The PDF has no document info dictionary. Page headers come
    /// from the chart role, not from [`Chart::title`].
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the font or the layout is illegal.
    /// [`Error::code`](crate::Error::code) is the stable value.
    pub fn to_pdf(&self, font: &Font) -> Result<Vec<u8>, Error> {
        draw(self.graph.clone(), font)
    }

    /// `{subject name}股权架构图`. The name is copied as stored.
    #[must_use]
    pub fn title(&self) -> String {
        captions::main_title(self.subject_name())
    }

    /// Display name of the subject, as stored on the request.
    #[must_use]
    pub fn subject_name(&self) -> &str {
        self.graph.node_name(self.graph.subject)
    }
}

fn draw(graph: graph::Graph, font: &Font) -> Result<Vec<u8>, Error> {
    let shown = content::display_of(&graph);
    let margin = graph.style.margin_mm;
    let node_width = graph.style.node_width_mm;
    let geometry = crate::units::PageGeometry::portrait(margin);
    let mut metrics = FontMetrics::from_font(font);
    let boxes = {
        let inboxes: Vec<InboxNode<'_>> = graph
            .nodes
            .iter()
            .zip(shown.nodes())
            .map(|(node, content)| InboxNode {
                id: node.id,
                origin: node.origin.clone(),
                lines: &content.lines,
                is_company: content::company_fill_rec(node),
            })
            .collect();
        build_node_boxes(
            &inboxes,
            &mut metrics,
            node_width,
            graph.subject,
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

#[cfg(test)]
mod tests {
    #[test]
    fn chart_font_and_error_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<super::Chart>();
        assert_send_sync::<crate::Font>();
        assert_send_sync::<crate::Error>();
    }
}
