//! Pure-function lowering from [`DocumentSpec`] to layout nodes.

use std::collections::HashMap;
use std::sync::Arc;

use crate::canvas::{DecodedImage, ImageRegistry, LinkTarget, RecordingCanvas};
use crate::error::{
    ColumnWidthsReason, Error, ErrorBag, ErrorKind, Fallible, GeometryKind, Location, Result,
};
use crate::fonts::{FontFace, FontId, FontKey, FontSource, FontStore};
use crate::geom::{
    Color, Ink, Length, Margins, NonNegative, PageSize, Path, Pen, Point, Positive, Rect,
};
use crate::layout::canvas::{CanvasMarkNode, CanvasNode};
use crate::layout::column_width::{ColumnWidth, Ratio as LayoutRatio};
use crate::layout::columns::ColumnsNode;
use crate::layout::decorate::{Decorations, LayerContent, LayerItem, LayerText, PageInfo};
use crate::layout::flow::{check_limits, FlowNode, KeepTogetherNode, SpacerNode};
use crate::layout::frame::LayoutCtx;
use crate::layout::hline::HLineNode;
use crate::layout::image::ImageNode;
use crate::layout::list::{
    ListItem, ListKind as LayoutListKind, ListNode, ListSpec as LayoutListSpec,
};
use crate::layout::paginate::{layout_pages, paint_laid, PaginateConfig, SectionJob};
use crate::layout::paragraph::{
    BlankWidth, InlineBlank, InlineItem, TextNode, TextRun as LayoutTextRun,
};
use crate::layout::style::{Style, TextAlign};
use crate::layout::table::{self as layout_table, TableNode, TableSpec as LayoutTableSpec};
use crate::limits::Limits;

use super::token::DocToken;
use super::types::{default_style, numbering_kind, reject_auto_with_ratio, Rendered, Track};
use super::{
    Alignment, BandInset, BandSlot, BandSpec, BlankWidth as SpecBlankWidth, Block, CanvasMark,
    CanvasSpec, CellBlock, CellSpec, ColumnsSpec, DocumentSpec, HLineSpec, ImageRef, ImageSpec,
    InlineRun, LayerMark, LayerShape, ListKind, ListSpec, PageWhen, ParagraphSpec, Resources,
    SectionSpec, TextRun, TextStyle,
};

/// Lowers a data-only specification and records its laid-out pages.
///
/// `K` is the caller's font key (`Copy + Eq + Hash + Debug`). A key that was not
/// registered fails with [`ErrorKind::UnregisteredFont`]. A bold run whose family
/// has no bold face fails with [`ErrorKind::BoldFaceRequired`].
///
/// # Errors
///
/// Returns resource, construction, layout, or drawing failures collected for the document.
pub fn layout_spec<K>(spec: DocumentSpec<K>) -> Result<Rendered>
where
    K: Copy + Eq + std::hash::Hash + std::fmt::Debug,
{
    let DocumentSpec {
        page_size,
        resources,
        limits,
        info,
        mut sections,
    } = spec;
    if sections.is_empty() {
        sections.push(SectionSpec::default());
    }

    let doc = DocToken::next();
    let mut errors = ErrorBag::default();
    let (mut fonts, font_table, images, image_handles) =
        lower_resources(resources, limits, doc, &mut errors);
    let style = default_style();
    let mut configs = Vec::with_capacity(sections.len());
    let mut roots = Vec::with_capacity(sections.len());
    let mut decorations = Vec::with_capacity(sections.len());

    for (index, section) in sections.into_iter().enumerate() {
        let mark = errors.mark();
        validate_section_images(&section, &image_handles, &mut errors);
        let (config, root, decoration, section_errors) = lower_section(
            section,
            page_size,
            style.clone(),
            &font_table,
            doc,
            &image_handles,
        );
        errors.absorb(section_errors);
        errors.locate_since(mark, &Location::section(index));
        configs.push(config);
        roots.push(root);
        decorations.push(decoration);
    }

    if let Err(hard) = check_limits(&roots, &limits) {
        return Err(errors.fail(hard));
    }
    let jobs = configs
        .into_iter()
        .zip(roots)
        .zip(&decorations)
        .map(|((config, root), decoration)| SectionJob {
            config,
            root,
            decorations: decoration,
        })
        .collect();
    let laid = {
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        layout_pages(jobs, &mut ctx, limits.max_pages())
    };
    let laid = match laid {
        Ok(laid) => laid,
        Err(hard) => return Err(errors.fail(hard)),
    };
    let laid = errors.finish(laid)?;
    let mut recording = RecordingCanvas::new();
    paint_laid(&laid, &mut recording).map_err(crate::error::Errors::from)?;
    Ok(Rendered {
        recording,
        fonts,
        images,
        info,
    })
}

#[derive(Clone, Copy)]
struct ResolvedFamily {
    normal: FontId,
    bold: Option<FontId>,
}

#[derive(Clone)]
struct FontTable<K> {
    by_key: HashMap<K, ResolvedFamily>,
}

fn register_face(fonts: &mut FontStore, face: FontFace, errors: &mut ErrorBag) -> Option<FontId> {
    match fonts.register(FontSource {
        bytes: Arc::new(face.bytes),
        index: face.index,
    }) {
        Ok(id) => Some(id),
        Err(error) => {
            errors.record(error);
            None
        }
    }
}

fn resolve_font<K: FontKey>(
    fonts: &FontTable<K>,
    key: K,
    bold: bool,
    errors: &mut ErrorBag,
) -> Option<FontId> {
    let Some(family) = fonts.by_key.get(&key) else {
        errors.record(Error::from(ErrorKind::UnregisteredFont {
            key: format!("{key:?}"),
        }));
        return None;
    };
    if bold {
        if let Some(id) = family.bold {
            return Some(id);
        }
        errors.record(Error::from(ErrorKind::BoldFaceRequired));
        return Some(family.normal);
    }
    Some(family.normal)
}

fn lower_resources<K: FontKey>(
    resources: Resources<K>,
    limits: Limits,
    doc: DocToken,
    errors: &mut ErrorBag,
) -> (
    FontStore,
    FontTable<K>,
    Box<dyn ImageRegistry>,
    LoweredImages,
) {
    let mut fonts = FontStore::new();
    fonts.set_max_bytes(limits.max_font_bytes());
    let mut by_key = HashMap::new();
    for (key, family) in resources.fonts.into_entries() {
        let Some(normal) = register_face(&mut fonts, family.normal, errors) else {
            continue;
        };
        let bold = family
            .bold
            .and_then(|face| register_face(&mut fonts, face, errors));
        by_key.insert(key, ResolvedFamily { normal, bold });
    }

    let mut registry = crate::image_registry();
    registry.set_limits(&limits);
    let mut handles = Vec::with_capacity(resources.images.len());
    for image in resources.images {
        match registry.register(image.bytes, image.format) {
            Ok(decoded) => handles.push(Some(HeldImage { decoded, doc })),
            Err(error) => {
                errors.record(error);
                handles.push(None);
            }
        }
    }
    (
        fonts,
        FontTable { by_key },
        registry,
        LoweredImages {
            resources: resources.token,
            handles,
        },
    )
}

#[derive(Clone, Copy)]
struct HeldImage {
    decoded: DecodedImage,
    doc: DocToken,
}

#[derive(Clone)]
struct LoweredImages {
    resources: DocToken,
    handles: Vec<Option<HeldImage>>,
}

fn lower_section<K: FontKey>(
    spec: SectionSpec<K>,
    default_page: PageSize,
    style: Style,
    fonts: &FontTable<K>,
    doc: DocToken,
    images: &LoweredImages,
) -> (PaginateConfig, FlowNode, Decorations<'static>, ErrorBag) {
    let SectionSpec {
        page_size,
        margins,
        header,
        footer,
        behind,
        front,
        body,
    } = spec;
    let mut errors = ErrorBag::default();
    let nodes = lower_blocks(body, &style, fonts, doc, images, &mut errors);
    let header = header.and_then(|spec| own_band(spec, fonts, &mut errors));
    let footer = footer.and_then(|spec| own_band(spec, fonts, &mut errors));
    let behind = own_marks(behind, fonts, &mut errors);
    let front = own_marks(front, fonts, &mut errors);
    let decorations = Decorations {
        header: header.map(|spec| band_content(spec, style.clone())),
        footer: footer.map(|spec| band_content(spec, style.clone())),
        behind: layer_content(behind, style.clone(), doc, images.clone()),
        front: layer_content(front, style, doc, images.clone()),
    };
    let config = PaginateConfig {
        page_size: page_size.unwrap_or(default_page).resolve(),
        margins,
    };
    (config, FlowNode::Stack(nodes), decorations, errors)
}

fn lower_blocks<K: FontKey>(
    blocks: Vec<Block<K>>,
    style: &Style,
    fonts: &FontTable<K>,
    doc: DocToken,
    images: &LoweredImages,
    errors: &mut ErrorBag,
) -> Vec<FlowNode> {
    let mut nodes = Vec::new();
    let mut style = style.clone();
    for block in blocks {
        match block {
            Block::Paragraph(spec) => {
                if let Some(node) = lower_paragraph(spec, &style, fonts, errors) {
                    nodes.push(node);
                }
            }
            Block::Spacer(height) => nodes.push(FlowNode::Spacer(SpacerNode { height })),
            Block::PageBreak => nodes.push(FlowNode::PageBreak),
            Block::KeepTogether(children) => {
                let child = lower_blocks(children, &style, fonts, doc, images, errors);
                nodes.push(FlowNode::KeepTogether(KeepTogetherNode::Open(child)));
            }
            Block::Styled {
                style: patch,
                children,
            } => {
                let saved = style.clone();
                apply_text_style(&mut style, patch);
                nodes.extend(lower_blocks(children, &style, fonts, doc, images, errors));
                style = saved;
            }
            Block::Columns(spec) => {
                if let Some(node) = lower_columns(spec, &style, fonts, doc, images, errors) {
                    nodes.push(node);
                }
            }
            Block::Table(spec) => {
                if let Some(node) = lower_table(spec, &style, fonts, doc, images, errors) {
                    nodes.push(node);
                }
            }
            Block::List(spec) => {
                if let Some(node) = lower_list(spec, &style, fonts, doc, images, errors) {
                    nodes.push(node);
                }
            }
            Block::Image(spec) => {
                if let Some(node) = lower_image(spec, doc, images, errors) {
                    nodes.push(node);
                }
            }
            Block::Canvas(spec) => {
                if let Some(node) = lower_canvas(spec, fonts, errors) {
                    nodes.push(node);
                }
            }
            Block::HLine(spec) => nodes.push(lower_hline(spec)),
        }
    }
    nodes
}

fn apply_text_style(style: &mut Style, patch: TextStyle) {
    if let Some(size) = patch.font_size {
        style.font_size = size;
    }
    if let Some(color) = patch.color {
        style.color = color;
    }
}

fn lower_paragraph<K: FontKey>(
    spec: ParagraphSpec<K>,
    parent: &Style,
    fonts: &FontTable<K>,
    errors: &mut ErrorBag,
) -> Option<FlowNode> {
    let mut bag = ErrorBag::default();
    let mut items = Vec::new();
    let mut active_link = None;
    push_inline_runs(spec.runs, fonts, &mut items, &mut active_link, &mut bag);
    let structural = bag.as_slice().iter().any(|error| {
        !matches!(
            error.kind,
            ErrorKind::BoldFaceRequired | ErrorKind::UnregisteredFont { .. }
        )
    });
    let font_only_empty = items.is_empty()
        && bag.as_slice().iter().any(|error| {
            matches!(
                error.kind,
                ErrorKind::UnregisteredFont { .. } | ErrorKind::BoldFaceRequired
            )
        });
    errors.absorb(bag);
    if structural || font_only_empty {
        return None;
    }
    let mut style = parent.clone();
    if let Some(font) = first_run_font(&items) {
        style.font = font;
    }
    style.first_line_indent = spec.first_line_indent;
    style.align = match spec.align {
        Alignment::Start => TextAlign::Start,
        Alignment::Center => TextAlign::Center,
        Alignment::End => TextAlign::End,
    };
    if let Some(color) = spec.color {
        style.color = color;
    }
    if let Some(font_size) = spec.font_size {
        style.font_size = font_size;
    }
    Some(FlowNode::Text(TextNode::from_items(items, style)))
}

fn first_run_font(items: &[InlineItem]) -> Option<FontId> {
    items.iter().find_map(|item| match item {
        InlineItem::Text(run) => Some(run.font),
        InlineItem::Blank(_) => None,
    })
}

fn push_inline_runs<K: FontKey>(
    runs: Vec<InlineRun<K>>,
    fonts: &FontTable<K>,
    items: &mut Vec<InlineItem>,
    active_link: &mut Option<LinkTarget>,
    errors: &mut ErrorBag,
) {
    for run in runs {
        match run {
            InlineRun::Text { text, font } => {
                if text.is_empty() {
                    continue;
                }
                if let Some(id) = resolve_font(fonts, font, false, errors) {
                    append_text(items, id, &text, active_link.as_ref(), errors);
                }
            }
            InlineRun::Bold { text, font } => {
                if text.is_empty() {
                    continue;
                }
                if let Some(id) = resolve_font(fonts, font, true, errors) {
                    append_text(items, id, &text, active_link.as_ref(), errors);
                }
            }
            InlineRun::Link { uri, runs } => match LinkTarget::uri(&uri) {
                Ok(target) => {
                    let previous = active_link.replace(target);
                    push_inline_runs(runs, fonts, items, active_link, errors);
                    *active_link = previous;
                }
                Err(error) => errors.record(error),
            },
            InlineRun::Blank(spec) => {
                if sealed(items) {
                    errors.record(Error::from(ErrorKind::RunAfterFillBlank));
                    continue;
                }
                let width = match spec.width {
                    SpecBlankWidth::Fixed(width) => BlankWidth::Fixed(width),
                    SpecBlankWidth::Em(ems) => BlankWidth::Em(ems),
                    SpecBlankWidth::ToEndOfLine => BlankWidth::ToEndOfLine,
                };
                let mut blank = InlineBlank::new(width);
                if let Some(thickness) = spec.thickness {
                    blank.thickness = thickness;
                }
                if let Some(color) = spec.color {
                    blank.color = Some(color);
                }
                if let Some(offset) = spec.baseline_offset {
                    blank.baseline_offset = offset;
                }
                items.push(InlineItem::Blank(blank));
            }
        }
    }
}

fn sealed(items: &[InlineItem]) -> bool {
    matches!(
        items.last(),
        Some(InlineItem::Blank(blank)) if matches!(blank.width, BlankWidth::ToEndOfLine)
    )
}

fn append_text(
    items: &mut Vec<InlineItem>,
    font: FontId,
    text: &str,
    active_link: Option<&LinkTarget>,
    errors: &mut ErrorBag,
) {
    if text.is_empty() {
        return;
    }
    if sealed(items) {
        errors.record(Error::from(ErrorKind::RunAfterFillBlank));
        return;
    }
    if let Some(InlineItem::Text(last)) = items.last_mut() {
        if last.font == font && last.link.as_ref() == active_link {
            last.text.push_str(text);
            return;
        }
    }
    items.push(InlineItem::Text(
        LayoutTextRun::new(text, font).with_link(active_link.cloned()),
    ));
}

fn lower_hline(spec: HLineSpec) -> FlowNode {
    let align = match spec.align {
        Alignment::Start => TextAlign::Start,
        Alignment::Center => TextAlign::Center,
        Alignment::End => TextAlign::End,
    };
    FlowNode::HLine(HLineNode::new(
        spec.width,
        align,
        spec.thickness,
        spec.color,
        spec.space_before,
        spec.space_after,
    ))
}

fn lower_image(
    spec: ImageSpec,
    doc: DocToken,
    images: &LoweredImages,
    errors: &mut ErrorBag,
) -> Option<FlowNode> {
    let image = image_for(spec.image, images)?;
    if image.doc != doc {
        errors.record(Error::from(ErrorKind::UnknownImage {
            id: image.decoded.id.as_u32(),
        }));
        return None;
    }
    let align = match spec.align {
        Alignment::Start => TextAlign::Start,
        Alignment::Center => TextAlign::Center,
        Alignment::End => TextAlign::End,
    };
    match ImageNode::new(image.decoded, spec.width, spec.height, align) {
        Ok(node) => Some(FlowNode::Image(node)),
        Err(error) => {
            errors.record(error);
            None
        }
    }
}

fn text_align(align: Alignment) -> TextAlign {
    match align {
        Alignment::Start => TextAlign::Start,
        Alignment::Center => TextAlign::Center,
        Alignment::End => TextAlign::End,
    }
}

fn lower_canvas<K: FontKey>(
    spec: CanvasSpec<K>,
    fonts: &FontTable<K>,
    errors: &mut ErrorBag,
) -> Option<FlowNode> {
    let mut marks = Vec::with_capacity(spec.marks.len());
    for mark in spec.marks {
        match mark {
            CanvasMark::Line { from, to, pen } => {
                marks.push(CanvasMarkNode::Line { from, to, pen });
            }
            CanvasMark::Rect { rect, ink } => marks.push(CanvasMarkNode::Rect { rect, ink }),
            CanvasMark::Fill { path, color } => marks.push(CanvasMarkNode::Fill { path, color }),
            CanvasMark::Text {
                text,
                font,
                at,
                style,
            } => {
                if text.is_empty() {
                    errors.record(Error::from(ErrorKind::EmptyCanvasText));
                    return None;
                }
                let id = resolve_font(fonts, font, false, errors)?;
                marks.push(CanvasMarkNode::Text {
                    text,
                    font: id,
                    at,
                    size: style.font_size.unwrap_or_else(seed_font_size),
                    color: style.color.unwrap_or_else(seed_color),
                });
            }
        }
    }
    match CanvasNode::new(spec.size, text_align(spec.align), marks) {
        Ok(node) => Some(FlowNode::Canvas(node)),
        Err(error) => {
            errors.record(error);
            None
        }
    }
}

/// Document seed from `default_style`, not an enclosing `Block::Styled`.
fn seed_font_size() -> Length<Positive> {
    Positive::pt(12.0).expect("12 pt is positive")
}

fn seed_color() -> Color {
    Color::rgb(0, 0, 0)
}

fn lower_columns<K: FontKey>(
    spec: ColumnsSpec<K>,
    style: &Style,
    fonts: &FontTable<K>,
    doc: DocToken,
    images: &LoweredImages,
    errors: &mut ErrorBag,
) -> Option<FlowNode> {
    let mut bag = ErrorBag::default();
    let mut cols = Vec::new();
    for column in spec.columns {
        let width = match column.width {
            Track::Fixed(width) => ColumnWidth::Fixed(width),
            Track::Ratio(ratio) => match LayoutRatio::new(ratio.get()) {
                Ok(ratio) => ColumnWidth::Ratio(ratio),
                Err(error) => {
                    bag.record(error);
                    continue;
                }
            },
            Track::Auto => ColumnWidth::Auto,
        };
        let mark = bag.mark();
        let nodes = lower_blocks(column.blocks, style, fonts, doc, images, &mut bag);
        if bag.clean_since(mark) {
            cols.push((width, nodes));
        }
    }
    let widths: Vec<_> = cols.iter().map(|(width, _)| *width).collect();
    if let Err(error) = reject_auto_with_ratio(&widths) {
        bag.record(error);
    }
    let failed = !bag.is_empty();
    errors.absorb(bag);
    if failed {
        return None;
    }
    match ColumnsNode::new(spec.gap, cols) {
        Ok(node) => Some(FlowNode::Columns(node)),
        Err(error) => {
            errors.record(error);
            None
        }
    }
}

fn lower_table<K: FontKey>(
    spec: super::TableSpec<K>,
    style: &Style,
    fonts: &FontTable<K>,
    doc: DocToken,
    images: &LoweredImages,
    errors: &mut ErrorBag,
) -> Option<FlowNode> {
    let mut bag = ErrorBag::default();
    let first = spec.header.first().or(spec.body.first());
    let cols: usize = first.map_or(0, |row| {
        row.iter()
            .map(|cell| usize::from(cell.col_span.get()))
            .sum()
    });
    let header = lower_table_rows(spec.header, style, fonts, doc, images, &mut bag);
    let body = lower_table_rows(spec.body, style, fonts, doc, images, &mut bag);
    let mut blocked = !bag.is_empty() || header.is_none() || body.is_none();
    let widths = match spec.tracks {
        Some(tracks) => table_widths(tracks, cols, &mut blocked, &mut bag),
        None => Some(vec![
            ColumnWidth::Ratio(
                LayoutRatio::new(1.0).expect("1 is positive")
            );
            cols
        ]),
    };
    if let Some(widths) = widths.as_ref() {
        if let Err(error) = reject_auto_with_ratio(widths) {
            bag.record(error);
            blocked = true;
        }
    }
    let failed =
        blocked || !bag.is_empty() || header.is_none() || body.is_none() || widths.is_none();
    errors.absorb(bag);
    if failed {
        return None;
    }
    let (Some(header), Some(body), Some(widths)) = (header, body, widths) else {
        return None;
    };
    let layout_spec = LayoutTableSpec {
        header,
        body,
        repeat_header: spec.repeat_header,
        padding: spec.cell_padding,
        border: layout_table::Border {
            width: spec.border_width,
            color: spec.border_color,
        },
        ..LayoutTableSpec::new(widths)
    };
    match TableNode::new(layout_spec) {
        Ok(node) => Some(FlowNode::Table(node)),
        Err(error) => {
            errors.record(error);
            None
        }
    }
}

fn table_widths(
    tracks: Vec<Track>,
    cols: usize,
    blocked: &mut bool,
    into: &mut ErrorBag,
) -> Option<Vec<ColumnWidth>> {
    if tracks.len() != cols {
        into.record(Error::from(ErrorKind::ColumnWidths {
            reason: ColumnWidthsReason::ColumnCount,
        }));
        *blocked = true;
        return None;
    }
    let mut widths = Vec::new();
    for track in tracks {
        match track.into_width() {
            Ok(width) => widths.push(width),
            Err(error) => {
                into.record(error);
                *blocked = true;
            }
        }
    }
    if *blocked {
        None
    } else {
        Some(widths)
    }
}

fn lower_table_rows<K: FontKey>(
    rows: Vec<Vec<CellSpec<K>>>,
    style: &Style,
    fonts: &FontTable<K>,
    doc: DocToken,
    images: &LoweredImages,
    errors: &mut ErrorBag,
) -> Option<Vec<Vec<layout_table::Cell<FlowNode>>>> {
    let mut out = Vec::with_capacity(rows.len());
    let mut ok = true;
    for row in rows {
        let mut stored = Vec::with_capacity(row.len());
        for cell in row {
            match lower_cell(cell, style, fonts, doc, images, errors) {
                Some(built) => stored.push(built),
                None => ok = false,
            }
        }
        out.push(stored);
    }
    ok.then_some(out)
}

fn lower_cell<K: FontKey>(
    spec: CellSpec<K>,
    style: &Style,
    fonts: &FontTable<K>,
    doc: DocToken,
    images: &LoweredImages,
    errors: &mut ErrorBag,
) -> Option<layout_table::Cell<FlowNode>> {
    let mut bag = ErrorBag::default();
    let mut content = Vec::new();
    for block in spec.content {
        match block {
            CellBlock::Text { text, font } => {
                if text.is_empty() {
                    continue;
                }
                if let Some(id) = resolve_font(fonts, font, false, &mut bag) {
                    let mut cell_style = style.clone();
                    cell_style.font = id;
                    content.push(FlowNode::Text(TextNode::new(text, cell_style)));
                }
            }
            CellBlock::Paragraph(paragraph) => {
                if let Some(node) = lower_paragraph(paragraph, style, fonts, &mut bag) {
                    content.push(node);
                }
            }
            CellBlock::HLine(line) => content.push(lower_hline(line)),
            CellBlock::Image(image) => {
                if let Some(node) = lower_image(image, doc, images, &mut bag) {
                    content.push(node);
                }
            }
            CellBlock::Canvas(canvas) => {
                if let Some(node) = lower_canvas(canvas, fonts, &mut bag) {
                    content.push(node);
                }
            }
            CellBlock::Spacer(height) => {
                content.push(FlowNode::Spacer(SpacerNode { height }));
            }
            CellBlock::List(list) => {
                if let Some(node) = lower_list(list, style, fonts, doc, images, &mut bag) {
                    content.push(node);
                }
            }
        }
    }
    let failed = !bag.is_empty();
    errors.absorb(bag);
    if failed {
        return None;
    }
    match layout_table::Cell::new(content) {
        Ok(built) => Some(
            built
                .with_span(spec.col_span, spec.row_span)
                .with_fill(spec.fill),
        ),
        Err(error) => {
            errors.record(error);
            None
        }
    }
}

fn lower_list<K: FontKey>(
    spec: ListSpec<K>,
    style: &Style,
    fonts: &FontTable<K>,
    doc: DocToken,
    images: &LoweredImages,
    errors: &mut ErrorBag,
) -> Option<FlowNode> {
    let mut bag = ErrorBag::default();
    let mut items = Vec::new();
    for item in spec.items {
        let mark = bag.mark();
        let nodes = lower_blocks(item.blocks, style, fonts, doc, images, &mut bag);
        if !bag.clean_since(mark) {
            continue;
        }
        let built = match item.marker {
            Some(marker) => ListItem::custom(marker, nodes),
            None => ListItem::new(nodes),
        };
        match built {
            Ok(item) => items.push(item),
            Err(error) => bag.record(error),
        }
    }
    let kind = match spec.kind {
        ListKind::Bullet => LayoutListKind::Bullet,
        ListKind::Ordered(numbering) => LayoutListKind::Ordered {
            style: numbering_kind(numbering),
            start: spec.start,
        },
    };
    let failed = !bag.is_empty();
    errors.absorb(bag);
    if failed {
        return None;
    }
    let layout_spec = LayoutListSpec {
        kind,
        marker_width: spec.marker_width,
        nest_indent: spec.nest_indent,
        marker_style: style.clone(),
        items,
    };
    match ListNode::new(layout_spec) {
        Ok(node) => Some(FlowNode::List(node)),
        Err(error) => {
            errors.record(error);
            None
        }
    }
}

struct OwnedBand {
    inset: BandInset,
    left: Option<OwnedSlot>,
    right: Option<OwnedSlot>,
}

struct OwnedSlot {
    runs: Vec<TextRun>,
    uri: Option<Vec<TextRun>>,
    font: FontId,
}

fn own_band<K: FontKey>(
    spec: BandSpec<K>,
    fonts: &FontTable<K>,
    errors: &mut ErrorBag,
) -> Option<OwnedBand> {
    let BandSpec { inset, left, right } = spec;
    let left = left.and_then(|slot| own_slot(slot, fonts, errors));
    let right = right.and_then(|slot| own_slot(slot, fonts, errors));
    if left.is_none() && right.is_none() {
        return None;
    }
    Some(OwnedBand { inset, left, right })
}

fn own_slot<K: FontKey>(
    slot: BandSlot<K>,
    fonts: &FontTable<K>,
    errors: &mut ErrorBag,
) -> Option<OwnedSlot> {
    let (runs, uri, key) = match slot {
        BandSlot::Text { runs, font } => (runs, None, font),
        BandSlot::Link { uri, text, font } => (text, Some(uri), font),
    };
    let font = resolve_font(fonts, key, false, errors)?;
    Some(OwnedSlot { runs, uri, font })
}

fn band_content(spec: OwnedBand, style: Style) -> crate::layout::decorate::BandContent<'static> {
    Box::new(move |info, errors| Ok(lower_band(&spec, info, &style, errors)))
}

#[derive(Clone, Copy)]
struct PageFacts {
    number: u32,
    count: u32,
    section_number: u32,
    section_count: u32,
    margins: Margins,
}

impl From<&PageInfo> for PageFacts {
    fn from(info: &PageInfo) -> Self {
        Self {
            number: info.number.get(),
            count: info.count.get(),
            section_number: info.section_number.get(),
            section_count: info.section_count.get(),
            margins: info.margins,
        }
    }
}

impl PageFacts {
    fn is_last(self) -> bool {
        self.number == self.count
    }
}

fn lower_band(
    spec: &OwnedBand,
    info: &PageInfo,
    style: &Style,
    errors: &mut ErrorBag,
) -> Vec<FlowNode> {
    let page = PageFacts::from(info);
    let (inset_left, inset_right) = match spec.inset {
        BandInset::Fixed { left, right } => (left, right),
        BandInset::PageMargins => (page.margins.left(), page.margins.right()),
    };
    let left = spec
        .left
        .as_ref()
        .and_then(|slot| band_slot(slot, &page, errors));
    let right = spec
        .right
        .as_ref()
        .and_then(|slot| band_slot(slot, &page, errors));

    let column = |slot: BandSlotResolved, align: TextAlign| {
        let mut style = style.clone();
        style.align = align;
        style.font = slot.font;
        let node = if slot.text.is_empty() {
            TextNode::from_items(Vec::new(), style)
        } else {
            TextNode::from_runs(
                vec![LayoutTextRun::new(slot.text, slot.font).with_link(slot.link)],
                style,
            )
        };
        vec![FlowNode::Text(node)]
    };
    let even = ColumnWidth::Ratio(LayoutRatio::new(1.0).expect("1 is positive"));
    let slots: Vec<_> = [(left, TextAlign::Start), (right, TextAlign::End)]
        .into_iter()
        .filter_map(|(text, align)| text.map(|text| (even, column(text, align))))
        .collect();
    if slots.is_empty() {
        return Vec::new();
    }
    let mut spacer = |pad: Length<NonNegative>| -> Option<(ColumnWidth, Vec<FlowNode>)> {
        if pad.as_pt() == 0.0 {
            return None;
        }
        match Positive::pt(pad.as_pt()) {
            Ok(width) => Some((ColumnWidth::Fixed(width), Vec::new())),
            Err(error) => {
                errors.record(error);
                None
            }
        }
    };
    let mut tracks: Vec<_> = spacer(inset_left).into_iter().collect();
    tracks.extend(slots);
    tracks.extend(spacer(inset_right));
    if tracks.len() == 1 {
        return tracks.remove(0).1;
    }
    match ColumnsNode::new(NonNegative::ZERO, tracks) {
        Ok(node) => vec![FlowNode::Columns(node)],
        Err(error) => {
            errors.record(error);
            Vec::new()
        }
    }
}

struct BandSlotResolved {
    text: String,
    link: Option<LinkTarget>,
    font: FontId,
}

fn band_slot(
    slot: &OwnedSlot,
    page: &PageFacts,
    errors: &mut ErrorBag,
) -> Option<BandSlotResolved> {
    let text = render_text(&slot.runs, page);
    let link = if let Some(uri_runs) = &slot.uri {
        let uri = render_text(uri_runs, page);
        match LinkTarget::uri(&uri) {
            Ok(target) => Some(target),
            Err(error) => {
                errors.record(error);
                return None;
            }
        }
    } else {
        None
    };
    Some(BandSlotResolved {
        text,
        link,
        font: slot.font,
    })
}

fn render_text(runs: &[TextRun], page: &PageFacts) -> String {
    let mut text = String::new();
    for run in runs {
        match run {
            TextRun::Literal(value) => text.push_str(value),
            TextRun::PageNumber => text.push_str(&page.number.to_string()),
            TextRun::PageCount => text.push_str(&page.count.to_string()),
            TextRun::SectionNumber => text.push_str(&page.section_number.to_string()),
            TextRun::SectionCount => text.push_str(&page.section_count.to_string()),
        }
    }
    text
}

enum OwnedMark {
    Text {
        when: PageWhen,
        text: Vec<TextRun>,
        font: FontId,
        at: Point,
        style: TextStyle,
    },
    Image {
        when: PageWhen,
        image: ImageRef,
        rect: Rect,
    },
    Line {
        when: PageWhen,
        from: Point,
        to: Point,
        pen: Pen,
    },
    Rect {
        when: PageWhen,
        rect: Rect,
        ink: Ink,
    },
    Ellipse {
        when: PageWhen,
        rect: Rect,
        ink: Ink,
    },
    Rotated {
        when: PageWhen,
        degrees: f32,
        pivot: Point,
        shapes: Vec<LayerShape>,
    },
}

fn own_marks<K: FontKey>(
    marks: Vec<LayerMark<K>>,
    fonts: &FontTable<K>,
    errors: &mut ErrorBag,
) -> Vec<OwnedMark> {
    let mut owned = Vec::with_capacity(marks.len());
    for mark in marks {
        match mark {
            LayerMark::Text {
                when,
                text,
                font,
                at,
                style,
            } => {
                let Some(font) = resolve_font(fonts, font, false, errors) else {
                    continue;
                };
                owned.push(OwnedMark::Text {
                    when,
                    text,
                    font,
                    at,
                    style,
                });
            }
            LayerMark::Image { when, image, rect } => {
                owned.push(OwnedMark::Image { when, image, rect });
            }
            LayerMark::Line {
                when,
                from,
                to,
                pen,
            } => owned.push(OwnedMark::Line {
                when,
                from,
                to,
                pen,
            }),
            LayerMark::Rect { when, rect, ink } => {
                owned.push(OwnedMark::Rect { when, rect, ink });
            }
            LayerMark::Ellipse { when, rect, ink } => {
                owned.push(OwnedMark::Ellipse { when, rect, ink });
            }
            LayerMark::Rotated {
                when,
                degrees,
                pivot,
                shapes,
            } => owned.push(OwnedMark::Rotated {
                when,
                degrees,
                pivot,
                shapes,
            }),
        }
    }
    owned
}

fn layer_content(
    marks: Vec<OwnedMark>,
    style: Style,
    doc: DocToken,
    images: LoweredImages,
) -> Option<LayerContent<'static>> {
    if marks.is_empty() {
        return None;
    }
    Some(Box::new(move |info, errors| {
        Ok(lower_layer_marks(
            &marks, info, &style, doc, &images, errors,
        ))
    }))
}

fn lower_layer_marks(
    marks: &[OwnedMark],
    info: &PageInfo,
    style: &Style,
    doc: DocToken,
    images: &LoweredImages,
    errors: &mut ErrorBag,
) -> Vec<LayerItem> {
    let page = PageFacts::from(info);
    let mut items = Vec::new();
    for mark in marks {
        let when = match mark {
            OwnedMark::Text { when, .. }
            | OwnedMark::Image { when, .. }
            | OwnedMark::Line { when, .. }
            | OwnedMark::Rect { when, .. }
            | OwnedMark::Ellipse { when, .. }
            | OwnedMark::Rotated { when, .. } => *when,
        };
        if !when.matches(&page) {
            continue;
        }
        match mark {
            OwnedMark::Text {
                text,
                font,
                at,
                style: patch,
                ..
            } => {
                let mut item_style = style.clone();
                apply_text_style(&mut item_style, *patch);
                item_style.font = *font;
                items.push(LayerItem::Text(LayerText {
                    text: render_text(text, &page),
                    at: *at,
                    style: item_style,
                }));
            }
            OwnedMark::Image { image, rect, .. } => {
                if let Some(image) = image_for(*image, images) {
                    if image.doc == doc {
                        items.push(LayerItem::Image {
                            image: image.decoded,
                            rect: *rect,
                        });
                    } else {
                        errors.record(Error::from(ErrorKind::UnknownImage {
                            id: image.decoded.id.as_u32(),
                        }));
                    }
                }
            }
            OwnedMark::Line { from, to, pen, .. } => {
                push_shape(
                    &mut items,
                    errors,
                    straight_path(*from, *to),
                    Ink::Stroke(*pen),
                    None,
                );
            }
            OwnedMark::Rect { rect, ink, .. } => {
                push_shape(&mut items, errors, Path::rect(*rect), *ink, None);
            }
            OwnedMark::Ellipse { rect, ink, .. } => {
                push_shape(&mut items, errors, Path::ellipse(*rect), *ink, None);
            }
            OwnedMark::Rotated {
                degrees,
                pivot,
                shapes,
                ..
            } => {
                if !degrees.is_finite() {
                    errors.record(GeometryKind::NonFiniteRotation { degrees: *degrees });
                    continue;
                }
                for shape in shapes {
                    match shape {
                        LayerShape::Line { from, to, pen } => push_shape(
                            &mut items,
                            errors,
                            straight_path(*from, *to),
                            Ink::Stroke(*pen),
                            Some((*degrees, *pivot)),
                        ),
                        LayerShape::Rect { rect, ink } => push_shape(
                            &mut items,
                            errors,
                            Path::rect(*rect),
                            *ink,
                            Some((*degrees, *pivot)),
                        ),
                        LayerShape::Ellipse { rect, ink } => push_shape(
                            &mut items,
                            errors,
                            Path::ellipse(*rect),
                            *ink,
                            Some((*degrees, *pivot)),
                        ),
                    }
                }
            }
        }
    }
    items
}

fn push_shape(
    items: &mut Vec<LayerItem>,
    errors: &mut ErrorBag,
    path: Fallible<Path>,
    ink: Ink,
    turn: Option<(f32, Point)>,
) {
    let path = path.and_then(|path| match turn {
        Some((degrees, about)) if !is_zero_angle(degrees) => path.rotated(degrees, about),
        _ => Ok(path),
    });
    match path {
        Ok(path) => items.push(LayerItem::Shape { path, ink }),
        Err(error) => errors.record(error),
    }
}

fn straight_path(from: Point, to: Point) -> Fallible<Path> {
    if from == to {
        Err(GeometryKind::ZeroLengthSegment.into())
    } else {
        Ok(Path::line(from, to))
    }
}

fn is_zero_angle(degrees: f32) -> bool {
    degrees.to_bits() == 0 || degrees.to_bits() == (-0.0f32).to_bits()
}

impl PageWhen {
    fn matches(self, page: &PageFacts) -> bool {
        match self {
            Self::All => true,
            Self::Last => page.is_last(),
        }
    }
}

fn validate_section_images<K>(
    section: &SectionSpec<K>,
    images: &LoweredImages,
    errors: &mut ErrorBag,
) {
    validate_marks(&section.behind, images, errors);
    validate_marks(&section.front, images, errors);
    validate_blocks(&section.body, images, errors);
}

fn validate_blocks<K>(blocks: &[Block<K>], images: &LoweredImages, errors: &mut ErrorBag) {
    for block in blocks {
        match block {
            Block::KeepTogether(children) | Block::Styled { children, .. } => {
                validate_blocks(children, images, errors);
            }
            Block::Columns(spec) => {
                for column in &spec.columns {
                    validate_blocks(&column.blocks, images, errors);
                }
            }
            Block::Table(spec) => {
                for row in spec.header.iter().chain(spec.body.iter()) {
                    for cell in row {
                        validate_cell_blocks(&cell.content, images, errors);
                    }
                }
            }
            Block::List(spec) => {
                for item in &spec.items {
                    validate_blocks(&item.blocks, images, errors);
                }
            }
            Block::Image(spec) => reject_image(spec.image, images, errors),
            Block::Paragraph(_)
            | Block::Spacer(_)
            | Block::PageBreak
            | Block::HLine(_)
            | Block::Canvas(_) => {}
        }
    }
}

fn validate_cell_blocks<K>(blocks: &[CellBlock<K>], images: &LoweredImages, errors: &mut ErrorBag) {
    for block in blocks {
        match block {
            CellBlock::Image(spec) => reject_image(spec.image, images, errors),
            CellBlock::List(spec) => {
                for item in &spec.items {
                    validate_blocks(&item.blocks, images, errors);
                }
            }
            CellBlock::Text { .. }
            | CellBlock::Paragraph(_)
            | CellBlock::HLine(_)
            | CellBlock::Spacer(_)
            | CellBlock::Canvas(_) => {}
        }
    }
}

fn validate_marks<K>(marks: &[LayerMark<K>], images: &LoweredImages, errors: &mut ErrorBag) {
    for mark in marks {
        if let LayerMark::Image { image, .. } = mark {
            reject_image(*image, images, errors);
        }
    }
}

fn reject_image(image: ImageRef, images: &LoweredImages, errors: &mut ErrorBag) {
    if unknown_image(image, images) {
        errors.record(Error::from(ErrorKind::UnknownImage { id: image.index }));
    }
}

fn image_for(image: ImageRef, images: &LoweredImages) -> Option<HeldImage> {
    if unknown_image(image, images) {
        return None;
    }
    images
        .handles
        .get(image.index as usize)
        .and_then(|slot| *slot)
}

fn unknown_image(image: ImageRef, images: &LoweredImages) -> bool {
    image.resources != images.resources || images.handles.get(image.index as usize).is_none()
}
