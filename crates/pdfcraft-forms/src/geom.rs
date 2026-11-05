//! Widget rectangles in PDF user space.

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PdfRect {
    pub(crate) x0: f32,
    pub(crate) y0: f32,
    pub(crate) x1: f32,
    pub(crate) y1: f32,
}

impl PdfRect {
    pub(crate) fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        let (x0, x1) = ordered(x0, x1);
        let (y0, y1) = ordered(y0, y1);
        Self { x0, y0, x1, y1 }
    }

    pub(crate) fn width(self) -> f32 {
        self.x1 - self.x0
    }

    pub(crate) fn height(self) -> f32 {
        self.y1 - self.y0
    }
}

fn ordered(a: f32, b: f32) -> (f32, f32) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}
