//! Side-by-side columns in the flow, and the track width solver tables share.

#![cfg_attr(not(test), allow(dead_code))]

use crate::error::{ColumnWidthsReason, Error, ErrorKind, Fallible as Result};
use crate::geom::{Length, NonNegative, Positive};
use crate::layout::frame::{Flowable, Fragment, Frame, LayoutCtx, Split};
use crate::layout::nesting::{check_content, Container};

pub(crate) const MIN_COLUMN_COUNT: usize = 2;
pub(crate) const MAX_AUTO_TRACKS: usize = 1;
const DEFAULT_COLUMN_GAP_PT: f32 = 12.0;

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
    pub(crate) const ONE: Self = Self(1.0);

    pub(crate) fn new(weight: f32) -> Result<Self> {
        if !weight.is_finite() {
            return Err(Error::from(ErrorKind::ColumnWidths {
                reason: ColumnWidthsReason::NonFiniteRatio,
            }));
        }
        if weight <= 0.0 {
            return Err(Error::from(ErrorKind::ColumnWidths {
                reason: ColumnWidthsReason::NonPositiveRatio,
            }));
        }
        Ok(Self(weight))
    }
}

/// `inner` is frame width minus gaps. Auto takes all of `rest`. Auto and Ratio together fail.
pub(crate) fn resolve_column_widths(specs: &[ColumnWidth], inner: f32) -> Result<Vec<f32>> {
    let fail = |reason| Err(Error::from(ErrorKind::ColumnWidths { reason }));
    if specs.is_empty() {
        return fail(ColumnWidthsReason::ColumnCount);
    }
    let autos = specs
        .iter()
        .filter(|s| matches!(s, ColumnWidth::Auto))
        .count();
    if autos > MAX_AUTO_TRACKS {
        return fail(ColumnWidthsReason::MoreThanOneAuto);
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
        return fail(ColumnWidthsReason::AutoWithRatio);
    }
    if fixed > inner {
        return fail(ColumnWidthsReason::FixedSumExceeds);
    }
    let rest = inner - fixed;
    let flexible = weights > 0.0 || autos > 0;
    if flexible && rest <= 0.0 {
        return fail(ColumnWidthsReason::NoRemainingWidth);
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
        return fail(ColumnWidthsReason::NoRemainingWidth);
    }
    Ok(widths)
}

pub(crate) fn default_column_gap() -> Length<NonNegative> {
    NonNegative::pt(DEFAULT_COLUMN_GAP_PT).expect("default column gap is non-negative")
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ColumnsNode<N> {
    widths: Vec<ColumnWidth>,
    gap: f32,
    columns: Vec<N>,
}

impl<N: Flowable> ColumnsNode<N> {
    pub(crate) fn new(
        gap: Length<NonNegative>,
        columns: Vec<(ColumnWidth, Vec<N>)>,
    ) -> Result<Self> {
        if columns.len() < MIN_COLUMN_COUNT {
            return Err(Error::from(ErrorKind::ColumnWidths {
                reason: ColumnWidthsReason::ColumnCount,
            }));
        }
        let mut widths = Vec::with_capacity(columns.len());
        let mut stacks = Vec::with_capacity(columns.len());
        for (width, kids) in columns {
            check_content(Container::Column, &kids)?;
            widths.push(width);
            stacks.push(N::stack(kids));
        }
        Ok(Self {
            widths,
            gap: gap.as_pt(),
            columns: stacks,
        })
    }

    pub(crate) fn column_stacks(&self) -> &[N] {
        &self.columns
    }

    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split<Self>> {
        let gaps: f32 = self.columns.iter().skip(1).map(|_| self.gap).sum();
        let inner = avail.width - gaps;
        if inner <= 0.0 {
            return Err(Error::from(ErrorKind::ColumnWidths {
                reason: ColumnWidthsReason::Gap,
            }));
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
            rest.push(tail.unwrap_or_else(|| N::stack(Vec::new())));
            x += width + self.gap;
        }

        let tail = ColumnsNode {
            widths: self.widths,
            gap: self.gap,
            columns: rest,
        };
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
