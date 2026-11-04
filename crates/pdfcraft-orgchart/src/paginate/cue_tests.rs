//! Continuation cues, overflow links, and attachment captions after final numbering.

use super::test_support::{chain, fake_pages, id_sets, set_of};
use crate::captions::CueSide;
use crate::page::{ContCue, NodePageLink, PageSpec};
use crate::{ChartRequest, Edge, Node};

#[test]
fn shallow_cones_pack_ahead_of_a_deeper_cone() {
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("root", "主体"),
            Node::new("deep", "深"),
            Node::new("d1", "上层"),
            Node::new("d2", "顶层"),
            Node::new("s1", "浅1"),
            Node::new("s2", "浅2"),
        ],
        vec![
            Edge::new("deep", "root"),
            Edge::new("d1", "deep"),
            Edge::new("d2", "d1"),
            Edge::new("s1", "root"),
            Edge::new("s2", "root"),
        ],
    );
    let pages = fake_pages(&request, 4).expect("pages");
    assert_eq!(id_sets(&pages)[0], set_of(&["root", "deep", "s1", "s2"]));
    assert_eq!(
        pages[0].cont_cues,
        vec![ContCue {
            target_page: 2,
            direct_count: 1,
            side: CueSide::Shareholder,
            anchor_id: "deep".to_owned(),
        }]
    );
    assert_eq!(id_sets(&pages)[1], set_of(&["deep", "d1", "d2"]));
}

#[test]
fn shareholder_pages_cue_the_next_direct_page_not_investments() {
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("root", "主体"),
            Node::new("s1", "股东1"),
            Node::new("s2", "股东2"),
            Node::new("s3", "股东3"),
            Node::new("s4", "股东4"),
            Node::new("v1", "投资"),
        ],
        vec![
            Edge::new("s1", "root"),
            Edge::new("s2", "root"),
            Edge::new("s3", "root"),
            Edge::new("s4", "root"),
            Edge::new("root", "v1"),
        ],
    );
    let pages = fake_pages(&request, 3).expect("pages");
    assert_eq!(
        id_sets(&pages),
        vec![
            set_of(&["root", "s1", "__overflow__:root:s2"]),
            set_of(&["root", "s2", "__overflow__:root:s3"]),
            set_of(&["root", "s3", "s4"]),
            set_of(&["root", "v1"]),
        ]
    );
    assert_eq!(
        pages[0].cont_cues,
        vec![ContCue {
            target_page: 4,
            direct_count: 1,
            side: CueSide::Investment,
            anchor_id: "root".to_owned(),
        }]
    );
    assert!(pages[1].cont_cues.is_empty());
    assert!(pages[2].cont_cues.is_empty());
    assert!(pages[3].cont_cues.is_empty());
    assert_eq!(
        pages[0].node_page_links,
        vec![NodePageLink {
            node_id: "__overflow__:root:s2".to_owned(),
            target_page: 2,
        }]
    );
    assert_eq!(
        pages[1].node_page_links,
        vec![NodePageLink {
            node_id: "__overflow__:root:s3".to_owned(),
            target_page: 3,
        }]
    );
    assert!(pages[2].node_page_links.is_empty());
    let overflow_text = |page: &PageSpec, id: &str| {
        page.boxes
            .iter()
            .find(|box_| box_.protocol_id == id)
            .and_then(|box_| box_.lines.last())
            .map(|line| line.text.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        overflow_text(&pages[0], "__overflow__:root:s2"),
        "详见附件2"
    );
    assert_eq!(
        overflow_text(&pages[1], "__overflow__:root:s3"),
        "详见附件3"
    );
}

#[test]
fn chain_cues_each_logical_attachment_after_final_numbering() {
    let pages = fake_pages(&chain(&["a1", "a2", "a3"], false), 2).expect("pages");
    assert_eq!(
        id_sets(&pages),
        vec![
            set_of(&["root", "a1"]),
            set_of(&["a1", "a2"]),
            set_of(&["a2", "a3"]),
        ]
    );
    assert_eq!(
        pages[0].cont_cues,
        vec![ContCue {
            target_page: 2,
            direct_count: 1,
            side: CueSide::Shareholder,
            anchor_id: "a1".to_owned(),
        }]
    );
    assert_eq!(
        pages[1].cont_cues,
        vec![ContCue {
            target_page: 3,
            direct_count: 1,
            side: CueSide::Shareholder,
            anchor_id: "a2".to_owned(),
        }]
    );
    assert!(pages[2].cont_cues.is_empty());
    assert_eq!(
        pages
            .iter()
            .map(|page| page.sub_title.as_str())
            .collect::<Vec<_>>(),
        ["各股东/合伙人 → 主体", "名-a1持有主体", "名-a2持有名-a1",]
    );
    assert_eq!(
        pages
            .iter()
            .map(|page| page.main_title.as_str())
            .collect::<Vec<_>>(),
        ["股权架构图: 主体", "附件2: 名-a1", "附件3: 名-a2",]
    );
}

#[test]
fn cut_subtitle_uses_display_ratio_rounded_to_two_places() {
    let mut a1 = Node::new("a1", "名-a1");
    a1.ratio = Some(10.0);
    let mut a2 = Node::new("a2", "名-a2");
    a2.ratio = Some(80.5556);
    let mut edge_a1 = Edge::new("a1", "root");
    edge_a1.ratio = Some(10.0);
    let mut edge_a2 = Edge::new("a2", "a1");
    edge_a2.ratio = Some(80.5556);
    let request = ChartRequest::new(
        "root",
        vec![Node::new("root", "主体"), a1, a2, Node::new("a3", "名-a3")],
        vec![edge_a1, edge_a2, Edge::new("a3", "a2")],
    );
    let pages = fake_pages(&request, 2).expect("pages");
    assert_eq!(
        pages
            .iter()
            .map(|page| page.sub_title.as_str())
            .collect::<Vec<_>>(),
        [
            "各股东/合伙人 → 主体",
            "名-a1持有主体 10.00%",
            "名-a2持有名-a1 80.56%",
        ]
    );
    assert_eq!(
        pages
            .iter()
            .map(|page| page.main_title.as_str())
            .collect::<Vec<_>>(),
        ["股权架构图: 主体", "附件2: 名-a1", "附件3: 名-a2",]
    );
}

#[test]
fn completed_appendix_has_no_unanchored_sibling_cue() {
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("root", "主体"),
            Node::new("a", "族甲"),
            Node::new("a1", "甲上"),
            Node::new("a2", "甲顶"),
            Node::new("b", "族乙"),
            Node::new("b1", "乙上"),
            Node::new("b2", "乙顶"),
        ],
        vec![
            Edge::new("a", "root"),
            Edge::new("a1", "a"),
            Edge::new("a2", "a1"),
            Edge::new("b", "root"),
            Edge::new("b1", "b"),
            Edge::new("b2", "b1"),
        ],
    );
    let pages = fake_pages(&request, 3).expect("pages");
    assert_eq!(
        id_sets(&pages),
        vec![
            set_of(&["root", "a", "b"]),
            set_of(&["a", "a1", "a2"]),
            set_of(&["b", "b1", "b2"]),
        ]
    );
    assert_eq!(
        pages[0].cont_cues,
        vec![
            ContCue {
                target_page: 2,
                direct_count: 1,
                side: CueSide::Shareholder,
                anchor_id: "a".to_owned(),
            },
            ContCue {
                target_page: 3,
                direct_count: 1,
                side: CueSide::Shareholder,
                anchor_id: "b".to_owned(),
            },
        ]
    );
    assert!(pages[1].cont_cues.is_empty());
    assert!(pages[2].cont_cues.is_empty());
}

#[test]
fn completed_investment_appendix_has_no_unanchored_sibling_cue() {
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("root", "主体"),
            Node::new("s1", "股东"),
            Node::new("v", "投甲"),
            Node::new("v1", "甲下"),
            Node::new("v2", "甲叶"),
            Node::new("w", "投乙"),
            Node::new("w1", "乙下"),
            Node::new("w2", "乙叶"),
        ],
        vec![
            Edge::new("s1", "root"),
            Edge::new("root", "v"),
            Edge::new("v", "v1"),
            Edge::new("v1", "v2"),
            Edge::new("root", "w"),
            Edge::new("w", "w1"),
            Edge::new("w1", "w2"),
        ],
    );
    let pages = fake_pages(&request, 3).expect("pages");
    assert_eq!(
        id_sets(&pages),
        vec![
            set_of(&["root", "s1"]),
            set_of(&["root", "v", "w"]),
            set_of(&["v", "v1", "v2"]),
            set_of(&["w", "w1", "w2"]),
        ]
    );
    assert_eq!(
        pages[1].cont_cues,
        vec![
            ContCue {
                target_page: 3,
                direct_count: 1,
                side: CueSide::Investment,
                anchor_id: "v".to_owned(),
            },
            ContCue {
                target_page: 4,
                direct_count: 1,
                side: CueSide::Investment,
                anchor_id: "w".to_owned(),
            },
        ]
    );
    assert!(pages[2].cont_cues.is_empty());
    assert!(pages[3].cont_cues.is_empty());
}
