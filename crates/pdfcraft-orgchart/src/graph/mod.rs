//! The validated chart as an arena, its protocol checks, the `top_n` fold, and ratio lookup.

mod arena;
mod check;
mod fold;
mod ratio;

pub(crate) use arena::{Graph, NodeId, NodeOrigin, NodeRec};
#[cfg(test)]
pub(crate) use check::validate;
pub(crate) use check::{investment_ids, recheck, shareholder_ids};
pub(crate) use fold::prepare_graph;
pub(crate) use ratio::{ratio_source, resolve_display_ratio};

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{investment_ids, resolve_display_ratio, shareholder_ids, validate, NodeId};
    use crate::request::{ChartRequest, Edge, Node};

    fn id_of(graph: &super::Graph, protocol: &str) -> NodeId {
        graph.id_of(protocol).expect("node id")
    }

    fn ratio_of(request: &ChartRequest, protocol: &str) -> Option<f64> {
        let mut graph = validate(request).expect("valid graph");
        let id = id_of(&graph, protocol);
        resolve_display_ratio(&mut graph, id).expect("ratio")
    }

    #[test]
    fn node_ratio_wins_over_the_edge() {
        let request = ChartRequest::new(
            "root",
            vec![
                Node {
                    ratio: Some(55.0),
                    ..Node::new("a", "甲")
                },
                Node::new("root", "主体"),
            ],
            vec![Edge {
                ratio: Some(60.0),
                ..Edge::new("a", "root")
            }],
        );
        assert_eq!(ratio_of(&request, "a"), Some(55.0));
    }

    #[test]
    fn backfills_from_the_unique_edge() {
        let request = ChartRequest::new(
            "root",
            vec![Node::new("a", "甲"), Node::new("root", "主体")],
            vec![Edge {
                ratio: Some(60.0),
                ..Edge::new("a", "root")
            }],
        );
        assert_eq!(ratio_of(&request, "a"), Some(60.0));
    }

    #[test]
    fn missing_ratio_is_none() {
        let request = ChartRequest::new(
            "root",
            vec![Node::new("a", "甲"), Node::new("root", "主体")],
            vec![Edge::new("a", "root")],
        );
        assert_eq!(ratio_of(&request, "a"), None);
    }

    #[test]
    fn diamond_with_a_node_ratio_uses_that_ratio() {
        let request = ChartRequest::new(
            "root",
            vec![
                Node::new("root", "主体"),
                Node::new("a", "甲"),
                Node::new("b", "乙"),
                Node {
                    ratio: Some(50.0),
                    ..Node::new("d", "合资")
                },
            ],
            vec![
                Edge::new("root", "a"),
                Edge::new("root", "b"),
                Edge::new("a", "d"),
                Edge::new("b", "d"),
            ],
        );
        assert_eq!(ratio_of(&request, "d"), Some(50.0));
        assert_eq!(ratio_of(&request, "root"), None);
    }

    #[test]
    fn subject_does_not_take_a_holder_edge_ratio() {
        let request = ChartRequest::new(
            "root",
            vec![Node::new("root", "主体"), Node::new("a", "甲")],
            vec![Edge {
                ratio: Some(60.0),
                ..Edge::new("a", "root")
            }],
        );
        assert_eq!(ratio_of(&request, "root"), None);
    }

    #[test]
    fn horizontal_edges_stay_inside_one_cone() {
        let request = ChartRequest::new(
            "root",
            vec![
                Node::new("a", "a"),
                Node::new("b", "b"),
                Node::new("root", "root"),
                Node::new("x", "x"),
                Node::new("y", "y"),
            ],
            vec![
                Edge::new("a", "root"),
                Edge::new("b", "root"),
                Edge::new("a", "b"),
                Edge::new("root", "x"),
                Edge::new("root", "y"),
                Edge::new("x", "y"),
            ],
        );
        let graph = validate(&request).expect("valid graph");
        let holders: BTreeSet<_> = shareholder_ids(&graph)
            .into_iter()
            .map(|id| graph.label(id))
            .collect();
        let investments: BTreeSet<_> = investment_ids(&graph)
            .into_iter()
            .map(|id| graph.label(id))
            .collect();
        assert_eq!(holders, BTreeSet::from(["a".to_owned(), "b".to_owned()]));
        assert_eq!(
            investments,
            BTreeSet::from(["x".to_owned(), "y".to_owned()])
        );
    }
}
