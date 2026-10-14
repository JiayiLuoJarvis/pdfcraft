//! Protocol failures and accepted graphs, through `validate_request`.

use pdfcraft_orgchart::{
    validate_request, ChartOptions, ChartRequest, Edge, Error, ErrorCode, Node,
};

fn node(id: &str) -> Node {
    Node::new(id, id)
}

fn named(id: &str, name: &str) -> Node {
    Node::new(id, name)
}

fn chart(subject: &str, nodes: Vec<Node>, edges: Vec<Edge>) -> ChartRequest {
    ChartRequest::new(subject, nodes, edges)
}

fn with_options(mut request: ChartRequest, options: ChartOptions) -> ChartRequest {
    request.options = options;
    request
}

fn err(request: &ChartRequest) -> Error {
    validate_request(request).expect_err("request should fail")
}

#[test]
fn accepts_a_simple_dag() {
    let request = chart(
        "root",
        vec![node("a"), node("root"), node("b")],
        vec![
            Edge {
                ratio: Some(60.0),
                ..Edge::new("a", "root")
            },
            Edge {
                ratio: Some(100.0),
                ..Edge::new("root", "b")
            },
        ],
    );
    assert_eq!(validate_request(&request), Ok(()));
}

#[test]
fn rejects_a_missing_subject() {
    let error = err(&chart("missing", vec![node("a")], vec![]));
    assert_eq!(error.code, ErrorCode::SubjectNotFound);
    assert_eq!(error.code.as_str(), "SUBJECT_NOT_FOUND");
}

#[test]
fn rejects_a_cycle() {
    let error = err(&chart(
        "a",
        vec![node("a"), node("b")],
        vec![Edge::new("a", "b"), Edge::new("b", "a")],
    ));
    assert_eq!(error.code, ErrorCode::CycleDetected);
    assert_eq!(error.code.as_str(), "CYCLE_DETECTED");
}

#[test]
fn rejects_a_dangling_edge() {
    let error = err(&chart("a", vec![node("a")], vec![Edge::new("a", "ghost")]));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert!(error.message.contains("dangling edge"), "{}", error.message);
}

#[test]
fn rejects_a_self_loop() {
    let error = err(&chart("a", vec![node("a")], vec![Edge::new("a", "a")]));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert!(error.message.contains("self-loop"), "{}", error.message);
}

#[test]
fn rejects_a_blank_subject() {
    let error = err(&chart("   ", vec![node("a")], vec![]));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert_eq!(error.code.as_str(), "INVALID_PROTOCOL");
}

#[test]
fn rejects_a_blank_node_id_or_name() {
    let blank_id = err(&chart("a", vec![Node::new("  ", "x")], vec![]));
    assert_eq!(blank_id.code, ErrorCode::InvalidProtocol);
    let blank_name = err(&chart("a", vec![Node::new("a", "  ")], vec![]));
    assert_eq!(blank_name.code, ErrorCode::InvalidProtocol);
}

#[test]
fn rejects_a_duplicate_node_id() {
    let error = err(&chart("a", vec![node("a"), node("a")], vec![]));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert!(
        error.message.contains("duplicate node id"),
        "{}",
        error.message
    );
}

#[test]
fn accepts_ancestors_and_descendants() {
    let request = chart(
        "root",
        vec![node("gp"), node("a"), node("root"), node("b"), node("gc")],
        vec![
            Edge::new("gp", "a"),
            Edge::new("a", "root"),
            Edge::new("root", "b"),
            Edge::new("b", "gc"),
        ],
    );
    assert_eq!(validate_request(&request), Ok(()));
}

#[test]
fn rejects_a_shareholder_edge_into_the_investment_cone() {
    let error = err(&chart(
        "root",
        vec![node("a"), node("root"), node("b")],
        vec![
            Edge::new("a", "root"),
            Edge::new("root", "b"),
            Edge::new("a", "b"),
        ],
    ));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert!(
        error
            .message
            .contains("edge crosses shareholder/investment cones: a->b"),
        "{}",
        error.message
    );
}

#[test]
fn accepts_horizontal_edges_inside_one_cone() {
    let request = chart(
        "root",
        vec![node("a"), node("b"), node("root"), node("x"), node("y")],
        vec![
            Edge::new("a", "root"),
            Edge::new("b", "root"),
            Edge::new("a", "b"),
            Edge::new("root", "x"),
            Edge::new("root", "y"),
            Edge::new("x", "y"),
        ],
    );
    assert_eq!(validate_request(&request), Ok(()));
}

#[test]
fn an_investment_edge_back_to_a_shareholder_is_a_cycle() {
    let error = err(&chart(
        "root",
        vec![node("a"), node("root"), node("b")],
        vec![
            Edge::new("a", "root"),
            Edge::new("root", "b"),
            Edge::new("b", "a"),
        ],
    ));
    assert_eq!(error.code, ErrorCode::CycleDetected);
}

#[test]
fn rejects_the_overflow_prefix() {
    let reserved = "__overflow__:root:a";
    let error = err(&chart(
        "root",
        vec![node("root"), node(reserved)],
        vec![Edge::new(reserved, "root")],
    ));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert!(
        error.message.contains("reserved prefix"),
        "{}",
        error.message
    );
}

#[test]
fn accepts_the_other_prefix() {
    let request = chart(
        "root",
        vec![
            named("root", "主体"),
            Node {
                ratio: Some(30.0),
                ..named("__other__:root", "其他股东(2家)")
            },
            Node {
                ratio: Some(70.0),
                ..named("s1", "甲")
            },
        ],
        vec![
            Edge {
                ratio: Some(30.0),
                ..Edge::new("__other__:root", "root")
            },
            Edge {
                ratio: Some(70.0),
                ..Edge::new("s1", "root")
            },
        ],
    );
    assert_eq!(validate_request(&request), Ok(()));
}

#[test]
fn rejects_an_isolated_node() {
    let error = err(&chart(
        "root",
        vec![node("a"), node("root"), node("orphan")],
        vec![Edge::new("a", "root")],
    ));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert!(error.message.contains("orphan"), "{}", error.message);
}

#[test]
fn rejects_a_component_that_does_not_touch_the_subject() {
    let error = err(&chart(
        "root",
        vec![node("root"), node("x"), node("y")],
        vec![Edge::new("x", "y")],
    ));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
}

#[test]
fn rejects_a_sibling_that_only_shares_an_ancestor() {
    let error = err(&chart(
        "root",
        vec![node("a"), node("root"), node("sibling")],
        vec![Edge::new("a", "root"), Edge::new("a", "sibling")],
    ));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert!(error.message.contains("sibling"), "{}", error.message);
}

#[test]
fn accepts_a_subject_only_graph() {
    assert_eq!(
        validate_request(&chart("root", vec![node("root")], vec![])),
        Ok(())
    );
}

#[test]
fn rejects_too_many_nodes() {
    let nodes: Vec<Node> = (1..=101).map(|index| node(&format!("n{index}"))).collect();
    let error = err(&with_options(
        chart("n1", nodes, vec![]),
        ChartOptions {
            max_nodes: Some(100),
            ..ChartOptions::default()
        },
    ));
    assert_eq!(error.code, ErrorCode::TooManyNodes);
    assert_eq!(error.code.as_str(), "TOO_MANY_NODES");
}

#[test]
fn rejects_a_lower_node_cap() {
    let error = err(&with_options(
        chart(
            "root",
            vec![node("root"), node("a"), node("b")],
            vec![Edge::new("a", "root"), Edge::new("root", "b")],
        ),
        ChartOptions {
            max_nodes: Some(2),
            ..ChartOptions::default()
        },
    ));
    assert_eq!(error.code, ErrorCode::TooManyNodes);
}

#[test]
fn rejects_max_nodes_above_the_hard_limit() {
    let error = err(&with_options(
        chart("root", vec![node("root")], vec![]),
        ChartOptions {
            max_nodes: Some(101),
            ..ChartOptions::default()
        },
    ));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert!(error.message.contains("max_nodes"), "{}", error.message);
}

#[test]
fn rejects_top_n_above_the_cap() {
    let error = err(&with_options(
        chart("root", vec![node("root")], vec![]),
        ChartOptions {
            max_nodes: Some(4),
            top_n: Some(5),
            ..ChartOptions::default()
        },
    ));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert!(error.message.contains("top_n"), "{}", error.message);
}

#[test]
fn rejects_non_finite_and_out_of_range_ratios() {
    let root = named("root", "主体");
    for bad in [f64::NAN, f64::INFINITY, -0.01, 100.01] {
        let node_error = err(&chart(
            "root",
            vec![
                Node {
                    ratio: Some(bad),
                    ..named("a", "甲")
                },
                root.clone(),
            ],
            vec![Edge::new("a", "root")],
        ));
        assert_eq!(node_error.code, ErrorCode::InvalidProtocol);
        let edge_error = err(&chart(
            "root",
            vec![named("a", "甲"), root.clone()],
            vec![Edge {
                ratio: Some(bad),
                ..Edge::new("a", "root")
            }],
        ));
        assert_eq!(edge_error.code, ErrorCode::InvalidProtocol);
    }
}

#[test]
fn accepts_inclusive_ratio_and_size_bounds() {
    let mut request = chart(
        "root",
        vec![
            Node {
                ratio: Some(0.0),
                ..named("a", "甲")
            },
            Node {
                ratio: Some(100.0),
                ..named("root", "主体")
            },
        ],
        vec![Edge {
            ratio: Some(100.0),
            ..Edge::new("a", "root")
        }],
    );
    request.options.margin_mm = Some(40.0);
    request.options.node_width_mm = Some(20.0);
    assert_eq!(validate_request(&request), Ok(()));
    request.options.node_width_mm = Some(80.0);
    assert_eq!(validate_request(&request), Ok(()));
}

#[test]
fn rejects_illegal_margin_and_node_width() {
    let root = vec![node("root")];
    for margin in [0.0, 40.01, f64::NAN, f64::NEG_INFINITY] {
        let error = err(&with_options(
            chart("root", root.clone(), vec![]),
            ChartOptions {
                margin_mm: Some(margin),
                ..ChartOptions::default()
            },
        ));
        assert_eq!(error.code, ErrorCode::InvalidProtocol);
    }
    for width in [19.99, 80.01, f64::NAN] {
        let error = err(&with_options(
            chart("root", root.clone(), vec![]),
            ChartOptions {
                node_width_mm: Some(width),
                ..ChartOptions::default()
            },
        ));
        assert_eq!(error.code, ErrorCode::InvalidProtocol);
    }
}

#[test]
fn rejects_a_duplicate_edge() {
    let error = err(&chart(
        "root",
        vec![named("a", "甲"), named("root", "主体")],
        vec![
            Edge {
                ratio: Some(40.0),
                ..Edge::new("a", "root")
            },
            Edge {
                ratio: Some(60.0),
                ..Edge::new("a", "root")
            },
        ],
    ));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert!(
        error.message.contains("duplicate edge"),
        "{}",
        error.message
    );
}

#[test]
fn rejects_ambiguous_parent_edges() {
    let error = err(&chart(
        "root",
        vec![
            named("root", "主体"),
            named("a", "甲"),
            named("b", "乙"),
            named("d", "合资"),
        ],
        vec![
            Edge::new("root", "a"),
            Edge::new("root", "b"),
            Edge {
                ratio: Some(40.0),
                ..Edge::new("a", "d")
            },
            Edge {
                ratio: Some(60.0),
                ..Edge::new("b", "d")
            },
        ],
    ));
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert!(
        error.message.contains("ambiguous parent edges"),
        "{}",
        error.message
    );
}

#[test]
fn accepts_a_diamond_when_the_node_sets_its_ratio() {
    let request = chart(
        "root",
        vec![
            named("root", "主体"),
            named("a", "甲"),
            named("b", "乙"),
            Node {
                ratio: Some(50.0),
                ..named("d", "合资")
            },
        ],
        vec![
            Edge::new("root", "a"),
            Edge::new("root", "b"),
            Edge::new("a", "d"),
            Edge::new("b", "d"),
        ],
    );
    assert_eq!(validate_request(&request), Ok(()));
}
