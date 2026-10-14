//! `validate_request` surfaces `top_n` fold failures and does not recheck the folded cap.

use pdfcraft_orgchart::{validate_request, ChartRequest, Edge, ErrorCode, Node};

fn holders(top_n: Option<usize>) -> ChartRequest {
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
    request.options.top_n = top_n;
    request
}

#[test]
fn top_n_none_and_zero_pass_and_still_validate() {
    assert_eq!(validate_request(&holders(None)), Ok(()));
    assert_eq!(validate_request(&holders(Some(0))), Ok(()));
    let mut blank = holders(Some(0));
    blank.subject_id = "   ".to_owned();
    let error = validate_request(&blank).expect_err("blank subject");
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
}

#[test]
fn a_missing_shareholder_ratio_fails_when_top_n_is_set() {
    let request = ChartRequest {
        options: pdfcraft_orgchart::ChartOptions {
            top_n: Some(5),
            ..pdfcraft_orgchart::ChartOptions::default()
        },
        ..ChartRequest::new(
            "root",
            vec![Node::new("s1", "甲"), Node::new("root", "主体")],
            vec![Edge::new("s1", "root")],
        )
    };
    let error = validate_request(&request).expect_err("missing ratio");
    assert_eq!(error.code, ErrorCode::InvalidProtocol);
    assert!(
        error.message.contains("top_n requires"),
        "{}",
        error.message
    );
}

#[test]
fn top_n_zero_does_not_require_a_display_ratio() {
    let mut request = ChartRequest::new(
        "root",
        vec![Node::new("s1", "甲"), Node::new("root", "主体")],
        vec![Edge::new("s1", "root")],
    );
    request.options.top_n = Some(0);
    assert_eq!(validate_request(&request), Ok(()));
}

#[test]
fn a_fold_that_grows_past_max_nodes_still_passes_validate_request() {
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
    request.options.max_nodes = Some(4);
    request.options.top_n = Some(2);
    assert_eq!(validate_request(&request), Ok(()));
}

#[test]
fn folding_four_holders_down_to_top_n_passes() {
    assert_eq!(validate_request(&holders(Some(2))), Ok(()));
}
