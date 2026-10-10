#![allow(clippy::wildcard_imports)]
use super::*;

/// Columns being added to a row.
pub struct Columns {
    pub(crate) parent: Flow,
    pub(crate) cols: Vec<Result<(ColumnWidth, Vec<FlowNode>)>>,
    pub(crate) gap: Length<NonNegative>,
}

impl Columns {
    pub(crate) fn build_on(parent: &Flow, f: impl FnOnce(&mut Self)) -> Self {
        let mut columns = Self {
            parent: parent.child(),
            cols: Vec::new(),
            gap: default_column_gap(),
        };
        f(&mut columns);
        columns
    }

    /// Space between columns. Default 12 pt.
    pub fn gap(&mut self, gap: impl IntoNonNegative) {
        match gap.into_non_negative() {
            Ok(gap) => self.gap = gap,
            Err(error) => self.cols.push(Err(error)),
        }
    }

    /// Adds an equal-ratio column to the right of the previous ones.
    pub fn col(&mut self, f: impl FnOnce(&mut Flow)) {
        self.push_col(
            ColumnWidth::Ratio(Ratio::new(1.0).expect("1 is positive")),
            f,
        );
    }

    /// Adds a fixed-width column.
    pub fn col_fixed(&mut self, width: impl IntoPositive, f: impl FnOnce(&mut Flow)) {
        match width.into_positive() {
            Ok(width) => self.push_col(ColumnWidth::Fixed(width), f),
            Err(error) => self.cols.push(Err(error)),
        }
    }

    /// Adds a ratio-weighted column. Non-finite or non-positive weights fail assembly.
    pub fn col_ratio(&mut self, weight: f32, f: impl FnOnce(&mut Flow)) {
        match Ratio::new(weight) {
            Ok(ratio) => self.push_col(ColumnWidth::Ratio(ratio), f),
            Err(error) => self.cols.push(Err(error)),
        }
    }

    /// Adds a column that takes width left after Fixed tracks.
    pub fn col_auto(&mut self, f: impl FnOnce(&mut Flow)) {
        self.push_col(ColumnWidth::Auto, f);
    }

    fn push_col(&mut self, width: ColumnWidth, f: impl FnOnce(&mut Flow)) {
        let col = self.parent.nested(f).map(|nodes| (width, nodes));
        self.cols.push(col);
    }

    pub(crate) fn lower(self) -> Result<FlowNode> {
        let cols = self.cols.into_iter().collect::<Result<Vec<_>>>()?;
        let widths: Vec<_> = cols.iter().map(|(w, _)| *w).collect();
        reject_auto_with_ratio(&widths)?;
        Ok(FlowNode::Columns(ColumnsNode::new(self.gap, cols)?))
    }
}

pub(crate) fn reject_auto_with_ratio(widths: &[ColumnWidth]) -> Result<()> {
    let auto = widths.iter().any(|w| matches!(w, ColumnWidth::Auto));
    let ratio = widths.iter().any(|w| matches!(w, ColumnWidth::Ratio(_)));
    if auto && ratio {
        Err(Error::ColumnWidths {
            reason: "auto-with-ratio",
        })
    } else {
        Ok(())
    }
}
