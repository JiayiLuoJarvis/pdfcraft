//! The concrete flow node enum and its split dispatch.
//!
//! Not part of the public API. Callers never implement this protocol.

use std::collections::VecDeque;

use crate::error::{Error, ErrorKind, Fallible as Result, GeometryKind};
use crate::geom::{Finite, Length, NonNegative, Point, Positive};
use crate::layout::canvas::CanvasNode;
use crate::layout::columns::ColumnsNode;
use crate::layout::frame::{
    self, AnchorMark, Flowable, Fragment, FragmentCommand, Frame, LayoutCtx, NodeKind, Split,
};
use crate::layout::hline::HLineNode;
use crate::layout::image::ImageNode;
use crate::layout::inset::InsetNode;
use crate::layout::list::ListNode;
use crate::layout::paragraph::{ParagraphNode, TextNode};
use crate::layout::table::TableNode;
use crate::limits::{LimitKind, Limits};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpacerNode {
    pub(crate) height: Length<NonNegative>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedGroup {
    pub(crate) height: Length<NonNegative>,
    pub(crate) spacing_after: Length<NonNegative>,
    pub(crate) fragment: Fragment,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum KeepTogetherNode {
    Open(Vec<FlowNode>),
    Prepared(PreparedGroup),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnchorNode {
    pub(crate) id: String,
    pub(crate) children: Vec<FlowNode>,
    pub(crate) landed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FlowNode {
    Paragraph(ParagraphNode),
    Text(TextNode),
    Spacer(SpacerNode),
    PageBreak,
    KeepTogether(KeepTogetherNode),
    Anchor(AnchorNode),
    Inset(InsetNode<FlowNode>),
    Stack(Vec<FlowNode>),
    Columns(ColumnsNode<FlowNode>),
    Table(TableNode<FlowNode>),
    List(ListNode<FlowNode>),
    Image(ImageNode),
    Canvas(CanvasNode),
    HLine(HLineNode),
}

impl Flowable for FlowNode {
    fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split<Self>> {
        Ok(match self {
            FlowNode::PageBreak => return Err(GeometryKind::NestedPageBreak.into()),
            FlowNode::Paragraph(node) => node.split(avail)?.map(FlowNode::Paragraph),
            FlowNode::Text(node) => node.split(ctx, avail)?.map(FlowNode::Paragraph),
            FlowNode::Spacer(node) => node.split(avail)?.map(FlowNode::Spacer),
            FlowNode::KeepTogether(node) => split_keep_together(ctx, avail, node)?,
            FlowNode::Anchor(node) => split_anchor(ctx, avail, node)?,
            FlowNode::Inset(node) => node.split(ctx, avail)?.map(FlowNode::Inset),
            FlowNode::Stack(kids) => split_stack(ctx, avail, kids)?,
            FlowNode::Columns(node) => node.split(ctx, avail)?.map(FlowNode::Columns),
            FlowNode::Table(node) => node.split(ctx, avail)?.map(FlowNode::Table),
            FlowNode::List(node) => node.split(ctx, avail)?.map(FlowNode::List),
            FlowNode::Image(node) => node.split(avail)?.map(FlowNode::Image),
            FlowNode::Canvas(node) => node.split(ctx, avail)?.map(FlowNode::Canvas),
            FlowNode::HLine(node) => node.split(avail)?.map(FlowNode::HLine),
        })
    }

    fn stack(nodes: Vec<Self>) -> Self {
        FlowNode::Stack(nodes)
    }

    fn kind(&self) -> NodeKind {
        match self {
            FlowNode::Paragraph(_) | FlowNode::Text(_) => NodeKind::Paragraph,
            FlowNode::Spacer(_) => NodeKind::Spacer,
            FlowNode::PageBreak => NodeKind::PageBreak,
            FlowNode::KeepTogether(_) => NodeKind::KeepTogether,
            FlowNode::Anchor(_) => NodeKind::Anchor,
            FlowNode::Inset(_) => NodeKind::Inset,
            FlowNode::Stack(_) => NodeKind::Stack,
            FlowNode::Columns(_) => NodeKind::Columns,
            FlowNode::Table(_) => NodeKind::Table,
            FlowNode::List(_) => NodeKind::List,
            FlowNode::Image(_) => NodeKind::Image,
            FlowNode::Canvas(_) => NodeKind::Canvas,
            FlowNode::HLine(_) => NodeKind::HLine,
        }
    }

    fn nested(&self) -> &[Self] {
        match self {
            FlowNode::Stack(kids) | FlowNode::KeepTogether(KeepTogetherNode::Open(kids)) => kids,
            FlowNode::Anchor(node) => &node.children,
            FlowNode::Inset(node) => std::slice::from_ref(node.child.as_ref()),
            FlowNode::Columns(columns) => columns.column_stacks(),
            FlowNode::Paragraph(_)
            | FlowNode::Text(_)
            | FlowNode::Spacer(_)
            | FlowNode::PageBreak
            | FlowNode::KeepTogether(KeepTogetherNode::Prepared(_))
            | FlowNode::Table(_)
            | FlowNode::List(_)
            | FlowNode::Image(_)
            | FlowNode::Canvas(_)
            | FlowNode::HLine(_) => &[],
        }
    }

    fn list_bodies(&self) -> Vec<&Self> {
        let mut bodies = Vec::new();
        if let FlowNode::List(list) = self {
            list.append_bodies(&mut bodies);
        }
        bodies
    }
}

impl FlowNode {
    pub(crate) fn keep_with_next(&self) -> bool {
        match self {
            FlowNode::Paragraph(p) => p.keep_with_next(),
            FlowNode::Text(t) => t.keep_with_next(),
            FlowNode::Spacer(_)
            | FlowNode::PageBreak
            | FlowNode::KeepTogether(_)
            | FlowNode::Anchor(_)
            | FlowNode::Inset(_)
            | FlowNode::Stack(_)
            | FlowNode::Columns(_)
            | FlowNode::Table(_)
            | FlowNode::List(_)
            | FlowNode::Image(_)
            | FlowNode::Canvas(_)
            | FlowNode::HLine(_) => false,
        }
    }

    pub(crate) fn is_page_break(&self) -> bool {
        matches!(self, FlowNode::PageBreak)
    }

    pub(crate) fn paragraph_total_height_with_spacing(
        &self,
        ctx: &mut LayoutCtx<'_>,
        width: Length<Positive>,
    ) -> Result<Length<NonNegative>> {
        match self {
            FlowNode::Paragraph(p) => Ok(p.total_height() + p.spacing_after),
            FlowNode::Text(t) => {
                let h = t.total_height(ctx, width)?;
                Ok(h + t.spacing_after())
            }
            FlowNode::Spacer(_)
            | FlowNode::PageBreak
            | FlowNode::KeepTogether(_)
            | FlowNode::Anchor(_)
            | FlowNode::Inset(_)
            | FlowNode::Stack(_)
            | FlowNode::Columns(_)
            | FlowNode::Table(_)
            | FlowNode::List(_)
            | FlowNode::Image(_)
            | FlowNode::Canvas(_)
            | FlowNode::HLine(_) => Ok(NonNegative::ZERO),
        }
    }

    pub(crate) fn opening_height(
        &mut self,
        ctx: &mut LayoutCtx<'_>,
        avail: Frame,
    ) -> Result<Length<NonNegative>> {
        match self {
            FlowNode::Paragraph(p) => Ok(p.opening_height()),
            FlowNode::Text(t) => t.opening_height(ctx, avail.width),
            FlowNode::Table(table) => table.opening_height(ctx, avail),
            other @ (FlowNode::Spacer(_)
            | FlowNode::PageBreak
            | FlowNode::KeepTogether(_)
            | FlowNode::Anchor(_)
            | FlowNode::Inset(_)
            | FlowNode::Stack(_)
            | FlowNode::Columns(_)
            | FlowNode::List(_)
            | FlowNode::Image(_)
            | FlowNode::Canvas(_)
            | FlowNode::HLine(_)) => {
                let full = Frame {
                    width: avail.width,
                    height: avail.full_height.into(),
                    full_height: avail.full_height,
                };
                let (h, _) = frame::natural_height(other.clone(), ctx, full)?;
                Ok(h)
            }
        }
    }
}

pub(crate) fn fragment_is_effective(fragment: &Fragment) -> bool {
    fragment.height > NonNegative::ZERO || !fragment.commands.is_empty()
}

/// The first effective child owns the stack baseline, even when that value is `None`.
fn note_first_baseline(chosen: &mut bool, slot: &mut Option<Length<Finite>>, fragment: &Fragment) {
    if *chosen || !fragment_is_effective(fragment) {
        return;
    }
    *slot = fragment.first_baseline;
    *chosen = true;
}

fn stamp_anchor(id: &str, fragment: &mut Fragment) {
    fragment.anchors.push(AnchorMark {
        id: id.to_owned(),
        relative: Point::ZERO,
    });
}

fn split_anchor(
    ctx: &mut LayoutCtx<'_>,
    avail: Frame,
    node: AnchorNode,
) -> Result<Split<FlowNode>> {
    let AnchorNode {
        id,
        children,
        mut landed,
    } = node;
    let split = match sole_or_stack(children) {
        FlowNode::Stack(children) => {
            split_stack_for_anchor(ctx, avail, children, &id, &mut landed)?
        }
        child @ (FlowNode::Paragraph(_)
        | FlowNode::Text(_)
        | FlowNode::Spacer(_)
        | FlowNode::PageBreak
        | FlowNode::KeepTogether(_)
        | FlowNode::Anchor(_)
        | FlowNode::Inset(_)
        | FlowNode::Columns(_)
        | FlowNode::Table(_)
        | FlowNode::List(_)
        | FlowNode::Image(_)
        | FlowNode::Canvas(_)
        | FlowNode::HLine(_)) => match child.split(ctx, avail)? {
            Split::Fit {
                height,
                mut fragment,
            } => {
                land_once(&id, &mut landed, &mut fragment);
                Split::Fit { height, fragment }
            }
            Split::Partial {
                head_height,
                mut head,
                tail,
            } => {
                land_once(&id, &mut landed, &mut head);
                Split::Partial {
                    head_height,
                    head,
                    tail,
                }
            }
            Split::MoveToNextPage(child) => Split::MoveToNextPage(child),
        },
    };
    Ok(match split {
        Split::Fit { height, fragment } => Split::Fit { height, fragment },
        Split::Partial {
            head_height,
            head,
            tail,
        } => Split::Partial {
            head_height,
            head,
            tail: FlowNode::Anchor(AnchorNode {
                id,
                children: into_children(tail),
                landed,
            }),
        },
        Split::MoveToNextPage(child) => Split::MoveToNextPage(FlowNode::Anchor(AnchorNode {
            id,
            children: into_children(child),
            landed,
        })),
    })
}

fn into_children(node: FlowNode) -> Vec<FlowNode> {
    match node {
        FlowNode::Stack(children) => children,
        node @ (FlowNode::Paragraph(_)
        | FlowNode::Text(_)
        | FlowNode::Spacer(_)
        | FlowNode::PageBreak
        | FlowNode::KeepTogether(_)
        | FlowNode::Anchor(_)
        | FlowNode::Inset(_)
        | FlowNode::Columns(_)
        | FlowNode::Table(_)
        | FlowNode::List(_)
        | FlowNode::Image(_)
        | FlowNode::Canvas(_)
        | FlowNode::HLine(_)) => vec![node],
    }
}

fn land_once(id: &str, landed: &mut bool, fragment: &mut Fragment) {
    if !*landed && fragment_is_effective(fragment) {
        stamp_anchor(id, fragment);
        *landed = true;
    }
}

pub(crate) fn sole_or_stack(nodes: Vec<FlowNode>) -> FlowNode {
    match <[FlowNode; 1]>::try_from(nodes) {
        Ok([node]) => node,
        Err(nodes) if nodes.is_empty() => FlowNode::Stack(Vec::new()),
        Err(nodes) => FlowNode::Stack(nodes),
    }
}

#[derive(Debug, Default)]
pub(crate) struct PendingLandings {
    ids: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LandingScope(usize);

impl PendingLandings {
    pub(crate) fn open(&mut self, anchor: AnchorNode) -> (LandingScope, Vec<FlowNode>) {
        let scope = LandingScope(self.ids.len());
        if !anchor.landed {
            self.ids.push(anchor.id);
        }
        (scope, anchor.children)
    }

    pub(crate) fn land(&mut self, fragment: &mut Fragment) {
        if self.ids.is_empty() || !fragment_is_effective(fragment) {
            return;
        }
        for id in self.ids.drain(..) {
            stamp_anchor(&id, fragment);
        }
    }

    pub(crate) fn close(&mut self, scope: LandingScope) {
        self.ids.truncate(scope.0);
    }
}

impl SpacerNode {
    pub(crate) fn split(self, avail: Frame) -> Result<Split<Self>> {
        let h = self.height;
        if h == NonNegative::ZERO {
            return Ok(Split::Fit {
                height: NonNegative::ZERO,
                fragment: Fragment::empty(),
            });
        }
        let take = h.min(avail.height);
        let rest = (h - take).as_pt();
        let head = Fragment {
            height: take,
            spacing_after: NonNegative::ZERO,
            commands: Vec::new(),
            anchors: Vec::new(),
            first_baseline: None,
        };
        if rest <= 0.0 {
            Ok(Split::Fit {
                height: take,
                fragment: head,
            })
        } else {
            Ok(Split::Partial {
                head_height: take,
                head,
                tail: SpacerNode {
                    height: NonNegative::pt(rest)?,
                },
            })
        }
    }
}

fn split_keep_together(
    ctx: &mut LayoutCtx<'_>,
    avail: Frame,
    node: KeepTogetherNode,
) -> Result<Split<FlowNode>> {
    let prepared = match node {
        KeepTogetherNode::Prepared(prepared) => prepared,
        KeepTogetherNode::Open(kids) => {
            let full = Frame {
                width: avail.width,
                height: avail.full_height.into(),
                full_height: avail.full_height,
            };
            let (height, mut fragment) = frame::natural_height(FlowNode::Stack(kids), ctx, full)?;
            let spacing_after = fragment.spacing_after;
            fragment.spacing_after = NonNegative::ZERO;
            PreparedGroup {
                height,
                spacing_after,
                fragment,
            }
        }
    };

    if prepared.height.as_pt() <= avail.height.as_pt() {
        let mut fragment = prepared.fragment;
        fragment.spacing_after = prepared.spacing_after;
        Ok(Split::Fit {
            height: prepared.height,
            fragment,
        })
    } else if prepared.height.as_pt() <= avail.full_height.as_pt() {
        Ok(Split::MoveToNextPage(FlowNode::KeepTogether(
            KeepTogetherNode::Prepared(prepared),
        )))
    } else {
        Err(ErrorKind::UnsplittableTallerThanFrame {
            height: prepared.height.as_pt(),
            avail: avail.full_height.as_pt(),
        }
        .into())
    }
}

fn split_stack(
    ctx: &mut LayoutCtx<'_>,
    avail: Frame,
    kids: Vec<FlowNode>,
) -> Result<Split<FlowNode>> {
    split_stack_inner(ctx, avail, kids, None)
}

fn split_stack_for_anchor(
    ctx: &mut LayoutCtx<'_>,
    avail: Frame,
    kids: Vec<FlowNode>,
    id: &str,
    landed: &mut bool,
) -> Result<Split<FlowNode>> {
    split_stack_inner(ctx, avail, kids, Some((id, landed)))
}

fn should_push_chain_in_stack(
    first: &FlowNode,
    rest: &mut VecDeque<FlowNode>,
    ctx: &mut LayoutCtx<'_>,
    avail: Frame,
    placed_any: bool,
) -> Result<bool> {
    if !placed_any {
        return Ok(false);
    }
    let mut chain_end = 0;
    while let Some(node) = rest.get(chain_end) {
        if node.keep_with_next() {
            chain_end += 1;
        } else {
            break;
        }
    }
    if chain_end >= rest.len() {
        return Ok(false);
    }
    if let Some(node) = rest.get(chain_end) {
        if node.is_page_break() {
            return Ok(false);
        }
    }
    let mut needed = first.paragraph_total_height_with_spacing(ctx, avail.width)?;
    for node in rest.iter().take(chain_end) {
        needed += node.paragraph_total_height_with_spacing(ctx, avail.width)?;
    }
    let Some(target) = rest.get_mut(chain_end) else {
        return Ok(false);
    };
    let target_opening = target.opening_height(ctx, avail)?;
    needed += target_opening;
    Ok(needed > avail.height)
}

fn split_stack_finish(
    head: Fragment,
    tail_items: Vec<FlowNode>,
    placed_any: bool,
) -> Split<FlowNode> {
    if placed_any {
        Split::Partial {
            head_height: head.height,
            head,
            tail: FlowNode::Stack(tail_items),
        }
    } else {
        Split::MoveToNextPage(FlowNode::Stack(tail_items))
    }
}

struct StackPlacer<'a> {
    commands: Vec<FragmentCommand>,
    anchors: Vec<AnchorMark>,
    y: Length<NonNegative>,
    first_baseline: Option<Length<Finite>>,
    baseline_chosen: bool,
    landing: Option<(&'a str, &'a mut bool)>,
}

impl<'a> StackPlacer<'a> {
    fn new(landing: Option<(&'a str, &'a mut bool)>) -> Self {
        Self {
            commands: Vec::new(),
            anchors: Vec::new(),
            y: NonNegative::ZERO,
            first_baseline: None,
            baseline_chosen: false,
            landing,
        }
    }

    fn place_fragment(&mut self, mut frag: Fragment) -> Result<Length<NonNegative>> {
        land_nested_fragment(&mut self.landing, &mut frag);
        let spacing = frag.spacing_after;
        let offset = frag.with_offset(Finite::ZERO, self.y.into())?;
        note_first_baseline(&mut self.baseline_chosen, &mut self.first_baseline, &offset);
        self.commands.extend(offset.commands);
        self.anchors.extend(offset.anchors);
        Ok(spacing)
    }

    fn finish(
        self,
        spacing_after: Length<NonNegative>,
        tail: Vec<FlowNode>,
        placed_any: bool,
    ) -> Split<FlowNode> {
        split_stack_finish(
            Fragment {
                height: self.y,
                spacing_after,
                commands: self.commands,
                anchors: self.anchors,
                first_baseline: self.first_baseline,
            },
            tail,
            placed_any,
        )
    }
}

fn split_stack_inner(
    ctx: &mut LayoutCtx<'_>,
    mut avail: Frame,
    kids: Vec<FlowNode>,
    landing: Option<(&str, &mut bool)>,
) -> Result<Split<FlowNode>> {
    if kids.is_empty() {
        return Ok(Split::Fit {
            height: NonNegative::ZERO,
            fragment: Fragment::empty(),
        });
    }

    let mut kids: VecDeque<FlowNode> = kids.into();
    let mut trailing_spacing = NonNegative::ZERO;
    let mut placed_any = false;
    let mut placer = StackPlacer::new(landing);

    while let Some(child) = kids.pop_front() {
        if child.keep_with_next()
            && should_push_chain_in_stack(&child, &mut kids, ctx, avail, placed_any)?
        {
            let mut rest = Vec::with_capacity(kids.len() + 1);
            rest.push(child);
            rest.extend(kids);
            return Ok(placer.finish(trailing_spacing, rest, placed_any));
        }
        match child.split(ctx, avail)? {
            Split::Fit {
                height,
                fragment: child_frag,
            } => {
                let spacing = placer.place_fragment(child_frag)?;
                placer.y += height;
                avail.height = avail.height.saturating_sub(height);
                placed_any = true;

                if kids.is_empty() {
                    trailing_spacing = spacing;
                } else if spacing > NonNegative::ZERO && avail.height >= spacing {
                    placer.y += spacing;
                    avail.height = avail.height.sub_exact(spacing);
                } else if spacing > NonNegative::ZERO && ctx.natural {
                    placer.y += spacing;
                    avail.height = NonNegative::ZERO;
                }
            }
            Split::Partial {
                head_height,
                head,
                tail,
            } => {
                let spacing = placer.place_fragment(head)?;
                placer.y += head_height;
                let mut rest = Vec::with_capacity(kids.len() + 1);
                rest.push(tail);
                rest.extend(kids);
                let spacing_after = if ctx.natural {
                    spacing
                } else {
                    NonNegative::ZERO
                };
                return Ok(placer.finish(spacing_after, rest, true));
            }
            Split::MoveToNextPage(node) => {
                let mut rest = Vec::with_capacity(kids.len() + 1);
                rest.push(node);
                rest.extend(kids);
                return Ok(placer.finish(NonNegative::ZERO, rest, placed_any));
            }
        }
    }

    Ok(Split::Fit {
        height: placer.y,
        fragment: Fragment {
            height: placer.y,
            spacing_after: trailing_spacing,
            commands: placer.commands,
            anchors: placer.anchors,
            first_baseline: placer.first_baseline,
        },
    })
}

fn land_nested_fragment(landing: &mut Option<(&str, &mut bool)>, fragment: &mut Fragment) {
    if let Some((id, landed)) = landing {
        land_once(id, landed, fragment);
    }
}

/// Rejects a body tree over `limits`.
///
/// A stack is a container and is not a block. Text inside table cells, columns,
/// lists, and keep-together groups counts. Header, footer, and layer hooks are
/// not in `roots`. Paragraph bytes are the UTF-8 length of text runs.
pub(crate) fn check_limits(roots: &[FlowNode], limits: &Limits) -> Result<()> {
    let census = census(roots);
    exceeded(
        LimitKind::Blocks,
        census.blocks,
        u64::from(limits.max_blocks()),
    )?;
    exceeded(
        LimitKind::TableCells,
        census.cells,
        u64::from(limits.max_table_cells()),
    )?;
    exceeded(
        LimitKind::ParagraphBytes,
        census.paragraph_bytes,
        limits.max_paragraph_bytes() as u64,
    )?;
    Ok(())
}

fn exceeded(kind: LimitKind, value: u64, max: u64) -> Result<()> {
    if value > max {
        Err(Error::from(ErrorKind::LimitExceeded { kind, value, max }))
    } else {
        Ok(())
    }
}

struct Census {
    blocks: u64,
    cells: u64,
    paragraph_bytes: u64,
}

fn census(roots: &[FlowNode]) -> Census {
    let mut census = Census {
        blocks: 0,
        cells: 0,
        paragraph_bytes: 0,
    };
    let mut pending: Vec<&FlowNode> = Vec::new();
    pending.extend(roots.iter().rev());
    while let Some(node) = pending.pop() {
        match node {
            FlowNode::Stack(kids) => pending.extend(kids.iter().rev()),
            FlowNode::KeepTogether(KeepTogetherNode::Open(kids)) => {
                census.blocks = census.blocks.saturating_add(1);
                pending.extend(kids.iter().rev());
            }
            FlowNode::Anchor(node) => {
                census.blocks = census.blocks.saturating_add(1);
                pending.extend(node.children.iter().rev());
            }
            FlowNode::Inset(node) => {
                census.blocks = census.blocks.saturating_add(1);
                pending.push(node.child.as_ref());
            }
            FlowNode::KeepTogether(KeepTogetherNode::Prepared(_))
            | FlowNode::Spacer(_)
            | FlowNode::PageBreak
            | FlowNode::Image(_)
            | FlowNode::Canvas(_)
            | FlowNode::HLine(_) => {
                census.blocks = census.blocks.saturating_add(1);
            }
            FlowNode::Paragraph(paragraph) => {
                census.blocks = census.blocks.saturating_add(1);
                census.paragraph_bytes = census
                    .paragraph_bytes
                    .saturating_add(paragraph.text_bytes() as u64);
            }
            FlowNode::Text(text) => {
                census.blocks = census.blocks.saturating_add(1);
                census.paragraph_bytes = census
                    .paragraph_bytes
                    .saturating_add(text.byte_len() as u64);
            }
            FlowNode::Columns(columns) => {
                census.blocks = census.blocks.saturating_add(1);
                pending.extend(columns.column_stacks().iter().rev());
            }
            FlowNode::Table(table) => {
                census.blocks = census.blocks.saturating_add(1);
                census.cells = census.cells.saturating_add(table.open_cell_count() as u64);
                table.append_open_children(&mut pending);
            }
            FlowNode::List(list) => {
                census.blocks = census.blocks.saturating_add(1);
                list.append_bodies(&mut pending);
            }
        }
    }
    census
}
