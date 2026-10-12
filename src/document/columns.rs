#![allow(clippy::wildcard_imports)]
use super::*;

/// Columns being added to a row.
pub struct Columns {
    pub(crate) parent: Flow,
    pub(crate) cols: Vec<(ColumnWidth, Vec<FlowNode>)>,
    pub(crate) gap: Length<NonNegative>,
    pub(crate) errors: ErrorBag,
}

impl Columns {
    pub(crate) fn build_on(parent: &Flow, f: impl FnOnce(&mut Self)) -> Self {
        let mut columns = Self {
            parent: parent.child(),
            cols: Vec::new(),
            gap: default_column_gap(),
            errors: ErrorBag::default(),
        };
        f(&mut columns);
        columns
    }

    /// Space between columns. Default 12 pt.
    pub fn gap(&mut self, gap: impl IntoNonNegative) {
        match gap.into_non_negative() {
            Ok(gap) => self.gap = gap,
            Err(error) => self.errors.record(error),
        }
    }

    /// Adds an equal-ratio column to the right of the previous ones.
    pub fn col(&mut self, f: impl FnOnce(&mut Flow)) {
        self.push_col(
            ColumnWidth::Ratio(LayoutRatio::new(1.0).expect("1 is positive")),
            f,
        );
    }

    /// Adds a fixed-width column.
    pub fn col_fixed(&mut self, width: impl IntoPositive, f: impl FnOnce(&mut Flow)) {
        match width.into_positive() {
            Ok(width) => self.push_col(ColumnWidth::Fixed(width), f),
            Err(error) => self.errors.record(error),
        }
    }

    /// Adds a ratio-weighted column. Non-finite or non-positive weights fail assembly.
    pub fn col_ratio(&mut self, weight: f32, f: impl FnOnce(&mut Flow)) {
        match LayoutRatio::new(weight) {
            Ok(ratio) => self.push_col(ColumnWidth::Ratio(ratio), f),
            Err(error) => self.errors.record(error),
        }
    }

    /// Adds a column that takes width left after Fixed tracks.
    pub fn col_auto(&mut self, f: impl FnOnce(&mut Flow)) {
        self.push_col(ColumnWidth::Auto, f);
    }

    fn push_col(&mut self, width: ColumnWidth, f: impl FnOnce(&mut Flow)) {
        let mut child = self.parent.child();
        f(&mut child);
        let (nodes, errors) = child.into_parts();
        let failed = !errors.is_empty();
        self.errors.absorb(errors);
        if !failed {
            self.cols.push((width, nodes));
        }
    }

    pub(crate) fn lower(self, into: &mut ErrorBag) -> Option<FlowNode> {
        let failed = !self.errors.is_empty();
        into.absorb(self.errors);
        let widths: Vec<_> = self.cols.iter().map(|(width, _)| *width).collect();
        let widths_ok = match reject_auto_with_ratio(&widths) {
            Ok(()) => true,
            Err(error) => {
                into.record(error);
                false
            }
        };
        if failed || !widths_ok {
            return None;
        }
        match ColumnsNode::new(self.gap, self.cols) {
            Ok(node) => Some(FlowNode::Columns(node)),
            Err(error) => {
                into.record(error);
                None
            }
        }
    }
}

pub(crate) fn reject_auto_with_ratio(widths: &[ColumnWidth]) -> Fallible<()> {
    let auto = widths.iter().any(|w| matches!(w, ColumnWidth::Auto));
    let ratio = widths.iter().any(|w| matches!(w, ColumnWidth::Ratio(_)));
    if auto && ratio {
        Err(Error::from(ErrorKind::ColumnWidths {
            reason: ColumnWidthsReason::AutoWithRatio,
        }))
    } else {
        Ok(())
    }
}
