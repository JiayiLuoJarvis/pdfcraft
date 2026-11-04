//! A fixed-width measurer for tests that need no face.

use crate::error::Error;

use super::Measurer;

/// Two millimetres per character. Paging tests inject this instead of a face.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct FallbackMeasurer;

impl Measurer for FallbackMeasurer {
    fn string_width_mm(&mut self, text: &str, _size_pt: f64) -> Result<f64, Error> {
        Ok(crate::units::n64(text.chars().count()) * 2.0)
    }

    fn line_height_mm(&mut self, _size_pt: f64) -> f64 {
        5.0
    }

    fn ensure_glyphs(&mut self, _text: &str) -> Result<(), Error> {
        Ok(())
    }
}
