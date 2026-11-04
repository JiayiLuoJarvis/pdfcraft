//! In-box lines and ratio facts. Width and paging stay in later tests.

use super::{build_display, company_fill, ratio_facts, DisplayRole, LabelHug, LineKind};
use crate::graph;
use crate::{ChartRequest, Edge, Node, NodeType, PaidIn};

fn edge(from: &str, to: &str, ratio: f64) -> Edge {
    Edge {
        ratio: Some(ratio),
        ..Edge::new(from, to)
    }
}

fn id_of(request: &ChartRequest, protocol: &str) -> graph::NodeId {
    let graph = graph::validate(request).expect("valid");
    graph.id_of(protocol).expect(protocol)
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "the assertion checks that a stored ratio was copied unchanged"
)]
fn cones_keep_their_fields_and_leave_ratio_out_of_the_box() {
    let request = cone();
    let display = build_display(&request).expect("display");
    let pairs = |protocol: &str| {
        let id = id_of(&request, protocol);
        display
            .get(id)
            .expect(protocol)
            .lines
            .iter()
            .map(|line| (line.kind, line.text.as_str()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        pairs("root"),
        vec![(LineKind::Name, "主体"), (LineKind::LegalPerson, "李四"),]
    );
    assert_eq!(
        pairs("s"),
        vec![(LineKind::Name, "股东甲"), (LineKind::LegalPerson, "王五"),]
    );
    assert_eq!(
        pairs("c"),
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
    let s = id_of(&request, "s");
    assert!(display
        .get(s)
        .expect("s")
        .lines
        .iter()
        .all(|line| !line.text.contains("实缴")));
    let c = id_of(&request, "c");
    assert!(display
        .get(c)
        .expect("c")
        .lines
        .iter()
        .all(|line| !line.text.contains("赵六")));
    let facts = ratio_facts(&request).expect("facts");
    let holder = facts
        .iter()
        .find(|fact| fact.node_id == id_of(&request, "s"))
        .expect("s");
    assert_eq!(holder.text, "40.00%");
    assert_eq!(holder.ratio, 40.0);
    assert_eq!(holder.hug, LabelHug::Bottom);
    assert!(holder.preferred_parent.is_none());
    let child = facts.iter().find(|fact| fact.node_id == c).expect("c");
    assert_eq!(child.text, "51.20%");
    assert_eq!(child.hug, LabelHug::Top);
    let parent = child.preferred_parent.as_ref().expect("edge");
    let root = id_of(&request, "root");
    assert_eq!(parent.from_id, root);
    assert_eq!(parent.to_id, c);
    assert!(facts.iter().all(|fact| fact.node_id != root));
}

#[test]
fn blank_paid_in_adds_no_detail_line() {
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("root", "主体"),
            Node {
                legal_person: Some("不应出现".to_owned()),
                ..Node::new("c1", "只有名称")
            },
            Node {
                paid_in: Some(PaidIn {
                    amount: "   ".to_owned(),
                    currency: Some("   ".to_owned()),
                }),
                location: Some("  ".to_owned()),
                ..Node::new("c2", "空白实缴")
            },
        ],
        vec![edge("root", "c1", 10.0), edge("root", "c2", 0.0)],
    );
    let display = build_display(&request).expect("display");
    let c1 = id_of(&request, "c1");
    assert_eq!(
        display
            .get(c1)
            .expect("c1")
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        vec!["只有名称"]
    );
    let c2 = id_of(&request, "c2");
    assert_eq!(
        display
            .get(c2)
            .expect("c2")
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        vec!["空白实缴"]
    );
    let facts = ratio_facts(&request).expect("facts");
    assert_eq!(
        facts
            .iter()
            .find(|fact| fact.node_id == c2)
            .expect("c2")
            .text,
        "0.00%"
    );
    let root = id_of(&request, "root");
    assert!(facts.iter().all(|fact| fact.node_id != root));
}

#[test]
fn person_is_white_and_company_is_gray() {
    assert!(!company_fill(&Node {
        node_type: Some(NodeType::Person),
        legal_person: Some("张三".to_owned()),
        ..Node::new("p", "张三")
    }));
    assert!(company_fill(&Node {
        node_type: Some(NodeType::Company),
        ..Node::new("c", "公司")
    }));
    assert!(company_fill(&Node {
        legal_person: Some("李四".to_owned()),
        ..Node::new("u", "未标")
    }));
    assert!(!company_fill(&Node {
        legal_person: Some("  ".to_owned()),
        ..Node::new("b", "空白")
    }));
}

fn cone() -> ChartRequest {
    let mut root = Node::new("root", "主体");
    root.legal_person = Some("李四".to_owned());
    root.ratio = Some(10.0);
    root.paid_in = Some(PaidIn {
        amount: "1万".to_owned(),
        currency: Some("USD".to_owned()),
    });
    root.location = Some("上海".to_owned());
    root.approval_date = Some("2020-01-01".to_owned());

    let mut holder = Node::new("s", "股东甲");
    holder.legal_person = Some("  王五  ".to_owned());
    holder.ratio = Some(40.0);
    holder.paid_in = Some(PaidIn {
        amount: "9万".to_owned(),
        currency: Some("HKD".to_owned()),
    });
    holder.location = Some("北京".to_owned());
    holder.approval_date = Some("2019-01-01".to_owned());

    let mut child = Node::new("c", "子公司");
    child.legal_person = Some("赵六".to_owned());
    child.paid_in = Some(PaidIn {
        amount: "100万".to_owned(),
        currency: Some("CNY".to_owned()),
    });
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

fn nid(request: &ChartRequest, protocol: &str) -> graph::NodeId {
    graph::validate(request)
        .expect("valid")
        .id_of(protocol)
        .expect(protocol)
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "the assertion checks that a stored ratio was copied unchanged"
)]
fn cones_keep_their_own_fields_and_leave_ratio_out_of_the_box() {
    let request = cone();
    let display = build_display(&request).expect("display");
    let root = display.get(nid(&request, "root")).expect("root");
    assert_eq!(root.role, DisplayRole::Subject);
    assert_eq!(
        root.lines
            .iter()
            .map(|line| (line.kind, line.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(LineKind::Name, "主体"), (LineKind::LegalPerson, "李四"),]
    );
    let holder = display.get(nid(&request, "s")).expect("holder");
    assert_eq!(holder.role, DisplayRole::Shareholder);
    assert_eq!(
        holder
            .lines
            .iter()
            .map(|line| (line.kind, line.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(LineKind::Name, "股东甲"), (LineKind::LegalPerson, "王五"),]
    );
    let child = display.get(nid(&request, "c")).expect("child");
    assert_eq!(child.role, DisplayRole::Investment);
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
    let graph = graph::validate(&request).expect("valid");
    let by_id: std::collections::HashMap<_, _> = facts
        .iter()
        .map(|fact| (graph.label(fact.node_id), fact))
        .collect();
    assert_eq!(
        by_id
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["s", "c"])
    );
    let holder_fact = by_id["s"];
    assert_eq!(holder_fact.text, "40.00%");
    assert_eq!(holder_fact.ratio, 40.0);
    assert_eq!(holder_fact.hug, LabelHug::Bottom);
    assert!(holder_fact.preferred_parent.is_none());
    let child_fact = by_id["c"];
    assert_eq!(child_fact.text, "51.20%");
    assert_eq!(child_fact.hug, LabelHug::Top);
    let parent = child_fact.preferred_parent.as_ref().expect("parent edge");
    assert_eq!(parent.from_id, nid(&request, "root"));
    assert_eq!(parent.to_id, nid(&request, "c"));
}

#[test]
fn a_blank_paid_in_amount_adds_no_detail_line() {
    let mut name_only = Node::new("c1", "只有名称");
    name_only.legal_person = Some("不应出现".to_owned());
    let mut blank_paid = Node::new("c2", "空白实缴");
    blank_paid.paid_in = Some(PaidIn {
        amount: "   ".to_owned(),
        currency: Some("CNY".to_owned()),
    });
    blank_paid.location = Some("  ".to_owned());
    let request = ChartRequest::new(
        "root",
        vec![Node::new("root", "主体"), name_only, blank_paid],
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
    let texts = |protocol: &str| {
        display
            .get(nid(&request, protocol))
            .expect(protocol)
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(texts("c1"), ["只有名称"]);
    assert_eq!(texts("c2"), ["空白实缴"]);
    let facts = ratio_facts(&request).expect("facts");
    let graph = graph::validate(&request).expect("valid");
    let by_id: std::collections::HashMap<_, _> = facts
        .iter()
        .map(|fact| (graph.label(fact.node_id), fact.text.as_str()))
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
