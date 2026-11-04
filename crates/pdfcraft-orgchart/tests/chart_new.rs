//! Protocol failures and accepted graphs, through `Chart::new`.

#![expect(
    clippy::expect_used,
    reason = "integration test: a panic is the failure report"
)]

use std::num::NonZeroUsize;

use pdfcraft_orgchart::{Chart, ChartRequest, Edge, Error, ErrorCode, ErrorSubject, Node};

fn nz(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("nonzero")
}

fn node(id: &str) -> Node {
    Node::new(id, id)
}

fn named(id: &str, name: &str) -> Node {
    Node::new(id, name)
}

fn chart(subject: &str, nodes: Vec<Node>, edges: Vec<Edge>) -> ChartRequest {
    ChartRequest::new(subject, nodes, edges)
}

fn err(request: &ChartRequest) -> Error {
    Chart::new(request.clone()).expect_err("request should fail")
}

fn accept(request: &ChartRequest) {
    let _chart = Chart::new(request.clone()).expect("chart");
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
    accept(&request);
}

#[test]
fn rejects_a_missing_subject() {
    let error = err(&chart("missing", vec![node("a")], vec![]));
    assert_eq!(error.code(), ErrorCode::SubjectNotFound);
    assert_eq!(error.code().as_str(), "SUBJECT_NOT_FOUND");
    assert_eq!(error.subject(), Some(&ErrorSubject::Field("subject_id")));
}

#[test]
fn rejects_a_cycle() {
    let error = err(&chart(
        "a",
        vec![node("a"), node("b")],
        vec![Edge::new("a", "b"), Edge::new("b", "a")],
    ));
    assert_eq!(error.code(), ErrorCode::CycleDetected);
    assert_eq!(error.code().as_str(), "CYCLE_DETECTED");
    assert_eq!(
        error.subject(),
        Some(&ErrorSubject::Edge {
            index: 1,
            holder: "b".to_owned(),
            held: "a".to_owned(),
        })
    );
}

#[test]
fn rejects_a_dangling_edge() {
    let error = err(&chart("a", vec![node("a")], vec![Edge::new("a", "ghost")]));
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert!(
        error.message().contains("dangling edge"),
        "{}",
        error.message()
    );
    assert_eq!(
        error.subject(),
        Some(&ErrorSubject::Edge {
            index: 0,
            holder: "a".to_owned(),
            held: "ghost".to_owned(),
        })
    );
}

#[test]
fn rejects_a_self_loop() {
    let error = err(&chart("a", vec![node("a")], vec![Edge::new("a", "a")]));
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert!(error.message().contains("self-loop"), "{}", error.message());
    assert_eq!(
        error.subject(),
        Some(&ErrorSubject::Edge {
            index: 0,
            holder: "a".to_owned(),
            held: "a".to_owned(),
        })
    );
}

#[test]
fn rejects_a_blank_subject() {
    let error = err(&chart("   ", vec![node("a")], vec![]));
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert_eq!(error.code().as_str(), "INVALID_PROTOCOL");
    assert_eq!(error.subject(), Some(&ErrorSubject::Field("subject_id")));
}

#[test]
fn rejects_a_blank_node_id_or_name() {
    let blank_id = err(&chart("a", vec![Node::new("  ", "x")], vec![]));
    assert_eq!(blank_id.code(), ErrorCode::InvalidProtocol);
    assert_eq!(
        blank_id.subject(),
        Some(&ErrorSubject::Node {
            index: 0,
            id: "  ".to_owned(),
        })
    );
    let blank_name = err(&chart("a", vec![Node::new("a", "  ")], vec![]));
    assert_eq!(blank_name.code(), ErrorCode::InvalidProtocol);
    assert_eq!(
        blank_name.subject(),
        Some(&ErrorSubject::Node {
            index: 0,
            id: "a".to_owned(),
        })
    );
}

#[test]
fn rejects_a_duplicate_node_id() {
    let error = err(&chart("a", vec![node("a"), node("a")], vec![]));
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert!(
        error.message().contains("duplicate node id"),
        "{}",
        error.message()
    );
    assert_eq!(
        error.subject(),
        Some(&ErrorSubject::Node {
            index: 1,
            id: "a".to_owned(),
        })
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
    accept(&request);
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
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert!(
        error
            .message()
            .contains("edge crosses shareholder/investment cones: a->b"),
        "{}",
        error.message()
    );
    assert_eq!(
        error.subject(),
        Some(&ErrorSubject::Edge {
            index: 2,
            holder: "a".to_owned(),
            held: "b".to_owned(),
        })
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
    accept(&request);
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
    assert_eq!(error.code(), ErrorCode::CycleDetected);
}

#[test]
fn accepts_an_overflow_id() {
    let reserved = "__overflow__:root:a";
    accept(&chart(
        "root",
        vec![node("root"), node(reserved)],
        vec![Edge::new(reserved, "root")],
    ));
}

#[test]
fn other_prefix_without_folded_holders_is_a_normal_node() {
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
    accept(&request);
}

#[test]
fn rejects_an_isolated_node() {
    let error = err(&chart(
        "root",
        vec![node("a"), node("root"), node("orphan")],
        vec![Edge::new("a", "root")],
    ));
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert!(error.message().contains("orphan"), "{}", error.message());
    assert_eq!(
        error.subject(),
        Some(&ErrorSubject::Node {
            index: 2,
            id: "orphan".to_owned(),
        })
    );
}

#[test]
fn rejects_a_component_that_does_not_touch_the_subject() {
    let error = err(&chart(
        "root",
        vec![node("root"), node("x"), node("y")],
        vec![Edge::new("x", "y")],
    ));
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert_eq!(
        error.subject(),
        Some(&ErrorSubject::Node {
            index: 1,
            id: "x".to_owned(),
        })
    );
}

#[test]
fn rejects_a_sibling_that_only_shares_an_ancestor() {
    let error = err(&chart(
        "root",
        vec![node("a"), node("root"), node("sibling")],
        vec![Edge::new("a", "root"), Edge::new("a", "sibling")],
    ));
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert!(error.message().contains("sibling"), "{}", error.message());
}

#[test]
fn accepts_a_subject_only_graph() {
    accept(&chart("root", vec![node("root")], vec![]));
}

#[test]
fn rejects_too_many_nodes() {
    let nodes: Vec<Node> = (1..=101).map(|index| node(&format!("n{index}"))).collect();
    let error = err(&chart("n1", nodes, vec![]));
    assert_eq!(error.code(), ErrorCode::TooManyNodes);
    assert_eq!(error.code().as_str(), "TOO_MANY_NODES");
    assert_eq!(error.subject(), Some(&ErrorSubject::Field("nodes")));
}

#[test]
fn rejects_a_lower_node_cap() {
    let mut request = chart(
        "root",
        vec![node("root"), node("a"), node("b")],
        vec![Edge::new("a", "root"), Edge::new("root", "b")],
    );
    request.max_nodes = 2;
    let error = err(&request);
    assert_eq!(error.code(), ErrorCode::TooManyNodes);
    assert_eq!(error.subject(), Some(&ErrorSubject::Field("nodes")));
}

#[test]
fn rejects_max_nodes_above_the_hard_limit() {
    let mut request = chart("root", vec![node("root")], vec![]);
    request.max_nodes = 101;
    let error = err(&request);
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert!(error.message().contains("max_nodes"), "{}", error.message());
    assert_eq!(error.subject(), Some(&ErrorSubject::Field("max_nodes")));
}

#[test]
fn rejects_top_n_above_the_cap_and_accepts_equality() {
    let mut over = chart("root", vec![node("root")], vec![]);
    over.max_nodes = 4;
    over.top_n = Some(nz(5));
    let error = err(&over);
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert!(error.message().contains("0..="), "{}", error.message());
    assert_eq!(error.subject(), Some(&ErrorSubject::Field("top_n")));
    let mut equal = chart("root", vec![node("root")], vec![]);
    equal.max_nodes = 1;
    equal.top_n = Some(nz(1));
    accept(&equal);
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
        assert_eq!(node_error.code(), ErrorCode::InvalidProtocol);
        assert_eq!(
            node_error.subject(),
            Some(&ErrorSubject::Node {
                index: 0,
                id: "a".to_owned(),
            })
        );
        let edge_error = err(&chart(
            "root",
            vec![named("a", "甲"), root.clone()],
            vec![Edge {
                ratio: Some(bad),
                ..Edge::new("a", "root")
            }],
        ));
        assert_eq!(edge_error.code(), ErrorCode::InvalidProtocol);
        assert_eq!(
            edge_error.subject(),
            Some(&ErrorSubject::Edge {
                index: 0,
                holder: "a".to_owned(),
                held: "root".to_owned(),
            })
        );
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
    request.style.margin_mm = 40.0;
    request.style.node_width_mm = 20.0;
    accept(&request);
    request.style.node_width_mm = 80.0;
    accept(&request);
}

#[test]
fn rejects_illegal_margin_and_node_width() {
    let root = vec![node("root")];
    for margin in [0.0, 40.01, f64::NAN, f64::NEG_INFINITY] {
        let mut request = chart("root", root.clone(), vec![]);
        request.style.margin_mm = margin;
        let error = err(&request);
        assert_eq!(error.code(), ErrorCode::InvalidProtocol);
        assert_eq!(
            error.subject(),
            Some(&ErrorSubject::Field("style.margin_mm"))
        );
    }
    for width in [19.99, 80.01, f64::NAN] {
        let mut request = chart("root", root.clone(), vec![]);
        request.style.node_width_mm = width;
        let error = err(&request);
        assert_eq!(error.code(), ErrorCode::InvalidProtocol);
        assert_eq!(
            error.subject(),
            Some(&ErrorSubject::Field("style.node_width_mm"))
        );
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
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert!(
        error.message().contains("duplicate edge"),
        "{}",
        error.message()
    );
    assert_eq!(
        error.subject(),
        Some(&ErrorSubject::Edge {
            index: 1,
            holder: "a".to_owned(),
            held: "root".to_owned(),
        })
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
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert!(
        error.message().contains("ambiguous parent edges"),
        "{}",
        error.message()
    );
    assert_eq!(
        error.subject(),
        Some(&ErrorSubject::Node {
            index: 3,
            id: "d".to_owned(),
        })
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
    accept(&request);
}

#[test]
fn folded_holders_of_must_name_one_parent_with_a_direct_edge() {
    let dangling = chart(
        "root",
        vec![
            Node {
                folded_holders_of: Some("missing".to_owned()),
                ..named("other", "其他")
            },
            named("root", "主体"),
        ],
        vec![Edge::new("other", "root")],
    );
    let dangling_error = err(&dangling);
    assert_eq!(dangling_error.code(), ErrorCode::InvalidProtocol);
    assert_eq!(
        dangling_error.subject(),
        Some(&ErrorSubject::Node {
            index: 0,
            id: "other".to_owned(),
        })
    );

    let mut first = named("a", "甲");
    first.folded_holders_of = Some("root".to_owned());
    let mut second = named("b", "乙");
    second.folded_holders_of = Some("root".to_owned());
    let duplicate = chart(
        "root",
        vec![first, second, named("root", "主体")],
        vec![Edge::new("a", "root"), Edge::new("b", "root")],
    );
    let duplicate_error = err(&duplicate);
    assert_eq!(duplicate_error.code(), ErrorCode::InvalidProtocol);
    assert_eq!(
        duplicate_error.subject(),
        Some(&ErrorSubject::Node {
            index: 1,
            id: "b".to_owned(),
        })
    );

    let unlinked = chart(
        "root",
        vec![
            Node {
                folded_holders_of: Some("root".to_owned()),
                ..named("other", "其他")
            },
            named("s1", "甲"),
            named("root", "主体"),
        ],
        vec![Edge::new("other", "s1"), Edge::new("s1", "root")],
    );
    let unlinked_error = err(&unlinked);
    assert_eq!(unlinked_error.code(), ErrorCode::InvalidProtocol);
    assert_eq!(
        unlinked_error.subject(),
        Some(&ErrorSubject::Node {
            index: 0,
            id: "other".to_owned(),
        })
    );

    let mut stand_in = named("other", "其他股东(1家)");
    stand_in.ratio = Some(10.0);
    stand_in.folded_holders_of = Some("root".to_owned());
    accept(&chart(
        "root",
        vec![stand_in, named("s1", "甲"), named("root", "主体")],
        vec![
            Edge {
                ratio: Some(10.0),
                ..Edge::new("other", "root")
            },
            Edge {
                ratio: Some(90.0),
                ..Edge::new("s1", "root")
            },
        ],
    ));
}

#[test]
fn a_fold_that_leaves_two_parent_edges_is_invalid_protocol() {
    let mut request = chart(
        "root",
        vec![
            Node {
                ratio: Some(40.0),
                ..named("a", "甲")
            },
            Node {
                ratio: Some(30.0),
                ..named("b", "乙")
            },
            named("x", "丙"),
            named("root", "主体"),
        ],
        vec![
            Edge {
                ratio: Some(40.0),
                ..Edge::new("a", "root")
            },
            Edge {
                ratio: Some(30.0),
                ..Edge::new("b", "root")
            },
            Edge::new("x", "root"),
            Edge::new("x", "a"),
            Edge::new("x", "b"),
        ],
    );
    request.top_n = Some(nz(2));
    let error = err(&request);
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert_eq!(
        error.subject(),
        Some(&ErrorSubject::Node {
            index: 2,
            id: "x".to_owned(),
        })
    );
}
