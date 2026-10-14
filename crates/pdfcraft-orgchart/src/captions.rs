//! Page chrome copy. Simplified Chinese. The footer keeps the character `頁`.
//!
//! Page headers do not read `ChartOptions::title`. That field is only for
//! [`crate::document_title`].

/// Which side a continuation cue belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CueSide {
    /// Shareholder cone.
    Shareholder,
    /// Investment cone.
    Investment,
}

const ROLE_SEP: &str = ": ";

fn role_title(role: &str, name: &str) -> String {
    format!("{role}{ROLE_SEP}{name}")
}

/// Shareholder overview and whole-chart main title.
#[must_use]
pub fn equity_chart_title(subject_name: &str) -> String {
    role_title("股权架构图", subject_name)
}

/// Investment overview main title.
#[must_use]
pub fn investment_chart_title(subject_name: &str) -> String {
    role_title("对外投资", subject_name)
}

/// Side continuation main title.
///
/// `index` is the side-local page index, starting at 1. Callers pass `index >= 2`.
#[must_use]
pub fn continuation_title(index: usize, subject_name: &str) -> String {
    role_title(&format!("续页{index}"), subject_name)
}

/// Document-info title. `None` becomes `{subject_name}股权架构图`. A blank string stays blank.
#[must_use]
pub fn main_title(subject_name: &str, options_title: Option<&str>) -> String {
    match options_title {
        Some(title) => title.to_owned(),
        None => format!("{subject_name}股权架构图"),
    }
}

/// Cut-page main title.
///
/// `page_number` is the PDF page number, the same N as `详见附件{N}`.
#[must_use]
pub fn penetration_title(page_number: usize, name: &str) -> String {
    role_title(&format!("附件{page_number}"), name)
}

/// Whole-chart subtitle. Investments extend the arrow through the subject.
#[must_use]
pub fn whole_chart_subtitle(subject_name: &str, has_investments: bool) -> String {
    let base = format!("各股东/合伙人 → {subject_name}");
    if has_investments {
        format!("{base} → 对外投资")
    } else {
        base
    }
}

/// Shareholder-side subtitle. `index` is unused and kept so call sites match investment pages.
#[must_use]
pub fn shareholder_overview(subject_name: &str, index: usize) -> String {
    let _ = index;
    whole_chart_subtitle(subject_name, false)
}

/// Investment-side subtitle. `index` is unused.
#[must_use]
pub fn investment_overview(subject_name: &str, index: usize) -> String {
    let _ = index;
    format!("{subject_name} → 对外投资")
}

/// Main title and subtitle for a shareholder overview or continuation page.
///
/// `index` is 1-based. `1` is the overview. `2` and above are continuations.
#[must_use]
pub fn shareholder_page_titles(subject_name: &str, index: usize) -> (String, String) {
    let main = if index <= 1 {
        equity_chart_title(subject_name)
    } else {
        continuation_title(index, subject_name)
    };
    (main, shareholder_overview(subject_name, index))
}

/// Main title and subtitle for an investment overview or continuation page.
///
/// `index` is 1-based. `1` is the overview. `2` and above are continuations.
#[must_use]
pub fn investment_page_titles(subject_name: &str, index: usize) -> (String, String) {
    let main = if index <= 1 {
        investment_chart_title(subject_name)
    } else {
        continuation_title(index, subject_name)
    };
    (main, investment_overview(subject_name, index))
}

/// Shareholder cut subtitle. Omits the percent when `ratio` is `None`.
#[must_use]
pub fn holds_relation(holder: &str, held: &str, ratio: Option<f64>) -> String {
    with_ratio(&format!("{holder}持有{held}"), ratio)
}

/// Investment cut subtitle. Omits the percent when `ratio` is `None`.
#[must_use]
pub fn invests_relation(investor: &str, target: &str, ratio: Option<f64>) -> String {
    with_ratio(&format!("{investor}投资{target}"), ratio)
}

fn with_ratio(text: &str, ratio: Option<f64>) -> String {
    match ratio {
        Some(ratio) => format!("{text} {ratio:.2}%"),
        None => text.to_owned(),
    }
}

/// Folded shareholder name. `count` is the number of folded direct children.
#[must_use]
pub fn other_shareholders_name(count: usize) -> String {
    format!("其他股东({count}家)")
}

/// Folded investment name. `count` is the number of folded direct children.
#[must_use]
pub fn other_investments_name(count: usize) -> String {
    format!("其他投资({count}家)")
}

/// Second line of an overflow box, written after page numbers are known.
#[must_use]
pub fn overflow_detail_line(target_page: usize) -> String {
    format!("详见附件{target_page}")
}

/// First continuation-cue line. The attachment line is separate.
#[must_use]
pub fn cont_cue_see(side: CueSide) -> &'static str {
    match side {
        CueSide::Shareholder => "股东详见",
        CueSide::Investment => "对外投资详见",
    }
}

/// Prefix of the attachment line, before the page number.
pub const ATTACHMENT_PREFIX: &str = "附件";

/// Two continuation-cue lines. They stay separate so glyph checks never see a newline.
#[must_use]
pub fn cont_cue_lines(target_page: usize, side: CueSide) -> (String, String) {
    (
        cont_cue_see(side).to_owned(),
        format!("{ATTACHMENT_PREFIX}{target_page}"),
    )
}

/// Footer. The last character is `頁`.
#[must_use]
pub fn footer(page_index: usize, page_count: usize) -> String {
    format!("第 {page_index} / {page_count} 頁")
}

#[cfg(test)]
mod tests {
    use super::{
        cont_cue_lines, continuation_title, equity_chart_title, footer, holds_relation,
        investment_chart_title, investment_overview, investment_page_titles, invests_relation,
        main_title, other_investments_name, other_shareholders_name, overflow_detail_line,
        penetration_title, shareholder_overview, shareholder_page_titles, whole_chart_subtitle,
        CueSide,
    };

    #[test]
    fn page_titles_use_role_prefixes() {
        assert_eq!(shareholder_overview("主体", 1), "各股东/合伙人 → 主体");
        assert_eq!(shareholder_overview("主体", 2), "各股东/合伙人 → 主体");
        assert_eq!(investment_overview("主体", 1), "主体 → 对外投资");
        assert_eq!(investment_overview("主体", 3), "主体 → 对外投资");
        assert_eq!(
            shareholder_page_titles("主体", 1),
            (
                "股权架构图: 主体".to_string(),
                "各股东/合伙人 → 主体".to_string()
            )
        );
        assert_eq!(
            shareholder_page_titles("主体", 2),
            (
                "续页2: 主体".to_string(),
                "各股东/合伙人 → 主体".to_string()
            )
        );
        assert_eq!(
            investment_page_titles("主体", 1),
            ("对外投资: 主体".to_string(), "主体 → 对外投资".to_string())
        );
        assert_eq!(
            investment_page_titles("主体", 3),
            ("续页3: 主体".to_string(), "主体 → 对外投资".to_string())
        );
    }

    #[test]
    fn cut_relation_and_chrome() {
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
        assert_eq!(main_title("主体", None), "主体股权架构图");
        assert_eq!(main_title("主体", Some("自定义")), "自定义");
        assert_eq!(main_title("主体", Some("")), "");
        assert_eq!(equity_chart_title("主体"), "股权架构图: 主体");
        assert_eq!(investment_chart_title("主体"), "对外投资: 主体");
        assert_eq!(continuation_title(2, "主体"), "续页2: 主体");
        assert_eq!(penetration_title(2, "超频三"), "附件2: 超频三");
        assert_eq!(whole_chart_subtitle("主体", false), "各股东/合伙人 → 主体");
        assert_eq!(
            whole_chart_subtitle("主体", true),
            "各股东/合伙人 → 主体 → 对外投资"
        );
        assert_eq!(
            cont_cue_lines(2, CueSide::Shareholder),
            ("股东详见".to_string(), "附件2".to_string())
        );
        assert_eq!(
            cont_cue_lines(3, CueSide::Investment),
            ("对外投资详见".to_string(), "附件3".to_string())
        );
        assert_eq!(other_shareholders_name(3), "其他股东(3家)");
        assert_eq!(other_investments_name(3), "其他投资(3家)");
        assert_eq!(footer(1, 5), "第 1 / 5 頁");
        assert_eq!(overflow_detail_line(2), "详见附件2");
        assert_eq!(overflow_detail_line(12), "详见附件12");
    }
}

#[cfg(test)]
#[path = "captions_tests.rs"]
mod captions_tests;
