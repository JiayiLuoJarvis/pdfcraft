//! Shareholder `top_n` fold. Validates the original request, then rebuilds holders.

use std::collections::{HashMap, HashSet};

use crate::error::{Error, ErrorCode};
use crate::graph::{resolve_display_ratio, shareholder_ids, validate, Graph};
use crate::ids::{EdgeId, NodeId};
use crate::model::{ChartRequest, Edge, Node, NodeType, RATIO_MAX, RATIO_MIN};

struct Fold {
    parent: NodeId,
    dropped: Vec<EdgeId>,
    total: f64,
}

/// Fold shareholder children so each parent keeps at most `options.top_n`.
///
/// `None` and `Some(0)` return a clone of `request` after it validates.
/// The rebuilt request is not validated. A fold can add a synthetic holder
/// while a dropped holder stays reachable, and that can pass this step and
/// fail a later node-cap check.
pub(crate) fn prepare_request(request: &ChartRequest) -> Result<ChartRequest, Error> {
    let mut graph = validate(request)?;
    let Some(top_n) = request.options.top_n.filter(|top_n| *top_n > 0) else {
        return Ok(request.clone());
    };
    fold_shareholders(&mut graph, top_n)
}

fn fold_shareholders(graph: &mut Graph, top_n: usize) -> Result<ChartRequest, Error> {
    let holders = shareholder_ids(graph);
    let mut parents = vec![graph.subject];
    for node in &graph.nodes {
        if holders.contains(&node.id) {
            parents.push(node.id);
        }
    }
    let mut folds = Vec::new();
    for parent in parents {
        if let Some(fold) = fold_parent(graph, parent, top_n, &holders)? {
            folds.push(fold);
        }
    }
    if folds.is_empty() {
        return Ok(graph.to_request());
    }
    Ok(rebuild(graph, &holders, &folds))
}

fn fold_parent(
    graph: &mut Graph,
    parent: NodeId,
    top_n: usize,
    holders: &HashSet<NodeId>,
) -> Result<Option<Fold>, Error> {
    let parent_protocol = graph.protocol_id(parent).to_owned();
    let synthetic = synthetic_id(&parent_protocol);
    let children: Vec<EdgeId> = graph
        .incoming_edges(parent)
        .iter()
        .copied()
        .filter(|&edge| {
            let from = graph.edge_from(edge);
            holders.contains(&from) && graph.protocol_id(from) != synthetic
        })
        .collect();
    let mut ratios = HashMap::with_capacity(children.len());
    for &edge in &children {
        let from = graph.edge_from(edge);
        let ratio = resolve_display_ratio(graph, from)?.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProtocol,
                format!(
                    "top_n requires a display ratio for {} under {parent_protocol}",
                    graph.protocol_id(from)
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
    let dropped_ratios: Vec<f64> = dropped
        .iter()
        .map(|&edge| ratios[&graph.edge_from(edge)])
        .collect();
    let total = exact_sum(&dropped_ratios);
    if !total.is_finite() || !(RATIO_MIN..=RATIO_MAX).contains(&total) {
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!(
                "folded ratio sum={total:?} under {parent_protocol} must be in {RATIO_MIN}..{RATIO_MAX}"
            ),
        ));
    }
    if graph.contains_protocol(&synthetic) {
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!("node id {synthetic} is reserved for the folded holders of {parent_protocol}"),
        ));
    }
    Ok(Some(Fold {
        parent,
        dropped,
        total,
    }))
}

/// Finite sum matching `CPython` `math.fsum`, including the half-even correction.
fn exact_sum(values: &[f64]) -> f64 {
    let mut partials: Vec<f64> = Vec::new();
    for &value in values {
        let mut x = value;
        let mut i = 0;
        for j in 0..partials.len() {
            let mut y = partials[j];
            if x.abs() < y.abs() {
                std::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            let yr = hi - x;
            let lo = y - yr;
            if nonzero(lo) {
                partials[i] = lo;
                i += 1;
            }
            x = hi;
        }
        partials.truncate(i);
        if nonzero(x) {
            partials.push(x);
        }
    }
    if partials.is_empty() {
        return 0.0;
    }
    let mut n = partials.len() - 1;
    let mut hi = partials[n];
    let mut lo = 0.0;
    while n > 0 {
        let x = hi;
        n -= 1;
        let y = partials[n];
        hi = x + y;
        let yr = hi - x;
        lo = y - yr;
        if nonzero(lo) {
            break;
        }
    }
    if n > 0 {
        let next = partials[n - 1];
        if (lo < 0.0 && next < 0.0) || (lo > 0.0 && next > 0.0) {
            let y = lo * 2.0;
            let x = hi + y;
            let yr = x - hi;
            if same_float(y, yr) {
                hi = x;
            }
        }
    }
    hi
}

fn nonzero(value: f64) -> bool {
    value.to_bits() != 0.0_f64.to_bits() && value.to_bits() != (-0.0_f64).to_bits()
}

fn same_float(left: f64, right: f64) -> bool {
    left.to_bits() == right.to_bits()
        || (left.to_bits() == 0.0_f64.to_bits() && right.to_bits() == (-0.0_f64).to_bits())
        || (left.to_bits() == (-0.0_f64).to_bits() && right.to_bits() == 0.0_f64.to_bits())
}

fn rebuild(graph: &Graph, holders: &HashSet<NodeId>, folds: &[Fold]) -> ChartRequest {
    let folded_edges: HashSet<EdgeId> = folds
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
    let removed: HashSet<NodeId> = holders.difference(&alive).copied().collect();
    let kept: Vec<&Fold> = folds
        .iter()
        .filter(|fold| !removed.contains(&fold.parent))
        .collect();

    let mut nodes: Vec<Node> = graph
        .nodes
        .iter()
        .filter(|node| !removed.contains(&node.id))
        .map(|node| Node {
            id: node.protocol_id.clone(),
            name: node.name.clone(),
            legal_person: node.legal_person.clone(),
            ratio: node.ratio,
            node_type: node.node_type,
            paid_in_amount: node.paid_in_amount.clone(),
            currency: node.currency.clone(),
            location: node.location.clone(),
            approval_date: node.approval_date.clone(),
        })
        .collect();
    for fold in &kept {
        nodes.push(Node {
            id: synthetic_id(graph.protocol_id(fold.parent)),
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

    let mut edges: Vec<Edge> = graph
        .edges
        .iter()
        .filter(|edge| {
            !removed.contains(&edge.from)
                && !removed.contains(&edge.to)
                && !folded_edges.contains(&edge.id)
        })
        .map(|edge| Edge {
            from_id: graph.protocol_id(edge.from).to_owned(),
            to_id: graph.protocol_id(edge.to).to_owned(),
            ratio: edge.ratio,
        })
        .collect();
    for fold in &kept {
        edges.push(Edge {
            from_id: synthetic_id(graph.protocol_id(fold.parent)),
            to_id: graph.protocol_id(fold.parent).to_owned(),
            ratio: Some(fold.total),
        });
    }

    ChartRequest {
        subject_id: graph.subject_protocol_id.clone(),
        nodes,
        edges,
        options: graph.options.clone(),
    }
}

fn synthetic_id(parent_id: &str) -> String {
    format!("__other__:{parent_id}")
}

fn other_shareholders_name(count: usize) -> String {
    crate::captions::other_shareholders_name(count)
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
    fn six_equal_sixths_fold_to_one_hundred() {
        let share = 100.0 / 6.0;
        let mut nodes = vec![company("root", "主体", None)];
        let mut edges = Vec::new();
        for index in 0..7 {
            let id = format!("s{index}");
            nodes.push(person(&id, &id, share));
            edges.push(holding(&id, "root", share));
        }
        let mut request = ChartRequest::new("root", nodes, edges);
        request.options.top_n = Some(1);
        let folded = prepare_request(&request).expect("six sixths sum to 100");
        let synthetic = folded
            .nodes
            .iter()
            .find(|node| node.id == "__other__:root")
            .expect("synthetic");
        assert_eq!(synthetic.ratio.map(f64::to_bits), Some(100.0_f64.to_bits()));
        assert_eq!(synthetic.name, "其他股东(6家)");
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
