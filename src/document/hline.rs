#![allow(clippy::wildcard_imports)]
use super::*;

/// A solid horizontal line in the flow, cell, or list item it is added to.
///
/// Defaults: full width of the current frame, centered when shorter, 1 pt thick,
/// black, no space above or below. The line never splits; it moves to the next
/// page whole.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct HLine {
    pub(crate) width: Option<Length<Positive>>,
    pub(crate) align: TextAlign,
    pub(crate) thickness: Length<Positive>,
    pub(crate) color: Color,
    pub(crate) space_before: Length<NonNegative>,
    pub(crate) space_after: Length<NonNegative>,
    pub(crate) error: Option<Error>,
}

impl Default for HLine {
    fn default() -> Self {
        Self {
            width: None,
            align: TextAlign::Center,
            thickness: Positive::pt(1.0).expect("1 pt is positive"),
            color: Color::rgb(0, 0, 0),
            space_before: NonNegative::ZERO,
            space_after: NonNegative::ZERO,
            error: None,
        }
    }
}

impl HLine {
    /// Runs `f` on a default line. Pass the result to [`FlowWriter::add`].
    pub fn build(f: impl FnOnce(&mut Self)) -> Self {
        let mut line = Self::default();
        f(&mut line);
        line
    }

    /// Sets the line length. Longer than the frame is an error when laid out.
    pub fn width(&mut self, width: impl IntoPositive) {
        match width.into_positive() {
            Ok(width) => self.width = Some(width),
            Err(error) => self.note(error),
        }
    }

    /// Line thickness.
    pub fn thickness(&mut self, thickness: impl IntoPositive) {
        match thickness.into_positive() {
            Ok(thickness) => self.thickness = thickness,
            Err(error) => self.note(error),
        }
    }

    /// Line color, including alpha.
    pub fn color(&mut self, color: Color) {
        self.color = color;
    }

    /// Blank space above the line. Counts toward the line's height.
    pub fn space_before(&mut self, space: impl IntoNonNegative) {
        match space.into_non_negative() {
            Ok(space) => self.space_before = space,
            Err(error) => self.note(error),
        }
    }

    /// Blank space below the line. Counts toward the line's height.
    pub fn space_after(&mut self, space: impl IntoNonNegative) {
        match space.into_non_negative() {
            Ok(space) => self.space_after = space,
            Err(error) => self.note(error),
        }
    }

    /// Places a shorter line at the start of the frame.
    pub fn start(&mut self) {
        self.align = TextAlign::Start;
    }

    /// Places a shorter line at the end of the frame.
    pub fn end(&mut self) {
        self.align = TextAlign::End;
    }

    pub(crate) fn note(&mut self, error: Error) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    pub(crate) fn lower(self) -> Result<FlowNode> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(FlowNode::HLine(HLineNode::new(
            self.width,
            self.align,
            self.thickness,
            self.color,
            self.space_before,
            self.space_after,
        )))
    }
}
