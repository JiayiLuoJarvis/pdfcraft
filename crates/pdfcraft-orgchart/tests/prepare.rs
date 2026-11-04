//! `Chart::new` folds `top_n` and rechecks the folded node cap.

#![expect(
    clippy::expect_used,
    reason = "integration test: a panic is the failure report"
)]

use std::num::NonZeroUsize;

use pdfcraft_orgchart::{Chart, ChartRequest, Edge, ErrorCode, ErrorSubject, Node};

fn nz(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("nonzero")
}

fn holders(top_n: Option<NonZeroUsize>) -> ChartRequest {
    let mut request = ChartRequest::new(
        "root",
        vec![
            Node {
                ratio: Some(40.0),
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
            Node {
                ratio: Some(10.0),
                ..Node::new("s4", "丁")
            },
            Node::new("root", "主体"),
        ],
        vec![
            Edge {
                ratio: Some(40.0),
                ..Edge::new("s1", "root")
            },
            Edge {
                ratio: Some(30.0),
                ..Edge::new("s2", "root")
            },
            Edge {
                ratio: Some(20.0),
                ..Edge::new("s3", "root")
            },
            Edge {
                ratio: Some(10.0),
                ..Edge::new("s4", "root")
            },
        ],
    );
    request.top_n = top_n;
    request
}

#[test]
fn top_n_none_passes_and_a_blank_subject_fails() {
    let _chart = Chart::new(holders(None)).expect("chart");
    let mut blank = holders(None);
    blank.subject_id = "   ".to_owned();
    let error = Chart::new(blank).expect_err("blank subject");
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert_eq!(error.subject(), Some(&ErrorSubject::Field("subject_id")));
}

#[test]
fn a_missing_shareholder_ratio_fails_when_top_n_is_set() {
    let mut request = ChartRequest::new(
        "root",
        vec![Node::new("s1", "甲"), Node::new("root", "主体")],
        vec![Edge::new("s1", "root")],
    );
    request.top_n = Some(nz(5));
    let error = Chart::new(request).expect_err("missing ratio");
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
    assert!(
        error.message().contains("top_n requires"),
        "{}",
        error.message()
    );
    assert_eq!(
        error.subject(),
        Some(&ErrorSubject::Node {
            index: 0,
            id: "s1".to_owned(),
        })
    );
}

#[test]
fn a_fold_past_max_nodes_is_too_many_nodes() {
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
            Edge {
                ratio: Some(50.0),
                ..Edge::new("s1", "root")
            },
            Edge {
                ratio: Some(30.0),
                ..Edge::new("s2", "root")
            },
            Edge {
                ratio: Some(20.0),
                ..Edge::new("s3", "root")
            },
            Edge {
                ratio: Some(20.0),
                ..Edge::new("s3", "s1")
            },
        ],
    );
    request.max_nodes = 4;
    request.top_n = Some(nz(2));
    let error = Chart::new(request).expect_err("folded cap");
    assert_eq!(error.code(), ErrorCode::TooManyNodes);
    assert_eq!(error.subject(), Some(&ErrorSubject::Field("nodes")));
}

#[test]
fn folding_four_holders_down_to_top_n_passes() {
    let _chart = Chart::new(holders(Some(nz(2)))).expect("chart");
}
