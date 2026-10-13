//! In-box lines and ratio facts. Width and paging stay in later tests.

use quire_orgchart::display::{
    build_display, company_fill, ratio_facts, DisplayRole, LabelHug, LineKind,
};
use quire_orgchart::{ChartRequest, Edge, Node, NodeType};

fn cone() -> ChartRequest {
    let mut root = Node::new("root", "主体");
    root.legal_person = Some("李四".to_owned());
    root.ratio = Some(10.0);
    root.paid_in_amount = Some("1万".to_owned());
    root.currency = Some("USD".to_owned());
    root.location = Some("上海".to_owned());
    root.approval_date = Some("2020-01-01".to_owned());

    let mut holder = Node::new("s", "股东甲");
    holder.legal_person = Some("  王五  ".to_owned());
    holder.ratio = Some(40.0);
    holder.paid_in_amount = Some("9万".to_owned());
    holder.currency = Some("HKD".to_owned());
    holder.location = Some("北京".to_owned());
    holder.approval_date = Some("2019-01-01".to_owned());

    let mut child = Node::new("c", "子公司");
    child.legal_person = Some("赵六".to_owned());
    child.paid_in_amount = Some("100万".to_owned());
    child.currency = Some("CNY".to_owned());
    child.location = Some("  深圳  ".to_owned());
    child.approval_date = Some("2024-06-01".to_owned());

    ChartRequest::new(
        "root",
        vec![root, holder, child],
        vec![
            Edge {
                ratio: Some(40.0),
                ..Edge::new("s", "root")
            },
            Edge {
                ratio: Some(51.2),
                ..Edge::new("root", "c")
            },
        ],
    )
}

#[test]
#[allow(clippy::float_cmp)]
fn cones_keep_their_own_fields_and_leave_ratio_out_of_the_box() {
    let request = cone();
    let display = build_display(&request).expect("display");
    let root = display.get("root").expect("root");
    assert_eq!(root.role, DisplayRole::Subject);
    assert_eq!(root.role.as_str(), "SUBJECT");
    assert_eq!(
        root.lines
            .iter()
            .map(|line| (line.kind, line.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(LineKind::Name, "主体"), (LineKind::LegalPerson, "李四"),]
    );
    let holder = display.get("s").expect("holder");
    assert_eq!(holder.role.as_str(), "SHAREHOLDER");
    assert_eq!(
        holder
            .lines
            .iter()
            .map(|line| (line.kind, line.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(LineKind::Name, "股东甲"), (LineKind::LegalPerson, "王五"),]
    );
    let child = display.get("c").expect("child");
    assert_eq!(child.role.as_str(), "INVESTMENT");
    assert_eq!(
        child
            .lines
            .iter()
            .map(|line| (line.kind, line.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (LineKind::Name, "子公司"),
            (LineKind::Detail, "实缴 100万 CNY"),
            (LineKind::Detail, "深圳"),
            (LineKind::Detail, "2024-06-01"),
        ]
    );
    for content in display.nodes() {
        for line in &content.lines {
            assert!(!line.text.contains("法定代表人"));
            assert!(!line.text.contains('%'));
        }
    }
    assert!(holder.lines.iter().all(|line| !line.text.contains("实缴")));
    assert!(child.lines.iter().all(|line| !line.text.contains("赵六")));

    let facts = ratio_facts(&request).expect("facts");
    let by_id: std::collections::HashMap<_, _> = facts
        .iter()
        .map(|fact| (fact.node_id.as_str(), fact))
        .collect();
    assert_eq!(
        by_id
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["s", "c"])
    );
    let holder_fact = by_id["s"];
    assert_eq!(holder_fact.text, "40.00%");
    assert_eq!(holder_fact.ratio, 40.0);
    assert_eq!(holder_fact.hug, LabelHug::Bottom);
    assert_eq!(holder_fact.hug.as_str(), "BOTTOM");
    assert!(holder_fact.preferred_parent.is_none());
    let child_fact = by_id["c"];
    assert_eq!(child_fact.text, "51.20%");
    assert_eq!(child_fact.hug, LabelHug::Top);
    let parent = child_fact.preferred_parent.as_ref().expect("parent edge");
    assert_eq!(parent.from_id, "root");
    assert_eq!(parent.to_id, "c");
}

#[test]
fn currency_without_amount_and_blank_amount_add_no_detail_line() {
    let mut only_currency = Node::new("c1", "只有币种");
    only_currency.currency = Some("CNY".to_owned());
    only_currency.legal_person = Some("不应出现".to_owned());
    let mut blank_paid = Node::new("c2", "空白实缴");
    blank_paid.paid_in_amount = Some("   ".to_owned());
    blank_paid.location = Some("  ".to_owned());
    let request = ChartRequest::new(
        "root",
        vec![Node::new("root", "主体"), only_currency, blank_paid],
        vec![
            Edge {
                ratio: Some(10.0),
                ..Edge::new("root", "c1")
            },
            Edge {
                ratio: Some(0.0),
                ..Edge::new("root", "c2")
            },
        ],
    );
    let display = build_display(&request).expect("display");
    let texts = |id: &str| {
        display
            .get(id)
            .expect(id)
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(texts("c1"), ["只有币种"]);
    assert_eq!(texts("c2"), ["空白实缴"]);
    let facts = ratio_facts(&request).expect("facts");
    let by_id: std::collections::HashMap<_, _> = facts
        .iter()
        .map(|fact| (fact.node_id.as_str(), fact.text.as_str()))
        .collect();
    assert_eq!(by_id["c2"], "0.00%");
    assert!(!by_id.contains_key("root"));
}

#[test]
fn a_person_stays_white_and_a_company_is_gray() {
    let mut person = Node::new("p", "张三");
    person.node_type = Some(NodeType::Person);
    person.legal_person = Some("不应变灰".to_owned());
    assert!(!company_fill(&person));

    let mut company = Node::new("co", "甲公司");
    company.node_type = Some(NodeType::Company);
    assert!(company_fill(&company));

    let mut implied = Node::new("q", "乙");
    implied.legal_person = Some("  王五  ".to_owned());
    assert!(company_fill(&implied));

    let blank = Node::new("r", "丙");
    assert!(!company_fill(&blank));
}
