//! Image decode and the per-document image table.
//!
//! Only this file and `emit` name krilla images.

use std::fmt;

use krilla::image::Image;
use krilla::Data;

use crate::canvas::{DecodedImage, ImageFormat, ImageId, ImageRegistry};
use crate::error::{Error, ErrorKind, Result};

/// Encoded bytes above this are rejected before krilla sees them.
///
/// Without `rayon`, `Image::from_*` decodes pixels synchronously, so this runs first.
pub(crate) const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

/// Decoded pixel count above this is rejected after the header is read.
pub(crate) const MAX_IMAGE_PIXELS: u64 = 40_000_000;

/// §2.7 keeps interpolation off for every format.
const INTERPOLATE: bool = false;

/// Decoded image ready to paint. Cheap to clone (`Image` is an `Arc`).
#[derive(Clone)]
pub(super) struct Raster(Image);

impl Raster {
    pub(super) fn krilla(&self) -> &Image {
        &self.0
    }
}

impl fmt::Debug for Raster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (width, height) = self.0.size();
        write!(f, "Raster({width}x{height})")
    }
}

/// Images registered with one document, indexed by [`ImageId`].
///
/// Ids are dense from 0. The table is append-only, so an id never changes meaning.
#[derive(Debug, Default)]
pub(crate) struct ImageTable {
    rasters: Vec<Raster>,
}

impl ImageRegistry for ImageTable {
    fn register(&mut self, bytes: Vec<u8>, format: ImageFormat) -> Result<DecodedImage> {
        if bytes.is_empty() {
            return Err(Error::from(ErrorKind::Image {
                message: "bytes are empty".to_owned(),
            }));
        }
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(Error::from(ErrorKind::Image {
                message: format!("{} bytes; max is {MAX_IMAGE_BYTES}", bytes.len()),
            }));
        }
        let image = decode(bytes, format)?;
        let (width, height) = image.size();
        if width == 0 || height == 0 {
            return Err(Error::from(ErrorKind::Image {
                message: "has an empty side".to_owned(),
            }));
        }
        let pixels = u64::from(width) * u64::from(height);
        if pixels > MAX_IMAGE_PIXELS {
            return Err(Error::from(ErrorKind::Image {
                message: format!("{pixels} pixels; max is {MAX_IMAGE_PIXELS}"),
            }));
        }
        let id = u32::try_from(self.rasters.len()).map_err(|_| {
            Error::from(ErrorKind::Image {
                message: "table is full".to_owned(),
            })
        })?;
        self.rasters.push(Raster(image));
        Ok(DecodedImage {
            id: ImageId::from_raw(id),
            pixel_width: width,
            pixel_height: height,
        })
    }
}

impl ImageTable {
    pub(super) fn get(&self, id: ImageId) -> Option<&Raster> {
        self.rasters.get(id.as_u32() as usize)
    }
}

fn decode(bytes: Vec<u8>, format: ImageFormat) -> Result<Image> {
    let data = Data::from(bytes);
    let image = match format {
        ImageFormat::Png => Image::from_png(data, INTERPOLATE),
        ImageFormat::Jpeg => Image::from_jpeg(data, INTERPOLATE),
        ImageFormat::Gif => Image::from_gif(data, INTERPOLATE),
        ImageFormat::Webp => Image::from_webp(data, INTERPOLATE),
    };
    image.map_err(|message| {
        Error::from(ErrorKind::Image {
            message: format!("{format:?} decode failed: {message}"),
        })
    })
}

#[cfg(test)]
pub(super) fn one_pixel_png() -> &'static [u8] {
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/one-pixel.png"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_garbage_leave_the_table_empty() {
        let mut table = ImageTable::default();
        let empty = table.register(Vec::new(), ImageFormat::Png).unwrap_err();
        assert!(matches!(empty.kind, ErrorKind::Image { .. }));
        let garbage = table
            .register(b"not a png".to_vec(), ImageFormat::Png)
            .unwrap_err();
        assert!(matches!(garbage.kind, ErrorKind::Image { .. }));
        assert!(!matches!(garbage.kind, ErrorKind::Backend { .. }));

        let decoded = table
            .register(one_pixel_png().to_vec(), ImageFormat::Png)
            .unwrap();
        assert_eq!(decoded.id.as_u32(), 0);
        assert_eq!(decoded.pixel_width, 1);
        assert_eq!(decoded.pixel_height, 1);
        assert!(table.get(decoded.id).is_some());
    }

    #[test]
    fn byte_cap_rejects_before_decode() {
        let mut table = ImageTable::default();
        let err = table
            .register(vec![0; MAX_IMAGE_BYTES + 1], ImageFormat::Png)
            .unwrap_err();
        assert_eq!(
            err,
            Error::from(ErrorKind::Image {
                message: format!("{} bytes; max is {MAX_IMAGE_BYTES}", MAX_IMAGE_BYTES + 1),
            })
        );
        let decoded = table
            .register(one_pixel_png().to_vec(), ImageFormat::Jpeg)
            .unwrap_err();
        assert!(matches!(decoded.kind, ErrorKind::Image { .. }));
        let png = table
            .register(one_pixel_png().to_vec(), ImageFormat::Png)
            .unwrap();
        assert_eq!(png.id.as_u32(), 0);
    }
}
