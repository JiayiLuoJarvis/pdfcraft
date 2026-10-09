//! Atomic horizontal rule. Width resolves against the frame at split time.

use crate::error::{Error, Result};
use crate::geom::{Color, Length, NonNegative, Paint, Path, Point, Positive, Stroke};
use crate::layout::flow::{FlowNode, Fragment, FragmentCommand, Frame, Split};
use crate::layout::style::TextAlign;

/// A solid horizontal line in the flow.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HLineNode {
    /// `None` means the full width of the frame the node lands in.
    width: Option<Length<Positive>>,
    align: TextAlign,
    thickness: Length<Positive>,
    color: Color,
    space_before: Length<NonNegative>,
    space_after: Length<NonNegative>,
}

impl HLineNode {
    pub(crate) fn new(
        width: Option<Length<Positive>>,
        align: TextAlign,
        thickness: Length<Positive>,
        color: Color,
        space_before: Length<NonNegative>,
        space_after: Length<NonNegative>,
    ) -> Self {
        Self {
            width,
            align,
            thickness,
            color,
            space_before,
            space_after,
        }
    }

    fn height(&self) -> f32 {
        self.space_before.as_pt() + self.thickness.as_pt() + self.space_after.as_pt()
    }

    pub(crate) fn split(self, avail: Frame) -> Result<Split> {
        let width = self.width.map_or(avail.width, Length::as_pt);
        let height = self.height();
        if width > avail.width {
            return Err(Error::UnbreakableWiderThanFrame {
                width,
                avail: avail.width,
            });
        }
        if height > avail.full_height {
            return Err(Error::UnsplittableTallerThanFrame);
        }
        if height > avail.height {
            return Ok(Split::MoveToNextPage(FlowNode::HLine(self)));
        }
        let x = match self.align {
            TextAlign::Start => 0.0,
            TextAlign::Center => (avail.width - width) / 2.0,
            TextAlign::End => avail.width - width,
        };
        let y = self.space_before.as_pt() + self.thickness.as_pt() / 2.0;
        let path = Path::line(Point::try_pt(x, y)?, Point::try_pt(x + width, y)?);
        Ok(Split::Fit {
            height,
            fragment: Fragment {
                height,
                spacing_after: 0.0,
                commands: vec![FragmentCommand::Stroke {
                    path,
                    paint: Paint::solid(self.color),
                    stroke: Stroke::new(self.thickness),
                }],
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::CanvasCommand;
    use crate::fonts::FontStore;
    use crate::layout::columns::{ColumnWidth, ColumnsNode, Ratio};
    use crate::layout::decorate::Decorations;
    use crate::layout::test_util::{page, render, strokes};

    fn frame(width: f32, height: f32, full: f32) -> Frame {
        Frame {
            width,
            height,
            full_height: full,
        }
    }

    fn node(
        width: Option<f32>,
        align: TextAlign,
        thickness: f32,
        before: f32,
        after: f32,
    ) -> HLineNode {
        HLineNode::new(
            width.map(|w| Positive::pt(w).unwrap()),
            align,
            Positive::pt(thickness).unwrap(),
            Color::rgb(0, 0, 0),
            NonNegative::pt(before).unwrap(),
            NonNegative::pt(after).unwrap(),
        )
    }

    #[test]
    fn default_spans_frame_on_centre_line() {
        let Split::Fit { height, fragment } = node(None, TextAlign::Center, 2.0, 5.0, 3.0)
            .split(frame(300.0, 400.0, 400.0))
            .unwrap()
        else {
            panic!("expected fit");
        };
        assert_eq!(height.to_bits(), 10.0f32.to_bits());
        assert_eq!(fragment.spacing_after.to_bits(), 0.0f32.to_bits());
        let FragmentCommand::Stroke {
            path,
            paint,
            stroke,
        } = &fragment.commands[0]
        else {
            panic!("expected stroke");
        };
        assert_eq!(paint.color(), Color::rgb(0, 0, 0));
        assert_eq!(stroke.width().as_pt().to_bits(), 2.0f32.to_bits());
        let verbs = path.verbs();
        assert_eq!(verbs.len(), 2);
        match (&verbs[0], &verbs[1]) {
            (crate::geom::PathVerb::MoveTo(a), crate::geom::PathVerb::LineTo(b)) => {
                assert_eq!(a.x().as_pt().to_bits(), 0.0f32.to_bits());
                assert_eq!(a.y().as_pt().to_bits(), 6.0f32.to_bits());
                assert_eq!(b.x().as_pt().to_bits(), 300.0f32.to_bits());
                assert_eq!(b.y().as_pt().to_bits(), 6.0f32.to_bits());
            }
            _ => panic!("expected move+line"),
        }
    }

    #[test]
    fn short_line_aligns_center_start_end() {
        for (align, x) in [
            (TextAlign::Center, 100.0f32),
            (TextAlign::Start, 0.0),
            (TextAlign::End, 200.0),
        ] {
            let Split::Fit { fragment, .. } = node(Some(100.0), align, 1.0, 0.0, 0.0)
                .split(frame(300.0, 50.0, 50.0))
                .unwrap()
            else {
                panic!("expected fit");
            };
            let FragmentCommand::Stroke { path, .. } = &fragment.commands[0] else {
                panic!("expected stroke");
            };
            let crate::geom::PathVerb::MoveTo(origin) = &path.verbs()[0] else {
                panic!("expected move");
            };
            assert_eq!(origin.x().as_pt().to_bits(), x.to_bits());
            assert_eq!(origin.y().as_pt().to_bits(), 0.5f32.to_bits());
        }
    }

    #[test]
    fn wider_beats_taller_and_short_frame_moves() {
        let wide = node(Some(400.0), TextAlign::Start, 1.0, 0.0, 0.0);
        assert_eq!(
            wide.split(frame(300.0, 100.0, 100.0)).unwrap_err(),
            Error::UnbreakableWiderThanFrame {
                width: 400.0,
                avail: 300.0,
            }
        );

        let tall = node(None, TextAlign::Start, 1.0, 80.0, 80.0);
        assert_eq!(
            tall.split(frame(300.0, 100.0, 100.0)).unwrap_err(),
            Error::UnsplittableTallerThanFrame
        );

        let moved = node(None, TextAlign::Center, 1.0, 20.0, 20.0);
        assert!(matches!(
            moved.split(frame(300.0, 10.0, 100.0)).unwrap(),
            Split::MoveToNextPage(FlowNode::HLine(_))
        ));
    }

    #[test]
    fn overflowing_spaces_are_taller_than_frame() {
        let tall = HLineNode::new(
            None,
            TextAlign::Center,
            Positive::pt(1.0).unwrap(),
            Color::rgb(0, 0, 0),
            NonNegative::pt(f32::MAX).unwrap(),
            NonNegative::ZERO,
        );
        assert_eq!(
            tall.split(frame(300.0, 100.0, 100.0)).unwrap_err(),
            Error::UnsplittableTallerThanFrame
        );
    }

    #[test]
    fn column_offsets_the_line() {
        let line = node(None, TextAlign::Start, 1.0, 0.0, 0.0);
        let even = ColumnWidth::Ratio(Ratio::new(1.0).unwrap());
        let cols = ColumnsNode::new(
            NonNegative::ZERO,
            vec![
                (even, vec![FlowNode::HLine(line.clone())]),
                (even, vec![FlowNode::HLine(line)]),
            ],
        )
        .unwrap();
        let mut fonts = FontStore::new();
        let (result, rec) = render(
            FlowNode::Columns(cols),
            &mut fonts,
            page(200.0, 100.0),
            &Decorations::none(),
        );
        result.unwrap();
        let paths = strokes(&rec.pages()[0]);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], vec![Some((0.0, 0.5)), Some((100.0, 0.5))]);
        assert_eq!(paths[1], vec![Some((100.0, 0.5)), Some((200.0, 0.5))]);
    }

    #[test]
    fn recording_pins_stroke_paint_and_width() {
        let line = HLineNode::new(
            Some(Positive::pt(40.0).unwrap()),
            TextAlign::End,
            Positive::pt(1.5).unwrap(),
            Color::rgb(1, 2, 3),
            NonNegative::ZERO,
            NonNegative::ZERO,
        );
        let mut fonts = FontStore::new();
        let (result, rec) = render(
            FlowNode::HLine(line),
            &mut fonts,
            page(100.0, 50.0),
            &Decorations::none(),
        );
        result.unwrap();
        let pages = rec.pages();
        let CanvasCommand::StrokePath {
            path,
            paint,
            stroke,
        } = &pages[0].ops()[0]
        else {
            panic!("expected stroke path");
        };
        assert_eq!(paint.color(), Color::rgb(1, 2, 3));
        assert_eq!(stroke.width().as_pt().to_bits(), 1.5f32.to_bits());
        let crate::geom::PathVerb::MoveTo(origin) = &path.verbs()[0] else {
            panic!("expected move");
        };
        assert_eq!(origin.x().as_pt().to_bits(), 60.0f32.to_bits());
        assert_eq!(origin.y().as_pt().to_bits(), 0.75f32.to_bits());
    }
}
