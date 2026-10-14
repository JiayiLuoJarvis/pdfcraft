//! Track widths shared by columns and tables.

use crate::error::{ColumnWidthsReason, Error, ErrorKind, Fallible as Result};
use crate::geom::{Length, NonNegative, Positive};

const MAX_AUTO_TRACKS: usize = 1;

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
pub(crate) fn resolve_column_widths(
    specs: &[ColumnWidth],
    inner: Length<Positive>,
) -> Result<Vec<Length<Positive>>> {
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
    let mut fixed = NonNegative::ZERO;
    let mut weights = 0.0f32;
    for spec in specs {
        match spec {
            ColumnWidth::Fixed(w) => fixed += *w,
            ColumnWidth::Ratio(Ratio(w)) => weights += w,
            ColumnWidth::Auto => {}
        }
    }
    if autos > 0 && weights > 0.0 {
        return fail(ColumnWidthsReason::AutoWithRatio);
    }
    if fixed.as_pt() > inner.as_pt() {
        return fail(ColumnWidthsReason::FixedSumExceeds);
    }
    let rest = (inner - fixed).as_pt();
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
    Ok(widths.into_iter().map(Length::from_trusted_pt).collect())
}
