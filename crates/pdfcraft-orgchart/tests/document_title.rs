//! `document_title` reads the subject name and the optional caller title.

use pdfcraft_orgchart::{document_title, ChartRequest, ErrorCode, Node};

fn request(subject: &str, name: &str, title: Option<&str>) -> ChartRequest {
    let mut request = ChartRequest::new(subject, vec![Node::new(subject, name)], vec![]);
    request.options.title = title.map(str::to_owned);
    request
}

#[test]
fn none_uses_the_subject_name() {
    let title = document_title(&request("root", "华创", None)).expect("title");
    assert_eq!(title, "华创股权架构图");
}

#[test]
fn an_explicit_title_is_kept() {
    let title = document_title(&request("root", "华创", Some("自定义标题"))).expect("title");
    assert_eq!(title, "自定义标题");
}

#[test]
fn an_empty_title_stays_empty() {
    let title = document_title(&request("root", "华创", Some(""))).expect("title");
    assert_eq!(title, "");
}

#[test]
fn a_missing_subject_is_subject_not_found() {
    let mut request = request("root", "华创", None);
    request.subject_id = "ghost".to_owned();
    let error = document_title(&request).expect_err("missing");
    assert_eq!(error.code, ErrorCode::SubjectNotFound);
    assert_eq!(error.code.as_str(), "SUBJECT_NOT_FOUND");
}

#[test]
fn a_blank_subject_is_not_a_protocol_error() {
    let request = ChartRequest::new("   ", vec![Node::new("root", "华创")], vec![]);
    let error = document_title(&request).expect_err("blank id");
    assert_eq!(error.code, ErrorCode::SubjectNotFound);
}

#[test]
fn a_blank_name_is_copied_as_stored() {
    let title = document_title(&request("root", "", None)).expect("title");
    assert_eq!(title, "股权架构图");
}

#[test]
fn the_first_matching_node_wins() {
    let mut request = ChartRequest::new(
        "root",
        vec![Node::new("root", "先"), Node::new("root", "后")],
        vec![],
    );
    request.options.title = None;
    assert_eq!(document_title(&request).expect("title"), "先股权架构图");
}
