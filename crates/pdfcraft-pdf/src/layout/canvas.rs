//! Fixed-size vector block. Marks are shaped once, then only placed.

use crate::error::{Error, ErrorKind, Fallible as Result};
use crate::fonts::FontId;
use crate::geom::{
    Color, Finite, Ink, Length, NonNegative, Paint, Path, Pen, Point, Positive, Rect, Size, Stroke,
};
use crate::layout::atomic::{place_atomic, AtomicPlace};
use crate::layout::frame::{Fragment, FragmentCommand, Frame, LayoutCtx, Split};
use crate::layout::line_run::single_line_run;
use crate::layout::style::{Style, TextAlign};

/// A mark after the font key has been resolved. Text size and color are concrete.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CanvasMarkNode {
    /// Straight stroke.
    Line {
        /// Start.
        from: Point,
        /// End.
        to: Point,
        /// Stroke.
        pen: Pen,
    },
    /// Closed rectangle.
    Rect {
        /// Box.
        rect: Rect,
        /// Fill, stroke, or both.
        ink: Ink,
    },
    /// Filled path. The caller closes it.
    Fill {
        /// Path in block coordinates.
        path: Path,
        /// Fill color.
        color: Color,
    },
    /// One line. `at` is the baseline left.
    Text {
        /// Non-empty.
        text: String,
        /// Registered face.
        font: FontId,
        /// Baseline left, block coordinates, y down.
        at: Point,
        /// Concrete size. Enclosing `Styled` is not consulted.
        size: Length<Positive>,
        /// Fill.
        color: Color,
    },
}

/// An atomic flow box. `pending` is shaped on the first split.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CanvasNode {
    size: Size,
    align: TextAlign,
    pending: Option<Vec<CanvasMarkNode>>,
    commands: Vec<FragmentCommand>,
}

impl CanvasNode {
    /// Stores marks unshaped. An empty text string is rejected here.
    pub(crate) fn new(size: Size, align: TextAlign, marks: Vec<CanvasMarkNode>) -> Result<Self> {
        if marks
            .iter()
            .any(|mark| matches!(mark, CanvasMarkNode::Text { text, .. } if text.is_empty()))
        {
            return Err(Error::from(ErrorKind::EmptyCanvasText));
        }
        Ok(Self {
            size,
            align,
            pending: Some(marks),
            commands: Vec::new(),
        })
    }

    pub(crate) fn split(mut self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split<Self>> {
        self.ensure_shaped(ctx)?;
        match place_atomic(self.size, self.align, &avail)? {
            AtomicPlace::MoveToNextPage => Ok(Split::MoveToNextPage(self)),
            AtomicPlace::Fit { x, height } => {
                let fragment = Fragment {
                    height,
                    spacing_after: NonNegative::ZERO,
                    commands: self.commands,
                };
                let fragment = if x == 0.0 {
                    fragment
                } else {
                    fragment.with_offset(Finite::pt(x)?, Finite::ZERO)?
                };
                Ok(Split::Fit { height, fragment })
            }
        }
    }

    fn ensure_shaped(&mut self, ctx: &mut LayoutCtx<'_>) -> Result<()> {
        let Some(marks) = self.pending.take() else {
            return Ok(());
        };
        let mut commands = Vec::new();
        for mark in marks {
            push_mark(ctx, &mut commands, mark)?;
        }
        self.commands = commands;
        Ok(())
    }
}

fn push_mark(
    ctx: &mut LayoutCtx<'_>,
    commands: &mut Vec<FragmentCommand>,
    mark: CanvasMarkNode,
) -> Result<()> {
    match mark {
        CanvasMarkNode::Line { from, to, pen } => {
            commands.push(FragmentCommand::Stroke {
                path: Path::line(from, to),
                paint: Paint::solid(pen.color()),
                stroke: Stroke::new(pen.width()),
            });
        }
        CanvasMarkNode::Rect { rect, ink } => push_ink(commands, Path::rect(rect)?, ink),
        CanvasMarkNode::Fill { path, color } => {
            commands.push(FragmentCommand::Fill {
                path,
                paint: Paint::solid(color),
            });
        }
        CanvasMarkNode::Text {
            text,
            font,
            at,
            size,
            color,
        } => {
            let style = Style {
                font,
                font_size: size,
                line_height_factor: 1.0,
                color,
                paragraph_spacing: NonNegative::ZERO,
                first_line_indent: NonNegative::ZERO,
                align: TextAlign::Start,
            };
            let mut run = single_line_run(ctx, &text, &style, None)?;
            // `at` is the baseline. `single_line_run` stores the ascent there because
            // layer text's `at` is the line-box top.
            run.origin = Point::try_pt(at.x().as_pt(), at.y().as_pt())?;
            commands.push(FragmentCommand::Glyphs(run));
        }
    }
    Ok(())
}

fn push_ink(commands: &mut Vec<FragmentCommand>, path: Path, ink: Ink) {
    match ink {
        Ink::Fill(color) => commands.push(FragmentCommand::Fill {
            path,
            paint: Paint::solid(color),
        }),
        Ink::Stroke(pen) => commands.push(FragmentCommand::Stroke {
            path,
            paint: Paint::solid(pen.color()),
            stroke: Stroke::new(pen.width()),
        }),
        Ink::FillStroke(color, pen) => {
            commands.push(FragmentCommand::Fill {
                path: path.clone(),
                paint: Paint::solid(color),
            });
            commands.push(FragmentCommand::Stroke {
                path,
                paint: Paint::solid(pen.color()),
                stroke: Stroke::new(pen.width()),
            });
        }
    }
}
