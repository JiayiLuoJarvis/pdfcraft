//! Image decode and the per-document image table.
//!
//! Only this file and `emit` name krilla images.

use std::fmt;

use krilla::image::Image;
use krilla::Data;

use crate::canvas::{DecodedImage, ImageFormat, ImageId, ImageRegistry};
use crate::error::{Error, ErrorKind, Fallible as Result};
use crate::limits::{LimitKind, Limits};

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
    limits: Limits,
}

impl ImageRegistry for ImageTable {
    fn set_limits(&mut self, limits: &Limits) {
        self.limits = *limits;
    }

    fn register(&mut self, bytes: Vec<u8>, format: ImageFormat) -> Result<DecodedImage> {
        if bytes.is_empty() {
            return Err(Error::from(ErrorKind::Image {
                message: "bytes are empty".to_owned(),
            }));
        }
        let max_bytes = self.limits.max_image_bytes();
        if bytes.len() > max_bytes {
            return Err(Error::from(ErrorKind::LimitExceeded {
                kind: LimitKind::ImageBytes,
                value: bytes.len() as u64,
                max: max_bytes as u64,
            }));
        }
        if format == ImageFormat::Png {
            // krilla stores a PNG decode error until document finish, which
            // becomes Backend with no page. Inflate the pixels here instead.
            validate_png_pixels(&bytes, &self.limits)?;
        }
        let image = decode(bytes, format)?;
        let (width, height) = image.size();
        if width == 0 || height == 0 {
            return Err(Error::from(ErrorKind::Image {
                message: "has an empty side".to_owned(),
            }));
        }
        let max_edge = self.limits.max_image_edge();
        let edge = width.max(height);
        if edge > max_edge {
            return Err(Error::from(ErrorKind::LimitExceeded {
                kind: LimitKind::ImageEdge,
                value: u64::from(edge),
                max: u64::from(max_edge),
            }));
        }
        let pixels = u64::from(width).saturating_mul(u64::from(height));
        let max_pixels = self.limits.max_image_pixels();
        if pixels > max_pixels {
            return Err(Error::from(ErrorKind::LimitExceeded {
                kind: LimitKind::ImagePixels,
                value: pixels,
                max: max_pixels,
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

fn validate_png_pixels(bytes: &[u8], limits: &Limits) -> Result<()> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|error| png_error(&error))?;
    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    if width == 0 || height == 0 {
        return Err(Error::from(ErrorKind::Image {
            message: "has an empty side".to_owned(),
        }));
    }
    let max_edge = limits.max_image_edge();
    let edge = width.max(height);
    if edge > max_edge {
        return Err(Error::from(ErrorKind::LimitExceeded {
            kind: LimitKind::ImageEdge,
            value: u64::from(edge),
            max: u64::from(max_edge),
        }));
    }
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    let max_pixels = limits.max_image_pixels();
    if pixels > max_pixels {
        return Err(Error::from(ErrorKind::LimitExceeded {
            kind: LimitKind::ImagePixels,
            value: pixels,
            max: max_pixels,
        }));
    }
    let size = reader.output_buffer_size().ok_or_else(|| {
        Error::from(ErrorKind::Image {
            message: "Png decode failed: image is too large".to_owned(),
        })
    })?;
    let mut frame = vec![0; size];
    reader
        .next_frame(&mut frame)
        .map_err(|error| png_error(&error))?;
    Ok(())
}

fn png_error(error: &png::DecodingError) -> Error {
    Error::from(ErrorKind::Image {
        message: format!("Png decode failed: {error}"),
    })
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
        const MAX: usize = 32 * 1024 * 1024;
        let mut table = ImageTable::default();
        let err = table
            .register(vec![0; MAX + 1], ImageFormat::Png)
            .unwrap_err();
        assert_eq!(
            err,
            Error::from(ErrorKind::LimitExceeded {
                kind: LimitKind::ImageBytes,
                value: (MAX + 1) as u64,
                max: MAX as u64,
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

    #[test]
    fn pixel_cap_rejects_after_decode_and_drops_the_handle() {
        let mut table = ImageTable::default();
        let mut limits = Limits::default();
        limits.set_max_image_pixels(1).unwrap();
        table.set_limits(&limits);
        let err = table.register(rgb_png(2, 1), ImageFormat::Png).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::LimitExceeded {
                kind: LimitKind::ImagePixels,
                value: 2,
                max: 1,
            }
        );
        let kept = table.register(rgb_png(1, 1), ImageFormat::Png).unwrap();
        assert_eq!(kept.id.as_u32(), 0);
        assert_eq!((kept.pixel_width, kept.pixel_height), (1, 1));
    }

    #[test]
    fn edge_cap_rejects_8193_after_decode() {
        let mut table = ImageTable::default();
        let ok = table.register(rgb_png(1, 1), ImageFormat::Png).unwrap();
        assert_eq!((ok.pixel_width, ok.pixel_height), (1, 1));
        let err = table
            .register(rgb_png(8193, 1), ImageFormat::Png)
            .unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::LimitExceeded {
                kind: LimitKind::ImageEdge,
                value: 8193,
                max: 8192,
            }
        );
        let next = table.register(rgb_png(1, 1), ImageFormat::Png).unwrap();
        assert_eq!(next.id.as_u32(), 1);
    }

    fn rgb_png(width: u32, height: u32) -> Vec<u8> {
        let row = 1 + usize::try_from(width).unwrap() * 3;
        let rows = usize::try_from(height).unwrap();
        let raw = vec![0u8; row * rows];
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        let mut png = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
        png.extend(png_chunk(*b"IHDR", &ihdr));
        png.extend(png_chunk(*b"IDAT", &zlib_stored(&raw)));
        png.extend(png_chunk(*b"IEND", &[]));
        png
    }

    fn png_chunk(tag: [u8; 4], data: &[u8]) -> Vec<u8> {
        let mut body = Vec::with_capacity(4 + data.len());
        body.extend_from_slice(&tag);
        body.extend_from_slice(data);
        let crc = crc32(&body);
        let mut out = Vec::with_capacity(12 + data.len());
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc.to_be_bytes());
        out
    }

    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len() + 16);
        out.extend_from_slice(&[0x78, 0x01]);
        let mut offset = 0;
        while offset < data.len() {
            let end = (offset + 65535).min(data.len());
            out.push(u8::from(end == data.len()));
            let len = u16::try_from(end - offset).unwrap();
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(&data[offset..end]);
            offset = end;
        }
        out.extend_from_slice(&adler32(data).to_be_bytes());
        out
    }

    fn adler32(data: &[u8]) -> u32 {
        let mut a = 1u32;
        let mut b = 0u32;
        for &byte in data {
            a = (a + u32::from(byte)) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }
}
