//! Shareholder `top_n` fold. Validates the original request, then rebuilds holders.

use std::collections::{HashMap, HashSet};

use crate::error::{Error, ErrorCode};
use crate::graph::{resolve_display_ratio, shareholder_ids, validate, ValidatedGraph};
use crate::model::{ChartRequest, Edge, Node, NodeType, RATIO_MAX, RATIO_MIN};

struct Fold {
    parent: usize,
    dropped: Vec<usize>,
    total: f64,
}

/// Fold shareholder children so each parent keeps at most `options.top_n`.
///
/// `None` and `Some(0)` return a clone of `request` after it validates.
/// The rebuilt request is not validated. A fold can add a synthetic holder
/// while a dropped holder stays reachable, and that can pass this step and
/// fail a later node-cap check.
pub(crate) fn prepare_request(request: &ChartRequest) -> Result<ChartRequest, Error> {
    let graph = validate(request)?;
    let Some(top_n) = request.options.top_n.filter(|top_n| *top_n > 0) else {
        return Ok(request.clone());
    };
    fold_shareholders(&graph, top_n)
}

fn fold_shareholders(graph: &ValidatedGraph<'_>, top_n: usize) -> Result<ChartRequest, Error> {
    let holders = shareholder_ids(graph);
    let mut parents = vec![graph.subject];
    for index in 0..graph.request.nodes.len() {
        if holders.contains(&index) {
            parents.push(index);
        }
    }
    let mut cache = HashMap::new();
    let mut folds = Vec::new();
    for parent in parents {
        if let Some(fold) = fold_parent(graph, parent, top_n, &holders, &mut cache)? {
            folds.push(fold);
        }
    }
    if folds.is_empty() {
        return Ok(graph.request.clone());
    }
    Ok(rebuild(graph, &holders, &folds))
}

fn fold_parent(
    graph: &ValidatedGraph<'_>,
    parent: usize,
    top_n: usize,
    holders: &HashSet<usize>,
    cache: &mut HashMap<(usize, usize), bool>,
) -> Result<Option<Fold>, Error> {
    let parent_id = graph.node_id(parent);
    let synthetic = synthetic_id(parent_id);
    let children: Vec<usize> = graph
        .incoming_edges(parent)
        .iter()
        .copied()
        .filter(|&edge| {
            let from = graph.edge_from(edge);
            holders.contains(&from) && graph.node_id(from) != synthetic
        })
        .collect();
    let mut ratios = HashMap::with_capacity(children.len());
    for &edge in &children {
        let from = graph.edge_from(edge);
        let ratio = resolve_display_ratio(graph, from, cache)?.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProtocol,
                format!(
                    "top_n requires a display ratio for {} under {parent_id}",
                    graph.node_id(from)
                ),
            )
        })?;
        ratios.insert(from, ratio);
    }
    if children.len() <= top_n {
        return Ok(None);
    }
    let mut ranked = children;
    ranked.sort_by(|&left, &right| {
        let left_from = graph.edge_from(left);
        let right_from = graph.edge_from(right);
        ratios[&right_from]
            .total_cmp(&ratios[&left_from])
            .then(left_from.cmp(&right_from))
    });
    let dropped = ranked.split_off(top_n);
    let total: f64 = dropped
        .iter()
        .map(|&edge| ratios[&graph.edge_from(edge)])
        .sum();
    if !total.is_finite() || !(RATIO_MIN..=RATIO_MAX).contains(&total) {
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!(
                "folded ratio sum={total:?} under {parent_id} must be in {RATIO_MIN}..{RATIO_MAX}"
            ),
        ));
    }
    if graph.contains_id(&synthetic) {
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!("node id {synthetic} is reserved for the folded holders of {parent_id}"),
        ));
    }
    Ok(Some(Fold {
        parent,
        dropped,
        total,
    }))
}

fn rebuild(graph: &ValidatedGraph<'_>, holders: &HashSet<usize>, folds: &[Fold]) -> ChartRequest {
    let request = graph.request;
    let folded_edges: HashSet<usize> = folds
        .iter()
        .flat_map(|fold| fold.dropped.iter().copied())
        .collect();
    let mut alive = HashSet::new();
    let mut stack = vec![graph.subject];
    while let Some(current) = stack.pop() {
        for &edge in graph.incoming_edges(current) {
            let from = graph.edge_from(edge);
            if holders.contains(&from) && !folded_edges.contains(&edge) && alive.insert(from) {
                stack.push(from);
            }
        }
    }
    let removed: HashSet<usize> = holders.difference(&alive).copied().collect();
    let kept: Vec<&Fold> = folds
        .iter()
        .filter(|fold| !removed.contains(&fold.parent))
        .collect();

    let mut nodes: Vec<Node> = request
        .nodes
        .iter()
        .enumerate()
        .filter(|(index, _)| !removed.contains(index))
        .map(|(_, node)| node.clone())
        .collect();
    for fold in &kept {
        nodes.push(Node {
            id: synthetic_id(graph.node_id(fold.parent)),
            name: other_shareholders_name(fold.dropped.len()),
            legal_person: None,
            ratio: Some(fold.total),
            node_type: Some(NodeType::Company),
            paid_in_amount: None,
            currency: None,
            location: None,
            approval_date: None,
        });
    }

    let mut edges: Vec<Edge> = request
        .edges
        .iter()
        .enumerate()
        .filter(|(edge, _)| {
            let from = graph.edge_from(*edge);
            let to = graph.edge_to(*edge);
            !removed.contains(&from) && !removed.contains(&to) && !folded_edges.contains(edge)
        })
        .map(|(_, edge)| edge.clone())
        .collect();
    for fold in &kept {
        edges.push(Edge {
            from_id: synthetic_id(graph.node_id(fold.parent)),
            to_id: graph.node_id(fold.parent).to_owned(),
            ratio: Some(fold.total),
        });
    }

    ChartRequest {
        subject_id: request.subject_id.clone(),
        nodes,
        edges,
        options: request.options.clone(),
    }
}

fn synthetic_id(parent_id: &str) -> String {
    format!("__other__:{parent_id}")
}

fn other_shareholders_name(count: usize) -> String {
    format!("其他股东({count}家)")
}

#[cfg(test)]
mod tests {
    use super::prepare_request;
    use crate::error::ErrorCode;
    use crate::graph::validate;
    use crate::model::{ChartOptions, ChartRequest, Edge, Node, NodeType};

    fn person(id: &str, name: &str, ratio: f64) -> Node {
        Node {
            ratio: Some(ratio),
            node_type: Some(NodeType::Person),
            ..Node::new(id, name)
        }
    }

    fn company(id: &str, name: &str, ratio: Option<f64>) -> Node {
        Node {
            ratio,
            node_type: Some(NodeType::Company),
            ..Node::new(id, name)
        }
    }

    fn holding(from: &str, to: &str, ratio: f64) -> Edge {
        Edge {
            ratio: Some(ratio),
            ..Edge::new(from, to)
        }
    }

    fn many_holders(top_n: Option<usize>) -> ChartRequest {
        let mut request = ChartRequest::new(
            "root",
            vec![
                Node {
                    legal_person: Some("甲法人".to_owned()),
                    ..person("s1", "甲", 40.0)
                },
                person("s2", "乙", 30.0),
                person("s3", "丙", 20.0),
                person("s4", "丁", 10.0),
                company("root", "主体", None),
                company("c1", "被投", Some(100.0)),
            ],
            vec![
                holding("s1", "root", 40.0),
                holding("s2", "root", 30.0),
                holding("s3", "root", 20.0),
                holding("s4", "root", 10.0),
                holding("root", "c1", 100.0),
            ],
        );
        request.options = ChartOptions {
            title: Some("自定义".to_owned()),
            top_n,
            ..ChartOptions::default()
        };
        request
    }

    #[test]
    fn none_and_zero_return_the_same_request() {
        let base = many_holders(None);
        assert_eq!(prepare_request(&base).expect("none"), base);
        let zero = many_holders(Some(0));
        assert_eq!(prepare_request(&zero).expect("zero"), zero);
    }

    #[test]
    fn fold_is_idempotent_and_keeps_the_investment() {
        let request = many_holders(Some(2));
        let once = prepare_request(&request).expect("fold");
        let twice = prepare_request(&once).expect("again");
        let expected = ChartRequest {
            subject_id: "root".to_owned(),
            nodes: vec![
                Node {
                    legal_person: Some("甲法人".to_owned()),
                    ..person("s1", "甲", 40.0)
                },
                person("s2", "乙", 30.0),
                company("root", "主体", None),
                company("c1", "被投", Some(100.0)),
                company("__other__:root", "其他股东(2家)", Some(30.0)),
            ],
            edges: vec![
                holding("s1", "root", 40.0),
                holding("s2", "root", 30.0),
                holding("root", "c1", 100.0),
                holding("__other__:root", "root", 30.0),
            ],
            options: request.options.clone(),
        };
        assert_eq!(once, expected);
        assert_eq!(twice, once);
    }

    #[test]
    fn a_penetrating_parent_folds_its_own_children() {
        let mut request = ChartRequest::new(
            "root",
            vec![
                company("co", "控股", Some(60.0)),
                person("p1", "穿透甲", 50.0),
                person("p2", "穿透乙", 30.0),
                person("p3", "穿透丙", 20.0),
                company("root", "主体", None),
            ],
            vec![
                holding("co", "root", 60.0),
                holding("p1", "co", 50.0),
                holding("p2", "co", 30.0),
                holding("p3", "co", 20.0),
            ],
        );
        request.options.top_n = Some(1);
        let folded = prepare_request(&request).expect("fold");
        let expected = ChartRequest {
            options: request.options.clone(),
            ..ChartRequest::new(
                "root",
                vec![
                    company("co", "控股", Some(60.0)),
                    person("p1", "穿透甲", 50.0),
                    company("root", "主体", None),
                    company("__other__:co", "其他股东(2家)", Some(50.0)),
                ],
                vec![
                    holding("co", "root", 60.0),
                    holding("p1", "co", 50.0),
                    holding("__other__:co", "co", 50.0),
                ],
            )
        };
        assert_eq!(folded, expected);
    }

    #[test]
    fn edge_ratio_is_the_display_ratio_when_the_node_has_none() {
        let mut request = ChartRequest::new(
            "root",
            vec![
                Node::new("s1", "甲"),
                Node::new("s2", "乙"),
                Node::new("s3", "丙"),
                Node::new("root", "主体"),
            ],
            vec![
                holding("s1", "root", 50.0),
                holding("s2", "root", 30.0),
                holding("s3", "root", 20.0),
            ],
        );
        request.options.top_n = Some(1);
        let folded = prepare_request(&request).expect("fold");
        assert_eq!(
            folded,
            ChartRequest {
                options: request.options.clone(),
                ..ChartRequest::new(
                    "root",
                    vec![
                        Node::new("s1", "甲"),
                        Node::new("root", "主体"),
                        company("__other__:root", "其他股东(2家)", Some(50.0)),
                    ],
                    vec![
                        holding("s1", "root", 50.0),
                        holding("__other__:root", "root", 50.0),
                    ],
                )
            }
        );
    }

    #[test]
    fn node_ratio_wins_when_the_fold_sums_children() {
        let mut request = ChartRequest::new(
            "root",
            vec![
                Node {
                    ratio: Some(50.0),
                    ..Node::new("s1", "甲")
                },
                Node {
                    ratio: Some(30.0),
                    ..Node::new("s2", "乙")
                },
                Node {
                    ratio: Some(20.0),
                    ..Node::new("s3", "丙")
                },
                Node::new("root", "主体"),
            ],
            vec![
                holding("s1", "root", 1.0),
                holding("s2", "root", 1.0),
                holding("s3", "root", 1.0),
            ],
        );
        request.options.top_n = Some(1);
        let folded = prepare_request(&request).expect("fold");
        let synthetic = folded
            .nodes
            .iter()
            .find(|node| node.id == "__other__:root")
            .expect("synthetic");
        assert_eq!(synthetic.ratio, Some(50.0));
    }

    #[test]
    fn equal_ratios_keep_the_earlier_node() {
        let mut request = ChartRequest::new(
            "root",
            vec![
                Node {
                    ratio: Some(10.0),
                    ..Node::new("b", "乙")
                },
                Node {
                    ratio: Some(10.0),
                    ..Node::new("a", "甲")
                },
                Node {
                    ratio: Some(90.0),
                    ..Node::new("c", "丙")
                },
                Node::new("root", "主体"),
            ],
            vec![
                holding("a", "root", 10.0),
                holding("b", "root", 10.0),
                holding("c", "root", 90.0),
            ],
        );
        request.options.top_n = Some(2);
        let folded = prepare_request(&request).expect("fold");
        let ids: Vec<_> = folded.nodes.iter().map(|node| node.id.as_str()).collect();
        assert_eq!(ids, ["b", "c", "root", "__other__:root"]);
    }

    #[test]
    fn an_existing_synthetic_id_cannot_be_reused() {
        let mut request = ChartRequest::new(
            "root",
            vec![
                person("s1", "甲", 40.0),
                person("s2", "乙", 30.0),
                person("s3", "丙", 20.0),
                company("__other__:root", "已有", Some(10.0)),
                company("root", "主体", None),
            ],
            vec![
                holding("s1", "root", 40.0),
                holding("s2", "root", 30.0),
                holding("s3", "root", 20.0),
                holding("__other__:root", "root", 10.0),
            ],
        );
        request.options.top_n = Some(2);
        let error = prepare_request(&request).expect_err("collision");
        assert_eq!(error.code, ErrorCode::InvalidProtocol);
        assert!(
            error.message.contains("__other__:root"),
            "{}",
            error.message
        );
    }

    #[test]
    fn a_folded_sum_above_100_is_rejected() {
        let mut request = ChartRequest::new(
            "root",
            vec![
                person("s1", "甲", 60.0),
                person("s2", "乙", 60.0),
                person("s3", "丙", 60.0),
                company("root", "主体", None),
            ],
            vec![
                holding("s1", "root", 60.0),
                holding("s2", "root", 60.0),
                holding("s3", "root", 60.0),
            ],
        );
        request.options.top_n = Some(1);
        let error = prepare_request(&request).expect_err("sum");
        assert_eq!(error.code, ErrorCode::InvalidProtocol);
        assert!(error.message.contains("folded ratio"), "{}", error.message);
    }

    #[test]
    fn prepare_does_not_apply_max_nodes_to_the_folded_request() {
        let mut request = ChartRequest::new(
            "root",
            vec![
                person("s1", "甲", 50.0),
                person("s2", "乙", 30.0),
                person("s3", "丙", 20.0),
                company("root", "主体", None),
            ],
            vec![
                holding("s1", "root", 50.0),
                holding("s2", "root", 30.0),
                holding("s3", "root", 20.0),
                holding("s3", "s1", 20.0),
            ],
        );
        request.options.max_nodes = Some(4);
        request.options.top_n = Some(2);
        let prepared = prepare_request(&request).expect("fold stays under the original cap only");
        assert_eq!(prepared.nodes.len(), 5);
        match validate(&prepared) {
            Ok(_) => panic!("folded graph should fail the node cap"),
            Err(error) => assert_eq!(error.code, ErrorCode::TooManyNodes),
        }
    }
}
