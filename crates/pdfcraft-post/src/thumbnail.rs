//! First-page JPEG thumbnail.
//!
//! [`first_page_jpeg`] renders page 1 of an existing PDF to JPEG bytes,
//! scaled so the longest side fits [`ThumbnailOptions::max_longest_side`].
//! The renderer needs the opt-in `thumbnail` feature (hayro + image); without
//! it every input that would otherwise render reports
//! [`ErrorKind::ThumbnailFailed`] instead.

use lopdf::{Document, ObjectId};

use crate::error::{Error, ErrorKind};
use crate::metadata::open_pdf;
use crate::pages::materialized_page_with;

/// Options for [`first_page_jpeg`].
///
/// The struct is `non_exhaustive`, so start from [`Default`] and assign the
/// fields to change:
///
/// ```ignore
/// let mut opts = ThumbnailOptions::default();
/// opts.quality = 90;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThumbnailOptions {
    /// Longest output side in pixels.
    ///
    /// The page renders at native size when it already fits; it is never
    /// upscaled. A value of `0` cannot fit any page, so rendering fails with
    /// [`ErrorKind::ThumbnailFailed`].
    pub max_longest_side: u32,
    /// JPEG quality. Clamped to `1..=100` (`0` and values above `100` do not
    /// error; they behave as `1` and `100`).
    pub quality: u8,
}

impl Default for ThumbnailOptions {
    /// 320 px longest side at quality 75.
    fn default() -> Self {
        Self {
            max_longest_side: 320,
            quality: 75,
        }
    }
}

/// Render page 1 of `pdf` to JPEG bytes.
///
/// Only the first page renders: a multi-page input still yields exactly one
/// JPEG of page 1. Alpha flattens to white; the output never carries
/// transparency. Pages already smaller than
/// [`ThumbnailOptions::max_longest_side`] keep their native size.
///
/// A PDF with no pages yields `Ok(None)`; this is not an error.
///
/// # Errors
///
/// [`ErrorKind::NotAPdf`] when `pdf` is not a PDF,
/// [`ErrorKind::Encrypted`] when `pdf` is encrypted (including files whose
/// empty user password `load_mem` auto-decrypts; the hardened `open_pdf` gate
/// runs before any renderer call, so an encrypted input never reaches the
/// renderer), and [`ErrorKind::ThumbnailFailed`] when the page size cannot be
/// resolved, the `thumbnail` feature is disabled, or rendering or JPEG
/// encoding fails.
#[must_use = "dropping the thumbnail discards the only copy"]
pub fn first_page_jpeg(pdf: &[u8], opts: &ThumbnailOptions) -> Result<Option<Vec<u8>>, Error> {
    let Some((width_pt, height_pt)) = first_page_size(pdf)? else {
        return Ok(None);
    };
    #[cfg(feature = "thumbnail")]
    {
        render_first_page_jpeg(pdf, width_pt, height_pt, opts).map(Some)
    }
    #[cfg(not(feature = "thumbnail"))]
    {
        let _ = (width_pt, height_pt, opts);
        Err(Error::from(ErrorKind::ThumbnailFailed))
    }
}

/// Gate the input through `open_pdf`, then resolve page 1's size in points.
///
/// `Ok(None)` means the document parsed but holds no pages. Rotation swaps
/// the sides so the scale computed downstream matches the rendered pixels.
fn first_page_size(pdf: &[u8]) -> Result<Option<(f32, f32)>, Error> {
    let doc = open_pdf(pdf, ErrorKind::Encrypted)?;
    let pages = doc.get_pages();
    let Some((_, first_id)) = pages.iter().next() else {
        return Ok(None);
    };
    Ok(Some(page_size(&doc, *first_id)?))
}

fn page_size(doc: &Document, page_id: ObjectId) -> Result<(f32, f32), Error> {
    let failed = || Error::from(ErrorKind::ThumbnailFailed);
    let page = materialized_page_with(doc, page_id, &ErrorKind::ThumbnailFailed)?;
    let bounds = page
        .get(b"MediaBox")
        .ok()
        .and_then(|object| object.as_array().ok())
        .ok_or_else(&failed)?;
    if bounds.len() != 4 {
        return Err(failed());
    }
    let mut edge = [0.0_f32; 4];
    for (slot, object) in edge.iter_mut().zip(bounds.iter()) {
        let edge_value = if let Ok(exact) = object.as_f32() {
            exact
        } else {
            let raw = object.as_i64().map_err(|_| failed())?;
            #[expect(
                clippy::cast_precision_loss,
                reason = "MediaBox edges are point values far below 2^24"
            )]
            let approx = raw as f32;
            approx
        };
        *slot = edge_value;
    }
    let (mut width, mut height) = ((edge[2] - edge[0]).abs(), (edge[3] - edge[1]).abs());
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(failed());
    }
    let rotation = page
        .get(b"Rotate")
        .ok()
        .and_then(|object| object.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    if rotation == 90 || rotation == 270 {
        (width, height) = (height, width);
    }
    Ok((width, height))
}

#[cfg(feature = "thumbnail")]
fn render_first_page_jpeg(
    pdf: &[u8],
    width_pt: f32,
    height_pt: f32,
    opts: &ThumbnailOptions,
) -> Result<Vec<u8>, Error> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    use hayro::vello_cpu::color::palette::css::WHITE;
    use hayro::{render, PixmapSettings, RenderCache, RenderSettings};
    use image::codecs::jpeg::JpegEncoder;
    use image::imageops::{resize, FilterType};
    use image::{ExtendedColorType, RgbImage};

    let failed = || Error::from(ErrorKind::ThumbnailFailed);
    if opts.max_longest_side == 0 {
        return Err(failed());
    }
    let max_side = opts.max_longest_side;
    let longest_pt = width_pt.max(height_pt);
    #[expect(
        clippy::cast_precision_loss,
        reason = "thumbnail longest side is far below 2^24"
    )]
    let scale = if longest_pt <= max_side as f32 {
        1.0
    } else {
        max_side as f32 / longest_pt
    };
    // `Pdf::new` takes ownership; hayro's load error carries no
    // `std::error::Error` impl, so it maps to the kind without a source.
    let document = Pdf::new(pdf.to_vec()).map_err(|_| failed())?;
    let page = document.pages().first().ok_or_else(&failed)?;
    let cache = RenderCache::new();
    let pixmap = render(
        page,
        &cache,
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        },
    );
    let (rendered_w, rendered_h) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    if rendered_w == 0 || rendered_h == 0 {
        return Err(failed());
    }
    // Premultiplied RGBA over a white canvas: `out = premult + (255 - alpha)`
    // stays within `0..=255` because each premultiplied channel is at most its
    // alpha. White-background rendering already yields opaque pixels; the
    // flattening keeps that promise even if a backend pixel is not.
    let mut rgb = Vec::with_capacity((rendered_w * rendered_h * 3) as usize);
    let (pixels, _) = pixmap.data_as_u8_slice().as_chunks::<4>();
    for pixel in pixels {
        let [red, green, blue, alpha] = *pixel;
        let opaque = 255 - u16::from(alpha);
        rgb.push((u16::from(red) + opaque).min(255) as u8);
        rgb.push((u16::from(green) + opaque).min(255) as u8);
        rgb.push((u16::from(blue) + opaque).min(255) as u8);
    }
    let mut image = RgbImage::from_raw(rendered_w, rendered_h, rgb).ok_or_else(&failed)?;
    // Float truncation between the lopdf `MediaBox` and hayro's own box can
    // round the rendered side up by a pixel; shrink to fit so the
    // `max_longest_side` promise holds exactly.
    if rendered_w.max(rendered_h) > max_side {
        let (target_w, target_h) = fit_inside(rendered_w, rendered_h, max_side);
        image = resize(&image, target_w, target_h, FilterType::Triangle);
    }
    let (out_w, out_h) = (image.width(), image.height());
    let quality = opts.quality.clamp(1, 100);
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, quality)
        .encode(image.as_raw(), out_w, out_h, ExtendedColorType::Rgb8)
        .map_err(|err| Error::from(ErrorKind::ThumbnailFailed).with_source(err))?;
    Ok(jpeg)
}

/// Scale `(width, height)` so the longest side is `max`, keeping both sides
/// at least one pixel.
#[cfg(feature = "thumbnail")]
fn fit_inside(width: u32, height: u32, max: u32) -> (u32, u32) {
    #[expect(
        clippy::cast_precision_loss,
        reason = "thumbnail sides are far below 2^24"
    )]
    let longest = width.max(height).max(1) as f32;
    #[expect(
        clippy::cast_precision_loss,
        reason = "thumbnail sides are far below 2^24"
    )]
    let factor = max.max(1) as f32 / longest;
    #[expect(
        clippy::cast_precision_loss,
        reason = "thumbnail sides are far below 2^24"
    )]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "factor is in (0, 1], so the product fits and `.max(1)` floors it"
    )]
    #[expect(
        clippy::cast_sign_loss,
        reason = "sides and factor are positive, so the product is non-negative"
    )]
    let scaled = (
        ((width as f32 * factor) as u32).max(1),
        ((height as f32 * factor) as u32).max(1),
    );
    scaled
}
