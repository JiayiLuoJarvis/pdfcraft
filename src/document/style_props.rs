#![allow(clippy::wildcard_imports)]
use super::*;

/// Parent text style patch for [`Flow::stack`] / [`Layer::stack`]. Unset fields inherit.
#[derive(Debug, Default)]
pub struct TextProps {
    pub(crate) font_size: Option<Length<Positive>>,
    pub(crate) color: Option<Color>,
    pub(crate) errors: ErrorBag,
}

impl TextProps {
    /// Parent font size for content inside the stack.
    pub fn font_size(&mut self, size: impl IntoPositive) {
        match size.into_positive() {
            Ok(size) => self.font_size = Some(size),
            Err(error) => self.errors.record(error),
        }
    }

    /// Parent text color for content inside the stack.
    pub fn color(&mut self, color: Color) {
        self.color = Some(color);
    }

    /// Applies the patch. Returns false when a size was rejected. The error is in `into`.
    pub(crate) fn apply(self, style: &mut Style, into: &mut ErrorBag) -> bool {
        let failed = !self.errors.is_empty();
        into.absorb(self.errors);
        if failed {
            return false;
        }
        if let Some(font_size) = self.font_size {
            style.font_size = font_size;
        }
        if let Some(color) = self.color {
            style.color = color;
        }
        true
    }
}
