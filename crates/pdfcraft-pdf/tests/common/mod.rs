//! Shared helpers for integration tests that need a caller-supplied face.

#![allow(clippy::missing_panics_doc, clippy::must_use_candidate)]

use std::sync::Arc;

use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

/// Bytes from `PDFCRAFT_PDF_FONT`, or `PDFCRAFT_CJK_FONT` if the former is unset.
pub fn test_font_bytes() -> Option<Vec<u8>> {
    let path =
        std::env::var_os("PDFCRAFT_PDF_FONT").or_else(|| std::env::var_os("PDFCRAFT_CJK_FONT"))?;
    Some(std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "read test font {}: {error}",
            std::path::Path::new(&path).display()
        )
    }))
}

/// Bytes from `PDFCRAFT_CJK_FONT` (letter / CJK layout samples).
#[allow(dead_code)] // used by letter_layout / batch_baseline via #[path]
pub fn cjk_font_bytes() -> Option<Vec<u8>> {
    let path = std::env::var_os("PDFCRAFT_CJK_FONT")?;
    Some(std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "read CJK font {}: {error}",
            std::path::Path::new(&path).display()
        )
    }))
}

/// Line height of one face at `size` pt and `factor`.
#[allow(dead_code)]
#[must_use]
pub fn line_height_at(font: &[u8], size: f32, factor: f32) -> f32 {
    let font_ref = FontRef::new(font).unwrap_or_else(|_| {
        FontRef::from_index(font, 0).expect("PDFCRAFT_PDF_FONT must be a usable face")
    });
    let metrics = font_ref.metrics(Size::unscaled(), LocationRef::default());
    let scale = size / f32::from(metrics.units_per_em);
    let natural = metrics.ascent * scale - metrics.descent * scale + metrics.leading * scale;
    natural.max(size * factor)
}

/// `(baseline, line_height)` at 12 pt with factor 1.2 — same formula as layout.
#[allow(dead_code)] // used by document / letter_layout / spec_parity via #[path]
pub fn body_line_12(font: &[u8]) -> (f32, f32) {
    let font_ref = FontRef::new(font).unwrap_or_else(|_| {
        FontRef::from_index(font, 0).expect("PDFCRAFT_PDF_FONT must be a usable face")
    });
    let metrics = font_ref.metrics(Size::unscaled(), LocationRef::default());
    let upem = f32::from(metrics.units_per_em);
    let size = 12.0_f32;
    let scale = size / upem;
    let ascent = metrics.ascent * scale;
    let descent = metrics.descent * scale;
    let leading = metrics.leading * scale;
    let natural = ascent - descent + leading;
    let line_height = natural.max(size * 1.2);
    (ascent, line_height)
}

/// Shared face bytes for tests that register once.
#[allow(dead_code)]
pub fn test_font_arc() -> Option<Arc<Vec<u8>>> {
    test_font_bytes().map(Arc::new)
}

/// Advance width of `ch` at `size` pt (skrifa hmtx, no kerning).
#[allow(dead_code)] // used by document via #[path]
#[must_use]
pub fn advance_pt(font: &[u8], ch: char, size: f32) -> f32 {
    let font_ref = FontRef::new(font).unwrap_or_else(|_| {
        FontRef::from_index(font, 0).expect("PDFCRAFT_PDF_FONT must be a usable face")
    });
    let upem = f32::from(
        font_ref
            .metrics(Size::unscaled(), LocationRef::default())
            .units_per_em,
    );
    let gid = font_ref
        .charmap()
        .map(ch)
        .unwrap_or_else(|| panic!("font has no glyph for {ch:?}"));
    let advance = font_ref
        .glyph_metrics(Size::unscaled(), LocationRef::default())
        .advance_width(gid)
        .expect("advance");
    advance * size / upem
}
