//! Page chrome copy. Simplified Chinese. The footer keeps the character `頁`.
//!
//! Page headers do not read [`crate::Chart::title`]. That string is for the
//! caller to store as PDF document info.

/// Which side a continuation cue belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CueSide {
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
pub(crate) fn equity_chart_title(subject_name: &str) -> String {
    role_title("股权架构图", subject_name)
}

/// Investment overview main title.
#[must_use]
pub(crate) fn investment_chart_title(subject_name: &str) -> String {
    role_title("对外投资", subject_name)
}

/// Side continuation main title.
///
/// `index` is the side-local page index, starting at 1. Callers pass `index >= 2`.
#[must_use]
pub(crate) fn continuation_title(index: usize, subject_name: &str) -> String {
    role_title(&format!("续页{index}"), subject_name)
}

/// Document-info title: `{subject_name}股权架构图`.
#[must_use]
pub(crate) fn main_title(subject_name: &str) -> String {
    format!("{subject_name}股权架构图")
}

/// Cut-page main title.
///
/// `page_number` is the PDF page number, the same N as `详见附件{N}`.
#[must_use]
pub(crate) fn penetration_title(page_number: usize, name: &str) -> String {
    role_title(&format!("附件{page_number}"), name)
}

/// Whole-chart subtitle. Investments extend the arrow through the subject.
#[must_use]
pub(crate) fn whole_chart_subtitle(subject_name: &str, has_investments: bool) -> String {
    let base = format!("各股东/合伙人 → {subject_name}");
    if has_investments {
        format!("{base} → 对外投资")
    } else {
        base
    }
}

/// Shareholder-side subtitle.
///
/// The page index is accepted so overview and continuation pages share one call
/// shape. The subtitle does not change with that index.
#[must_use]
pub(crate) fn shareholder_overview(subject_name: &str, _index: usize) -> String {
    whole_chart_subtitle(subject_name, false)
}

/// Investment-side subtitle.
///
/// The page index is accepted so overview and continuation pages share one call
/// shape. The subtitle does not change with that index.
#[must_use]
pub(crate) fn investment_overview(subject_name: &str, _index: usize) -> String {
    format!("{subject_name} → 对外投资")
}

/// Main title and subtitle for a shareholder overview or continuation page.
///
/// `index` is 1-based. `1` is the overview. `2` and above are continuations.
#[must_use]
pub(crate) fn shareholder_page_titles(subject_name: &str, index: usize) -> (String, String) {
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
pub(crate) fn investment_page_titles(subject_name: &str, index: usize) -> (String, String) {
    let main = if index <= 1 {
        investment_chart_title(subject_name)
    } else {
        continuation_title(index, subject_name)
    };
    (main, investment_overview(subject_name, index))
}

/// Shareholder cut subtitle. Omits the percent when `ratio` is `None`.
#[must_use]
pub(crate) fn holds_relation(holder: &str, held: &str, ratio: Option<f64>) -> String {
    with_ratio(&format!("{holder}持有{held}"), ratio)
}

/// Investment cut subtitle. Omits the percent when `ratio` is `None`.
#[must_use]
pub(crate) fn invests_relation(investor: &str, target: &str, ratio: Option<f64>) -> String {
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
pub(crate) fn other_shareholders_name(count: usize) -> String {
    format!("其他股东({count}家)")
}

/// Folded investment name. `count` is the number of folded direct children.
#[must_use]
pub(crate) fn other_investments_name(count: usize) -> String {
    format!("其他投资({count}家)")
}

/// Second line of an overflow box, written after page numbers are known.
#[must_use]
pub(crate) fn overflow_detail_line(target_page: usize) -> String {
    format!("详见附件{target_page}")
}

/// First continuation-cue line. The attachment line is separate.
#[must_use]
pub(crate) fn cont_cue_see(side: CueSide) -> &'static str {
    match side {
        CueSide::Shareholder => "股东详见",
        CueSide::Investment => "对外投资详见",
    }
}

/// Prefix of the attachment line, before the page number.
pub(crate) const ATTACHMENT_PREFIX: &str = "附件";

/// Two continuation-cue lines. They stay separate so glyph checks never see a newline.
#[must_use]
pub(crate) fn cont_cue_lines(target_page: usize, side: CueSide) -> (String, String) {
    (
        cont_cue_see(side).to_owned(),
        format!("{ATTACHMENT_PREFIX}{target_page}"),
    )
}

/// Footer. The last character is `頁`.
#[must_use]
pub(crate) fn footer(page_index: usize, page_count: usize) -> String {
    format!("第 {page_index} / {page_count} 頁")
}

#[cfg(test)]
#[path = "captions_tests.rs"]
mod captions_tests;
