//! `Chart::title` is `{subject name}股权架构图`.

use pdfcraft_orgchart::{Chart, ChartRequest, ErrorCode, Node};

#[test]
fn title_copies_the_stored_subject_name() {
    let chart = Chart::new(ChartRequest::new(
        "root",
        vec![Node::new("root", "华创")],
        vec![],
    ))
    .expect("chart");
    assert_eq!(chart.subject_name(), "华创");
    assert_eq!(chart.title(), "华创股权架构图");
}

#[test]
fn a_missing_subject_is_subject_not_found() {
    let mut request = ChartRequest::new("root", vec![Node::new("root", "华创")], vec![]);
    request.subject_id = "ghost".to_owned();
    let error = Chart::new(request).expect_err("missing");
    assert_eq!(error.code(), ErrorCode::SubjectNotFound);
    assert_eq!(error.code().as_str(), "SUBJECT_NOT_FOUND");
}

#[test]
fn a_blank_subject_is_invalid_protocol() {
    let request = ChartRequest::new("   ", vec![Node::new("root", "华创")], vec![]);
    let error = Chart::new(request).expect_err("blank id");
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
}

#[test]
fn a_blank_name_is_invalid_protocol() {
    let request = ChartRequest::new("root", vec![Node::new("root", "")], vec![]);
    let error = Chart::new(request).expect_err("blank name");
    assert_eq!(error.code(), ErrorCode::InvalidProtocol);
}
