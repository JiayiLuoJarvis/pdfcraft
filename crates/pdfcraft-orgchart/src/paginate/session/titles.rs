//! Page titles and the caption on an attachment page.

use super::Session;
use crate::captions::{
    holds_relation, investment_page_titles, invests_relation, shareholder_page_titles,
};
use crate::error::Error;
use crate::font::Measurer;
use crate::graph::NodeId;
use crate::layout::LayoutEngine;

impl<E: LayoutEngine, M: Measurer> Session<'_, E, M> {
    pub(super) fn next_shareholder_titles(&mut self) -> (String, String) {
        self.shareholder_continue += 1;
        shareholder_page_titles(&self.subject_name, self.shareholder_continue)
    }

    pub(super) fn next_investment_titles(&mut self) -> (String, String) {
        self.investment_continue += 1;
        investment_page_titles(&self.subject_name, self.investment_continue)
    }

    pub(super) fn cut_caption(&mut self, node_id: NodeId) -> Result<String, Error> {
        let name = self.node_name(node_id);
        let ratio = crate::graph::resolve_display_ratio(&mut self.graph, node_id)
            .map_err(Error::into_layout_failed)?;
        match self
            .graph
            .layout_parent_ends(node_id)
            .map_err(Error::into_layout_failed)?
        {
            None => Ok(name),
            Some((from_id, to_id)) => {
                if from_id == node_id {
                    Ok(holds_relation(&name, &self.node_name(to_id), ratio))
                } else {
                    Ok(invests_relation(&self.node_name(from_id), &name, ratio))
                }
            }
        }
    }

    pub(super) fn node_name(&self, id: NodeId) -> String {
        self.graph.node_name(id).to_owned()
    }
}
