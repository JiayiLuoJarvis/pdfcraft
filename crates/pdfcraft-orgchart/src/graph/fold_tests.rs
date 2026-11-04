//! Shareholder `top_n` fold: ordering, placeholders, ratio sums, and idempotence.

use std::num::NonZeroUsize;

use super::{fold, prepare_graph};
use crate::error::ErrorCode;
use crate::graph::{recheck, validate, Graph, NodeId, NodeOrigin, NodeRec};
use crate::request::{ChartRequest, Edge, Node, NodeType, PaidIn};

fn top_n(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("top_n is non-zero")
}

/// Caller id, or `__other__:{parent}` for a fold stand-in.
fn key(graph: &Graph, id: NodeId) -> String {
    match graph.origin(id) {
        NodeOrigin::Caller { id, .. } => id.clone(),
        NodeOrigin::FoldedHolders { parent, .. } => format!("__other__:{}", key(graph, *parent)),
        NodeOrigin::Overflow { .. } => panic!("fold never sees overflow nodes"),
    }
}

/// Arena nodes in order, as request nodes keyed by [`key`].
fn nodes_of(graph: &Graph) -> Vec<Node> {
    graph
        .nodes
        .iter()
        .map(|node| Node {
            id: key(graph, node.id),
            name: node.name.clone(),
            legal_person: node.legal_person.clone(),
            ratio: node.ratio,
            node_type: node.node_type,
            paid_in: node.paid_in_amount.as_ref().map(|amount| PaidIn {
                amount: amount.clone(),
                currency: node.currency.clone(),
            }),
            location: node.location.clone(),
            approval_date: node.approval_date.clone(),
            folded_holders_of: None,
        })
        .collect()
}

/// Arena edges in order, with the request position they came from.
fn edges_of(graph: &Graph) -> Vec<(Edge, Option<usize>)> {
    graph
        .edges
        .iter()
        .map(|edge| {
            (
                Edge {
                    holder: key(graph, edge.from),
                    held: key(graph, edge.to),
                    ratio: edge.ratio,
                },
                edge.caller_index,
            )
        })
        .collect()
}

fn folded_counts(graph: &Graph) -> Vec<(String, usize)> {
    graph
        .nodes
        .iter()
        .filter_map(|node| match node.origin {
            NodeOrigin::FoldedHolders { count, .. } => Some((key(graph, node.id), count)),
            NodeOrigin::Caller { .. } | NodeOrigin::Overflow { .. } => None,
        })
        .collect()
}

fn node_keyed<'a>(graph: &'a Graph, wanted: &str) -> &'a NodeRec {
    graph
        .nodes
        .iter()
        .find(|node| key(graph, node.id) == wanted)
        .expect(wanted)
}

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

fn many_holders(limit: Option<NonZeroUsize>) -> ChartRequest {
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
    request.top_n = limit;
    request
}

#[test]
fn none_returns_the_same_request() {
    let request = many_holders(None);
    let prepared = prepare_graph(&request).expect("prepare");
    let plain = validate(&request).expect("validate");
    assert_eq!(nodes_of(&prepared), request.nodes);
    assert_eq!(nodes_of(&prepared), nodes_of(&plain));
    assert_eq!(edges_of(&prepared), edges_of(&plain));
    assert!(folded_counts(&prepared).is_empty());
}

#[test]
fn fold_is_idempotent_and_keeps_the_investment() {
    let request = many_holders(Some(top_n(2)));
    let once = prepare_graph(&request).expect("fold");
    let twice = fold(once.clone(), 2).expect("again");
    let expected = ChartRequest::new(
        "root",
        vec![
            Node {
                legal_person: Some("甲法人".to_owned()),
                ..person("s1", "甲", 40.0)
            },
            person("s2", "乙", 30.0),
            company("root", "主体", None),
            company("c1", "被投", Some(100.0)),
            company("__other__:root", "其他股东(2家)", Some(30.0)),
        ],
        vec![
            holding("s1", "root", 40.0),
            holding("s2", "root", 30.0),
            holding("root", "c1", 100.0),
            holding("__other__:root", "root", 30.0),
        ],
    );
    assert_eq!(nodes_of(&once), expected.nodes);
    assert_eq!(
        edges_of(&once),
        expected
            .edges
            .into_iter()
            .zip([Some(0), Some(1), Some(4), None])
            .collect::<Vec<_>>()
    );
    assert_eq!(folded_counts(&once), [("__other__:root".to_owned(), 2)]);
    assert_eq!(once.subject, once.nodes[2].id);
    assert_eq!(nodes_of(&twice), nodes_of(&once));
    assert_eq!(edges_of(&twice), edges_of(&once));
    assert_eq!(folded_counts(&twice), folded_counts(&once));
}

#[test]
fn writing_the_fold_back_does_not_fold_again() {
    let once = prepare_graph(&many_holders(Some(top_n(2)))).expect("fold");
    let mut stand_in = company("other", "其他股东(2家)", Some(30.0));
    stand_in.folded_holders_of = Some("root".to_owned());
    let mut again = ChartRequest::new(
        "root",
        vec![
            Node {
                legal_person: Some("甲法人".to_owned()),
                ..person("s1", "甲", 40.0)
            },
            person("s2", "乙", 30.0),
            company("root", "主体", None),
            company("c1", "被投", Some(100.0)),
            stand_in,
        ],
        vec![
            holding("s1", "root", 40.0),
            holding("s2", "root", 30.0),
            holding("root", "c1", 100.0),
            holding("other", "root", 30.0),
        ],
    );
    again.top_n = Some(top_n(2));
    let second = prepare_graph(&again).expect("write-back");
    let names = |graph: &Graph| {
        graph
            .nodes
            .iter()
            .map(|node| node.name.clone())
            .collect::<Vec<_>>()
    };
    assert!(
        folded_counts(&second).is_empty(),
        "a declared stand-in is not folded again"
    );
    assert_eq!(names(&second), names(&once));
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
    request.top_n = Some(top_n(1));
    let folded = prepare_graph(&request).expect("fold");
    let expected = ChartRequest::new(
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
    );
    assert_eq!(nodes_of(&folded), expected.nodes);
    assert_eq!(
        edges_of(&folded),
        expected
            .edges
            .into_iter()
            .zip([Some(0), Some(1), None])
            .collect::<Vec<_>>()
    );
    assert_eq!(folded_counts(&folded), [("__other__:co".to_owned(), 2)]);
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
    request.top_n = Some(top_n(1));
    let folded = prepare_graph(&request).expect("fold");
    assert_eq!(
        nodes_of(&folded),
        vec![
            Node::new("s1", "甲"),
            Node::new("root", "主体"),
            company("__other__:root", "其他股东(2家)", Some(50.0)),
        ]
    );
    assert_eq!(
        edges_of(&folded),
        vec![
            (holding("s1", "root", 50.0), Some(0)),
            (holding("__other__:root", "root", 50.0), None),
        ]
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
    request.top_n = Some(top_n(1));
    let folded = prepare_graph(&request).expect("fold");
    let synthetic = node_keyed(&folded, "__other__:root");
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
    request.top_n = Some(top_n(2));
    let folded = prepare_graph(&request).expect("fold");
    let ids: Vec<_> = folded
        .nodes
        .iter()
        .map(|node| key(&folded, node.id))
        .collect();
    assert_eq!(ids, ["b", "c", "root", "__other__:root"]);
}

#[test]
fn an_existing_stand_in_cannot_be_folded_again() {
    let mut other = company("other", "已有", Some(10.0));
    other.folded_holders_of = Some("root".to_owned());
    let mut request = ChartRequest::new(
        "root",
        vec![
            person("s1", "甲", 40.0),
            person("s2", "乙", 30.0),
            person("s3", "丙", 20.0),
            other,
            company("root", "主体", None),
        ],
        vec![
            holding("s1", "root", 40.0),
            holding("s2", "root", 30.0),
            holding("s3", "root", 20.0),
            holding("other", "root", 10.0),
        ],
    );
    request.top_n = Some(top_n(2));
    let error = prepare_graph(&request).expect_err("collision");
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert_eq!(
        error.subject(),
        Some(&crate::ErrorSubject::Node {
            index: 3,
            id: "other".to_owned(),
        })
    );
}

#[test]
fn other_prefix_without_folded_holders_is_ranked() {
    let mut request = ChartRequest::new(
        "root",
        vec![
            person("s1", "甲", 40.0),
            person("s2", "乙", 30.0),
            person("__other__:root", "普通", 10.0),
            company("root", "主体", None),
        ],
        vec![
            holding("s1", "root", 40.0),
            holding("s2", "root", 30.0),
            holding("__other__:root", "root", 10.0),
        ],
    );
    request.top_n = Some(top_n(1));
    let folded = prepare_graph(&request).expect("ordinary holder folds");
    assert_eq!(
        folded_counts(&folded).len(),
        1,
        "the prefix is not a stand-in"
    );
    assert!(
        folded.id_of("__other__:root").is_none(),
        "lowest holder was folded"
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
    request.top_n = Some(top_n(1));
    let folded = prepare_graph(&request).expect("six sixths sum to 100");
    let synthetic = node_keyed(&folded, "__other__:root");
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
    request.top_n = Some(top_n(1));
    let error = prepare_graph(&request).expect_err("sum");
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert!(
        error.message().contains("folded ratio"),
        "{}",
        error.message()
    );
    assert_eq!(
        error.subject(),
        Some(&crate::ErrorSubject::Node {
            index: 3,
            id: "root".to_owned(),
        })
    );
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
    request.max_nodes = 4;
    request.top_n = Some(top_n(2));
    let mut prepared = prepare_graph(&request).expect("fold stays under the original cap only");
    assert_eq!(prepared.nodes.len(), 5);
    match recheck(&mut prepared) {
        Ok(()) => panic!("folded graph should fail the node cap"),
        Err(error) => {
            assert_eq!(error.code(), ErrorCode::TooManyNodes);
            assert_eq!(error.subject(), Some(&crate::ErrorSubject::Field("nodes")));
        }
    }
}
