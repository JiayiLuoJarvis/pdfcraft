//! Side-by-side columns in the flow.

use crate::error::{ColumnWidthsReason, Error, ErrorKind, Fallible as Result};
use crate::geom::{Finite, Length, NonNegative};
use crate::layout::column_width::{resolve_column_widths, ColumnWidth};
use crate::layout::frame::{Flowable, Fragment, Frame, LayoutCtx, Split};
use crate::layout::nesting::{check_content, Container};

pub(crate) const MIN_COLUMN_COUNT: usize = 2;
#[cfg(test)]
const DEFAULT_COLUMN_GAP_PT: f32 = 12.0;

#[cfg(test)]
pub(crate) fn default_column_gap() -> Length<NonNegative> {
    NonNegative::pt(DEFAULT_COLUMN_GAP_PT).expect("default column gap is non-negative")
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ColumnsNode<N> {
    widths: Vec<ColumnWidth>,
    gap: Length<NonNegative>,
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
            gap,
            columns: stacks,
        })
    }

    pub(crate) fn column_stacks(&self) -> &[N] {
        &self.columns
    }

    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split<Self>> {
        let gaps: Length<NonNegative> = std::iter::repeat_n(self.gap, self.columns.len() - 1).sum();
        let Some(inner) = (avail.width - gaps).try_positive() else {
            return Err(Error::from(ErrorKind::ColumnWidths {
                reason: ColumnWidthsReason::Gap,
            }));
        };
        let widths = resolve_column_widths(&self.widths, inner)?;

        let mut x = NonNegative::ZERO;
        let mut height = NonNegative::ZERO;
        let mut spacing_after = NonNegative::ZERO;
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
                Split::MoveToNextPage(node) => (NonNegative::ZERO, Fragment::empty(), Some(node)),
            };
            progress |= placed > NonNegative::ZERO;
            unfinished |= tail.is_some();
            height = height.max(placed);
            spacing_after = spacing_after.max(fragment.spacing_after);
            commands.extend(fragment.with_offset(x.into(), Finite::ZERO)?.commands);
            rest.push(tail.unwrap_or_else(|| N::stack(Vec::new())));
            x += *width + self.gap;
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
                spacing_after: NonNegative::ZERO,
                commands,
            },
            tail,
        })
    }
}
