//! Side-by-side columns in the flow, and the track width solver tables share.

#![cfg_attr(not(test), allow(dead_code))]

use crate::error::{Error, Result};
use crate::geom::{Length, NonNegative, Positive};
use crate::layout::flow::{FlowNode, Fragment, Frame, LayoutCtx, Split};
use crate::layout::lower::{check_content, Container};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ColumnWidth {
    Fixed(Length<Positive>),
    Ratio(Ratio),
    /// Takes what Fixed and Ratio leave. Not sized by content, unlike HTML `auto`.
    Auto,
}

/// Positive weight. Ratio tracks split their share of the rest by weight sum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ratio(f32);

impl Ratio {
    pub(crate) fn new(weight: f32) -> Result<Self> {
        if !weight.is_finite() {
            return Err(Error::ColumnWidths {
                reason: "non-finite",
            });
        }
        if weight <= 0.0 {
            return Err(Error::ColumnWidths {
                reason: "non-positive-ratio",
            });
        }
        Ok(Self(weight))
    }
}

/// `inner` is frame width minus gaps. Auto takes all of `rest`. Auto and Ratio together fail.
pub(crate) fn resolve_column_widths(specs: &[ColumnWidth], inner: f32) -> Result<Vec<f32>> {
    let fail = |reason| Err(Error::ColumnWidths { reason });
    if specs.is_empty() {
        return fail("column-count");
    }
    let autos = specs
        .iter()
        .filter(|s| matches!(s, ColumnWidth::Auto))
        .count();
    if autos > 1 {
        return fail("more-than-one-auto");
    }
    let mut fixed = 0.0f32;
    let mut weights = 0.0f32;
    for spec in specs {
        match spec {
            ColumnWidth::Fixed(w) => fixed += w.as_pt(),
            ColumnWidth::Ratio(Ratio(w)) => weights += w,
            ColumnWidth::Auto => {}
        }
    }
    if autos > 0 && weights > 0.0 {
        return fail("auto-with-ratio");
    }
    if fixed > inner {
        return fail("fixed-sum-exceeds");
    }
    let rest = inner - fixed;
    let flexible = weights > 0.0 || autos > 0;
    if flexible && rest <= 0.0 {
        return fail("no-remaining-width");
    }
    let widths: Vec<f32> = specs
        .iter()
        .map(|spec| match spec {
            ColumnWidth::Fixed(w) => w.as_pt(),
            ColumnWidth::Ratio(Ratio(w)) => rest * w / weights,
            ColumnWidth::Auto => rest,
        })
        .collect();
    if widths.iter().any(|w| !w.is_finite() || *w <= 0.0) {
        return fail("no-remaining-width");
    }
    Ok(widths)
}

pub(crate) fn default_column_gap() -> Length<NonNegative> {
    NonNegative::pt(12.0).expect("12 pt is non-negative")
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ColumnsNode {
    widths: Vec<ColumnWidth>,
    gap: f32,
    columns: Vec<FlowNode>,
}

impl ColumnsNode {
    pub(crate) fn new(
        gap: Length<NonNegative>,
        columns: Vec<(ColumnWidth, Vec<FlowNode>)>,
    ) -> Result<Self> {
        if columns.len() < 2 {
            return Err(Error::ColumnWidths {
                reason: "column-count",
            });
        }
        let mut widths = Vec::with_capacity(columns.len());
        let mut stacks = Vec::with_capacity(columns.len());
        for (width, kids) in columns {
            check_content(Container::Column, &kids)?;
            widths.push(width);
            stacks.push(FlowNode::Stack(kids));
        }
        Ok(Self {
            widths,
            gap: gap.as_pt(),
            columns: stacks,
        })
    }

    pub(crate) fn column_stacks(&self) -> &[FlowNode] {
        &self.columns
    }

    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split> {
        let gaps: f32 = self.columns.iter().skip(1).map(|_| self.gap).sum();
        let inner = avail.width - gaps;
        if inner <= 0.0 {
            return Err(Error::ColumnWidths { reason: "gap" });
        }
        let widths = resolve_column_widths(&self.widths, inner)?;

        let mut x = 0.0f32;
        let mut height = 0.0f32;
        let mut spacing_after = 0.0f32;
        let mut progress = false;
        let mut commands = Vec::new();
        let mut rest = Vec::with_capacity(self.columns.len());
        let mut unfinished = false;
        for (column, width) in self.columns.into_iter().zip(&widths) {
            let frame = Frame {
                width: *width,
                height: avail.height,
                full_height: avail.full_height,
            };
            let (placed, fragment, tail) = match column.split(ctx, frame)? {
                Split::Fit { height, fragment } => (height, fragment, None),
                Split::Partial {
                    head_height,
                    head,
                    tail,
                } => (head_height, head, Some(tail)),
                Split::MoveToNextPage(node) => (0.0, Fragment::empty(), Some(node)),
            };
            progress |= placed > 0.0;
            unfinished |= tail.is_some();
            height = height.max(placed);
            spacing_after = spacing_after.max(fragment.spacing_after);
            commands.extend(fragment.with_offset(x, 0.0)?.commands);
            rest.push(tail.unwrap_or(FlowNode::Stack(Vec::new())));
            x += width + self.gap;
        }

        let tail = FlowNode::Columns(ColumnsNode {
            widths: self.widths,
            gap: self.gap,
            columns: rest,
        });
        if !unfinished {
            return Ok(Split::Fit {
                height,
                fragment: Fragment {
                    height,
                    spacing_after,
                    commands,
                },
            });
        }
        if !progress {
            return Ok(Split::MoveToNextPage(tail));
        }
        Ok(Split::Partial {
            head_height: height,
            head: Fragment {
                height,
                spacing_after: 0.0,
                commands,
            },
            tail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::decorate::Decorations;
    use crate::layout::flow::KeepTogetherNode;
    use crate::layout::paragraph::ParagraphNode;
    use crate::layout::test_util::{
        bits, fonts_and_style, metrics, page, render, runs, text, texts,
    };

    fn fixed(w: f32) -> ColumnWidth {
        ColumnWidth::Fixed(Positive::pt(w).unwrap())
    }

    fn ratio(w: f32) -> ColumnWidth {
        ColumnWidth::Ratio(Ratio::new(w).unwrap())
    }

    fn reason(result: Result<impl std::fmt::Debug>) -> &'static str {
        match result {
            Err(Error::ColumnWidths { reason }) => reason,
            other => panic!("expected ColumnWidths, got {other:?}"),
        }
    }

    #[test]
    fn auto_takes_what_fixed_leaves() {
        let widths = resolve_column_widths(&[fixed(80.0), ColumnWidth::Auto], 188.0).unwrap();
        assert_eq!(widths, [80.0, 108.0]);
    }

    #[test]
    fn ratios_split_the_rest_by_weight() {
        let widths = resolve_column_widths(&[ratio(2.0), ratio(1.0)], 90.0).unwrap();
        assert_eq!(widths, [60.0, 30.0]);
    }

    #[test]
    fn auto_beside_ratio_is_rejected() {
        let specs = [fixed(10.0), ratio(1.0), ColumnWidth::Auto];
        assert_eq!(
            reason(resolve_column_widths(&specs, 110.0)),
            "auto-with-ratio"
        );
    }

    #[test]
    fn fixed_only_leaves_slack_on_the_right() {
        assert_eq!(
            resolve_column_widths(&[fixed(50.0)], 200.0).unwrap(),
            [50.0]
        );
    }

    #[test]
    fn column_widths_more_than_one_auto() {
        let specs = [ColumnWidth::Auto, ColumnWidth::Auto];
        assert_eq!(
            reason(resolve_column_widths(&specs, 100.0)),
            "more-than-one-auto"
        );
    }

    #[test]
    fn column_widths_fixed_sum_exceeds() {
        let specs = [fixed(150.0), fixed(60.0)];
        assert_eq!(
            reason(resolve_column_widths(&specs, 200.0)),
            "fixed-sum-exceeds"
        );
    }

    #[test]
    fn column_widths_no_remaining_width() {
        let specs = [fixed(100.0), ColumnWidth::Auto];
        assert_eq!(
            reason(resolve_column_widths(&specs, 100.0)),
            "no-remaining-width"
        );
    }

    #[test]
    fn column_widths_bad_ratio() {
        assert_eq!(reason(Ratio::new(0.0)), "non-positive-ratio");
        assert_eq!(reason(Ratio::new(f32::NAN)), "non-finite");
    }

    #[test]
    fn column_widths_column_count() {
        let (_, style) = fonts_and_style();
        let one = ColumnsNode::new(
            default_column_gap(),
            vec![(fixed(10.0), vec![text("A", &style)])],
        );
        assert_eq!(reason(one), "column-count");
    }

    #[test]
    fn column_widths_gap_eats_the_frame() {
        let (mut fonts, style) = fonts_and_style();
        let cols = ColumnsNode::new(
            NonNegative::pt(300.0).unwrap(),
            vec![
                (ColumnWidth::Auto, vec![text("A", &style)]),
                (fixed(10.0), vec![text("B", &style)]),
            ],
        )
        .unwrap();
        let (result, rec) = render(
            FlowNode::Columns(cols),
            &mut fonts,
            page(200.0, 400.0),
            &Decorations::none(),
        );
        assert_eq!(reason(result), "gap");
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn columns_place_each_column_at_its_x() {
        let (mut fonts, style) = fonts_and_style();
        let b = metrics(&fonts, &style).baseline;
        let cols = ColumnsNode::new(
            default_column_gap(),
            vec![
                (fixed(80.0), vec![text("A", &style)]),
                (ColumnWidth::Auto, vec![text("X", &style)]),
            ],
        )
        .unwrap();
        let (result, rec) = render(
            FlowNode::Columns(cols),
            &mut fonts,
            page(200.0, 400.0),
            &Decorations::none(),
        );
        result.unwrap();
        let runs = runs(&rec.pages()[0]);
        assert_eq!(
            runs.iter().map(bits).collect::<Vec<_>>(),
            [
                ("A", 0.0f32.to_bits(), b.to_bits()),
                ("X", 92.0f32.to_bits(), b.to_bits())
            ]
        );
    }

    #[test]
    fn text_wraps_at_column_width_not_body_width() {
        let (mut fonts, style) = fonts_and_style();
        let m = metrics(&fonts, &style);
        let words = "AAAA AAAA AAAA";
        let cols = ColumnsNode::new(
            default_column_gap(),
            vec![
                (fixed(60.0), vec![text(words, &style)]),
                (ColumnWidth::Auto, Vec::new()),
            ],
        )
        .unwrap();
        let root = FlowNode::Stack(vec![text(words, &style), FlowNode::Columns(cols)]);
        let (result, rec) = render(root, &mut fonts, page(200.0, 400.0), &Decorations::none());
        result.unwrap();
        let ys: Vec<u32> = runs(&rec.pages()[0])
            .iter()
            .map(|r| r.2.to_bits())
            .collect();
        let line = |n: f32| (m.line_height * n + m.baseline).to_bits();
        assert_eq!(ys, [line(0.0), line(1.0), line(2.0), line(3.0)]);
    }

    #[test]
    fn columns_split_across_pages_per_column() {
        let (mut fonts, style) = fonts_and_style();
        let m = metrics(&fonts, &style);
        let cols = ColumnsNode::new(
            default_column_gap(),
            vec![
                (ratio(1.0), vec![text("A\nB\nC", &style)]),
                (ratio(1.0), vec![text("X", &style)]),
            ],
        )
        .unwrap();
        let config = page(200.0, m.line_height * 2.0 + 1.0);
        let (result, rec) = render(
            FlowNode::Columns(cols),
            &mut fonts,
            config,
            &Decorations::none(),
        );
        result.unwrap();
        let pages = rec.pages();
        assert_eq!(pages.len(), 2);
        assert_eq!(texts(&pages[0]), ["A", "B", "X"]);
        assert_eq!(
            runs(&pages[1]).iter().map(bits).collect::<Vec<_>>(),
            [("C", 0.0f32.to_bits(), m.baseline.to_bits())]
        );
    }

    #[test]
    fn columns_spacing_after_is_the_largest_column_spacing() {
        let (mut fonts, style) = fonts_and_style();
        let m = metrics(&fonts, &style);
        let mut wide = style.clone();
        wide.paragraph_spacing = NonNegative::pt(8.0).unwrap();
        let mut narrow = style.clone();
        narrow.paragraph_spacing = NonNegative::pt(3.0).unwrap();
        let cols = ColumnsNode::new(
            default_column_gap(),
            vec![
                (ratio(1.0), vec![text("A", &narrow)]),
                (ratio(1.0), vec![text("B", &wide)]),
            ],
        )
        .unwrap();
        let root = FlowNode::Stack(vec![FlowNode::Columns(cols), text("Z", &style)]);
        let (result, rec) = render(root, &mut fonts, page(200.0, 400.0), &Decorations::none());
        result.unwrap();
        let z = runs(&rec.pages()[0]).pop().unwrap();
        assert_eq!(z.2.to_bits(), (m.line_height + 8.0 + m.baseline).to_bits());
    }

    #[test]
    fn stuck_column_on_full_page_errors_with_no_output() {
        let (mut fonts, style) = fonts_and_style();
        let m = metrics(&fonts, &style);
        let cols = ColumnsNode::new(
            default_column_gap(),
            vec![
                (
                    ratio(1.0),
                    vec![FlowNode::KeepTogether(KeepTogetherNode::Open(vec![text(
                        "A\nB\nC", &style,
                    )]))],
                ),
                (ratio(1.0), vec![text("X", &style)]),
            ],
        )
        .unwrap();
        let config = page(200.0, m.line_height * 2.0 + 1.0);
        let (result, rec) = render(
            FlowNode::Columns(cols),
            &mut fonts,
            config,
            &Decorations::none(),
        );
        assert!(matches!(result, Err(Error::UnsplittableTallerThanFrame)));
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn pre_laid_paragraph_wider_than_column_errors() {
        let (mut fonts, style) = fonts_and_style();
        let wide = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
        let cols = ColumnsNode::new(
            default_column_gap(),
            vec![
                (fixed(80.0), vec![FlowNode::Paragraph(wide)]),
                (ColumnWidth::Auto, Vec::new()),
            ],
        )
        .unwrap();
        let (result, _) = render(
            FlowNode::Columns(cols),
            &mut fonts,
            page(200.0, 400.0),
            &Decorations::none(),
        );
        assert!(matches!(result, Err(Error::InvalidGeometry)));
    }
}
