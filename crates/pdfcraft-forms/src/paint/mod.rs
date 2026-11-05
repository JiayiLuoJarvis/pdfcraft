//! Page content streams for stamps and strokes.

mod embed;
mod stream;

use std::collections::BTreeMap;

pub(crate) use stream::{paint, Mark, Stamp, Stroke};

pub(crate) struct PaintFont<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) used: &'a BTreeMap<u16, (char, f32)>,
    pub(crate) upem: f32,
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
}
