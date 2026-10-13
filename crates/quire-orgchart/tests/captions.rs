//! Page chrome strings. They do not open a font.

use quire_orgchart::captions::{
    cont_cue_lines, continuation_title, equity_chart_title, footer, holds_relation,
    investment_chart_title, investment_overview, investment_page_titles, invests_relation,
    other_investments_name, other_shareholders_name, overflow_detail_line, penetration_title,
    shareholder_overview, shareholder_page_titles, whole_chart_subtitle, CueSide,
};

#[test]
fn overview_subtitles_have_no_continuation_suffix() {
    assert_eq!(shareholder_overview("主体", 1), "各股东/合伙人 → 主体");
    assert_eq!(shareholder_overview("主体", 2), "各股东/合伙人 → 主体");
    assert_eq!(investment_overview("主体", 1), "主体 → 对外投资");
    assert_eq!(investment_overview("主体", 3), "主体 → 对外投资");
}

#[test]
fn page_titles_use_role_prefixes() {
    assert_eq!(
        shareholder_page_titles("主体", 1),
        (
            "股权架构图: 主体".to_owned(),
            "各股东/合伙人 → 主体".to_owned()
        )
    );
    assert_eq!(
        shareholder_page_titles("主体", 2),
        ("续页2: 主体".to_owned(), "各股东/合伙人 → 主体".to_owned())
    );
    assert_eq!(
        investment_page_titles("主体", 1),
        ("对外投资: 主体".to_owned(), "主体 → 对外投资".to_owned())
    );
    assert_eq!(
        investment_page_titles("主体", 3),
        ("续页3: 主体".to_owned(), "主体 → 对外投资".to_owned())
    );
}

#[test]
fn cut_relation_names_the_parent_and_ratio() {
    assert_eq!(holds_relation("甲公司", "主体", None), "甲公司持有主体");
    assert_eq!(
        holds_relation("甲公司", "主体", Some(10.0)),
        "甲公司持有主体 10.00%"
    );
    assert_eq!(
        holds_relation("甲公司", "主体", Some(80.5556)),
        "甲公司持有主体 80.56%"
    );
    assert_eq!(invests_relation("主体", "乙公司", None), "主体投资乙公司");
    assert_eq!(
        invests_relation("主体", "乙公司", Some(4.2)),
        "主体投资乙公司 4.20%"
    );
}

#[test]
fn titles_match_the_role_table() {
    assert_eq!(equity_chart_title("主体"), "股权架构图: 主体");
    assert_eq!(investment_chart_title("主体"), "对外投资: 主体");
    assert_eq!(continuation_title(2, "主体"), "续页2: 主体");
    assert_eq!(penetration_title(2, "超频三"), "附件2: 超频三");
    assert_eq!(whole_chart_subtitle("主体", false), "各股东/合伙人 → 主体");
    assert_eq!(
        whole_chart_subtitle("主体", true),
        "各股东/合伙人 → 主体 → 对外投资"
    );
}

#[test]
fn cue_lines_and_other_and_footer() {
    assert_eq!(
        cont_cue_lines(2, CueSide::Shareholder),
        ("股东详见".to_owned(), "附件2".to_owned())
    );
    assert_eq!(
        cont_cue_lines(3, CueSide::Investment),
        ("对外投资详见".to_owned(), "附件3".to_owned())
    );
    assert_eq!(other_shareholders_name(3), "其他股东(3家)");
    assert_eq!(other_investments_name(3), "其他投资(3家)");
    assert_eq!(footer(1, 5), "第 1 / 5 頁");
    assert_eq!(footer(2, 5), "第 2 / 5 頁");
    assert_eq!(overflow_detail_line(2), "详见附件2");
    assert_eq!(overflow_detail_line(12), "详见附件12");
}
