//! Shareholder `top_n` fold: ordering, placeholders, ratio sums, and idempotence.

use super::prepare_request;
use crate::error::ErrorCode;
use crate::graph::validate;
use crate::request::{ChartOptions, ChartRequest, Edge, Node, NodeType};

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
        options: request.options,
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
        options: request.options,
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
            options: request.options,
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
