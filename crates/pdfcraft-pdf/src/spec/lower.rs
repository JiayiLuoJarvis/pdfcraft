//! Pure-function lowering from [`DocumentSpec`] to layout nodes.

use std::collections::HashMap;

use crate::canvas::{DecodedImage, ImageRegistry, LinkTarget, RecordingCanvas};
use crate::error::{
    ColumnWidthsReason, DecorationKind, Error, ErrorBag, ErrorKind, Fallible, GeometryKind,
    Location, Result,
};
use crate::fonts::{EmOffset, FaceDecorations, FontFace, FontId, FontKey, FontSource, FontStore};
use crate::geom::{
    Color, Finite, Ink, Length, Margins, NonNegative, PageSize, Path, Pen, Point, Positive, Rect,
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
use crate::layout::style::{Decoration, GlyphStyle, Style, TextAlign};
use crate::layout::table::{self as layout_table, TableNode, TableSpec as LayoutTableSpec};
use crate::limits::Limits;

use super::token::DocToken;
use super::types::{default_style, numbering_kind, reject_auto_with_ratio, Rendered, Track};
use super::{
    Alignment, BandInset, BandSlot, BandSpec, BlankWidth as SpecBlankWidth, Block, CanvasMark,
    CanvasSpec, CellBlock, CellSpec, ColumnsSpec, DocumentSpec, HLineSpec, ImageRef, ImageSpec,
    InlineRun, LayerMark, LayerShape, ListKind, ListSpec, PageWhen, ParagraphSpec, Resources,
    RunStyle, SectionSpec, TextRun,
};

/// Lowers a data-only specification and records its laid-out pages.
///
/// `K` is the caller's font key (`Copy + Eq + Hash + Debug`). A key that was not
/// registered fails with [`ErrorKind::UnregisteredFont`]. A run whose family lacks
/// the face its style needs fails with [`ErrorKind::BoldFaceRequired`],
/// [`ErrorKind::ItalicFaceRequired`], or [`ErrorKind::BoldItalicFaceRequired`]. An
/// underline or strikethrough the regular face has no metrics for fails with
/// [`ErrorKind::MissingDecorationMetrics`]. Tracking that would give a glyph a
/// negative advance fails with [`ErrorKind::NegativeTracking`].
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
    let mut configs = Vec::with_capacity(sections.len());
    let mut roots = Vec::with_capacity(sections.len());
    let mut decorations = Vec::with_capacity(sections.len());

    for (index, section) in sections.into_iter().enumerate() {
        let mark = errors.mark();
        validate_section_images(&section, &image_handles, &mut errors);
        let (config, root, decoration, section_errors) =
            lower_section(section, page_size, &font_table, doc, &image_handles);
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
    regular: FontId,
    bold: Option<FontId>,
    italic: Option<FontId>,
    bold_italic: Option<FontId>,
    decorations: FaceDecorations,
}

impl ResolvedFamily {
    /// List markers draw with the first registered face and never pick another.
    fn first_face() -> Self {
        Self {
            regular: FontId::from_raw(0),
            bold: None,
            italic: None,
            bold_italic: None,
            decorations: FaceDecorations::default(),
        }
    }
}

#[derive(Clone)]
struct FontTable<K> {
    by_key: HashMap<K, ResolvedFamily>,
}

/// Bold and italic flags. Together they pick one of the family's four faces.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FaceFlags {
    bold: bool,
    italic: bool,
}

/// Which decoration lines a run draws.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LineFlags {
    underline: bool,
    strikethrough: bool,
}

/// Inherited run values with every field decided.
#[derive(Debug, Clone, Copy, PartialEq)]
struct InheritedPaint {
    face: FaceFlags,
    lines: LineFlags,
    tracking: Length<Finite>,
    color: Color,
    font_size: Length<Positive>,
}

impl InheritedPaint {
    fn seed() -> Self {
        Self {
            face: FaceFlags::default(),
            lines: LineFlags::default(),
            tracking: Finite::ZERO,
            color: Color::rgb(0, 0, 0),
            font_size: Positive::pt(12.0).expect("12 pt is positive"),
        }
    }

    /// `None` keeps the outer value. Destructuring `style` makes a new
    /// [`RunStyle`] field a compile error until it is applied here.
    fn patch(self, style: RunStyle) -> Self {
        let RunStyle {
            bold,
            italic,
            underline,
            strikethrough,
            tracking,
            color,
            font_size,
        } = style;
        Self {
            face: FaceFlags {
                bold: bold.unwrap_or(self.face.bold),
                italic: italic.unwrap_or(self.face.italic),
            },
            lines: LineFlags {
                underline: underline.unwrap_or(self.lines.underline),
                strikethrough: strikethrough.unwrap_or(self.lines.strikethrough),
            },
            tracking: tracking.unwrap_or(self.tracking),
            color: color.unwrap_or(self.color),
            font_size: font_size.unwrap_or(self.font_size),
        }
    }

    /// Size and color only, for table cell text and list markers.
    fn plain(self) -> Self {
        Self {
            color: self.color,
            font_size: self.font_size,
            ..Self::seed()
        }
    }

    fn frame(self) -> Style {
        Style {
            font_size: self.font_size,
            color: self.color,
            ..default_style()
        }
    }
}

fn register_face(fonts: &mut FontStore, face: FontFace, errors: &mut ErrorBag) -> Option<FontId> {
    match fonts.register(FontSource {
        bytes: face.bytes,
        index: face.index,
    }) {
        Ok(id) => Some(id),
        Err(error) => {
            errors.record(error);
            None
        }
    }
}

fn family_of<'a, K: FontKey>(
    fonts: &'a FontTable<K>,
    key: K,
    errors: &mut ErrorBag,
) -> Option<&'a ResolvedFamily> {
    let family = fonts.by_key.get(&key);
    if family.is_none() {
        errors.record(Error::from(ErrorKind::UnregisteredFont {
            key: format!("{key:?}"),
        }));
    }
    family
}

/// The only producer of [`GlyphStyle`]. A missing face is recorded and the
/// regular face stands in, so later missing glyphs are still collected.
fn resolve_run(
    family: &ResolvedFamily,
    paint: InheritedPaint,
    errors: &mut ErrorBag,
) -> GlyphStyle {
    let variant = match (paint.face.bold, paint.face.italic) {
        (false, false) => None,
        (true, false) => Some((family.bold, ErrorKind::BoldFaceRequired)),
        (false, true) => Some((family.italic, ErrorKind::ItalicFaceRequired)),
        (true, true) => Some((family.bold_italic, ErrorKind::BoldItalicFaceRequired)),
    };
    let font = match variant {
        None => family.regular,
        Some((Some(face), _)) => face,
        Some((None, missing)) => {
            errors.record(Error::from(missing));
            family.regular
        }
    };
    let decoration = |wanted: bool, metrics: Option<EmOffset>, kind, errors: &mut ErrorBag| {
        if !wanted {
            return None;
        }
        let scaled = metrics.and_then(|offset| Decoration::at(offset, paint.font_size));
        if scaled.is_none() {
            errors.record(Error::from(ErrorKind::MissingDecorationMetrics {
                font: family.regular.as_u32(),
                decoration: kind,
            }));
        }
        scaled
    };
    GlyphStyle {
        font,
        font_size: paint.font_size,
        color: paint.color,
        tracking: paint.tracking,
        underline: decoration(
            paint.lines.underline,
            family.decorations.underline,
            DecorationKind::Underline,
            errors,
        ),
        strikethrough: decoration(
            paint.lines.strikethrough,
            family.decorations.strikethrough,
            DecorationKind::Strikethrough,
            errors,
        ),
    }
}

fn is_face_choice(kind: &ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::UnregisteredFont { .. }
            | ErrorKind::BoldFaceRequired
            | ErrorKind::ItalicFaceRequired
            | ErrorKind::BoldItalicFaceRequired
            | ErrorKind::MissingDecorationMetrics { .. }
    )
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
        let Some(regular) = register_face(&mut fonts, family.regular, errors) else {
            continue;
        };
        let decorations = match fonts.decorations(regular) {
            Ok(decorations) => decorations,
            Err(error) => {
                errors.record(error);
                continue;
            }
        };
        let mut optional =
            |face: Option<FontFace>| face.and_then(|face| register_face(&mut fonts, face, errors));
        let bold = optional(family.bold);
        let italic = optional(family.italic);
        let bold_italic = optional(family.bold_italic);
        by_key.insert(
            key,
            ResolvedFamily {
                regular,
                bold,
                italic,
                bold_italic,
                decorations,
            },
        );
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
    let seed = InheritedPaint::seed();
    let nodes = lower_blocks(body, &seed, fonts, doc, images, &mut errors);
    let header = header.and_then(|spec| own_band(spec, fonts, &mut errors));
    let footer = footer.and_then(|spec| own_band(spec, fonts, &mut errors));
    let behind = own_marks(behind, fonts, &mut errors);
    let front = own_marks(front, fonts, &mut errors);
    let decorations = Decorations {
        header: header.map(band_content),
        footer: footer.map(band_content),
        behind: layer_content(behind, doc, images.clone()),
        front: layer_content(front, doc, images.clone()),
    };
    let config = PaginateConfig {
        page_size: page_size.unwrap_or(default_page).resolve(),
        margins,
    };
    (config, FlowNode::Stack(nodes), decorations, errors)
}

fn lower_blocks<K: FontKey>(
    blocks: Vec<Block<K>>,
    paint: &InheritedPaint,
    fonts: &FontTable<K>,
    doc: DocToken,
    images: &LoweredImages,
    errors: &mut ErrorBag,
) -> Vec<FlowNode> {
    let mut nodes = Vec::new();
    for block in blocks {
        match block {
            Block::Paragraph(spec) => {
                if let Some(node) = lower_paragraph(spec, paint, fonts, errors) {
                    nodes.push(node);
                }
            }
            Block::Spacer(height) => nodes.push(FlowNode::Spacer(SpacerNode { height })),
            Block::PageBreak => nodes.push(FlowNode::PageBreak),
            Block::KeepTogether(children) => {
                let child = lower_blocks(children, paint, fonts, doc, images, errors);
                nodes.push(FlowNode::KeepTogether(KeepTogetherNode::Open(child)));
            }
            Block::Styled {
                style: patch,
                children,
            } => {
                let inner = paint.patch(patch);
                nodes.extend(lower_blocks(children, &inner, fonts, doc, images, errors));
            }
            Block::Columns(spec) => {
                if let Some(node) = lower_columns(spec, paint, fonts, doc, images, errors) {
                    nodes.push(node);
                }
            }
            Block::Table(spec) => {
                if let Some(node) = lower_table(spec, paint, fonts, doc, images, errors) {
                    nodes.push(node);
                }
            }
            Block::List(spec) => {
                if let Some(node) = lower_list(spec, paint, fonts, doc, images, errors) {
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

fn lower_paragraph<K: FontKey>(
    spec: ParagraphSpec<K>,
    parent: &InheritedPaint,
    fonts: &FontTable<K>,
    errors: &mut ErrorBag,
) -> Option<FlowNode> {
    let paint = parent.patch(RunStyle {
        color: spec.color,
        font_size: spec.font_size,
        ..RunStyle::default()
    });
    let mut bag = ErrorBag::default();
    let mut items = Vec::new();
    let mut active_link = None;
    push_inline_runs(
        spec.runs,
        &paint,
        fonts,
        &mut items,
        &mut active_link,
        &mut bag,
    );
    let structural = bag
        .as_slice()
        .iter()
        .any(|error| !is_face_choice(&error.kind));
    let font_only_empty = items.is_empty()
        && bag
            .as_slice()
            .iter()
            .any(|error| is_face_choice(&error.kind));
    errors.absorb(bag);
    if structural || font_only_empty {
        return None;
    }
    let mut style = paint.frame();
    style.font = first_run_font(&items);
    style.first_line_indent = spec.first_line_indent;
    style.align = text_align(spec.align);
    if let Some(factor) = spec.line_height_factor {
        style.line_height_factor = factor;
    }
    Some(FlowNode::Text(TextNode::from_items(items, style)))
}

fn first_run_font(items: &[InlineItem]) -> Option<FontId> {
    items.iter().find_map(|item| match item {
        InlineItem::Text(run) => Some(run.style.font),
        InlineItem::Blank(_) => None,
    })
}

fn push_inline_runs<K: FontKey>(
    runs: Vec<InlineRun<K>>,
    paint: &InheritedPaint,
    fonts: &FontTable<K>,
    items: &mut Vec<InlineItem>,
    active_link: &mut Option<LinkTarget>,
    errors: &mut ErrorBag,
) {
    for run in runs {
        match run {
            InlineRun::Text { text, font, style } => {
                if text.is_empty() {
                    continue;
                }
                if let Some(family) = family_of(fonts, font, errors) {
                    let glyph = resolve_run(family, paint.patch(style), errors);
                    append_text(items, glyph, &text, active_link.as_ref(), errors);
                }
            }
            InlineRun::Link { uri, runs } => match LinkTarget::uri(&uri) {
                Ok(target) => {
                    let previous = active_link.replace(target);
                    push_inline_runs(runs, paint, fonts, items, active_link, errors);
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
    style: GlyphStyle,
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
        if last.style == style && last.link.as_ref() == active_link {
            last.text.push_str(text);
            return;
        }
    }
    items.push(InlineItem::Text(
        LayoutTextRun::new(text, style).with_link(active_link.cloned()),
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
                let family = family_of(fonts, font, errors)?;
                let style = resolve_run(family, InheritedPaint::seed().patch(style), errors);
                marks.push(CanvasMarkNode::Text { text, at, style });
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

fn lower_columns<K: FontKey>(
    spec: ColumnsSpec<K>,
    paint: &InheritedPaint,
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
        let nodes = lower_blocks(column.blocks, paint, fonts, doc, images, &mut bag);
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
    paint: &InheritedPaint,
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
    let header = lower_table_rows(spec.header, paint, fonts, doc, images, &mut bag);
    let body = lower_table_rows(spec.body, paint, fonts, doc, images, &mut bag);
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
    paint: &InheritedPaint,
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
            match lower_cell(cell, paint, fonts, doc, images, errors) {
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
    paint: &InheritedPaint,
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
                if let Some(family) = family_of(fonts, font, &mut bag) {
                    let glyph = resolve_run(family, paint.plain(), &mut bag);
                    let mut cell_style = paint.frame();
                    cell_style.font = Some(glyph.font);
                    content.push(FlowNode::Text(TextNode::new(text, glyph, cell_style)));
                }
            }
            CellBlock::Paragraph(paragraph) => {
                if let Some(node) = lower_paragraph(paragraph, paint, fonts, &mut bag) {
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
                if let Some(node) = lower_list(list, paint, fonts, doc, images, &mut bag) {
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
    paint: &InheritedPaint,
    fonts: &FontTable<K>,
    doc: DocToken,
    images: &LoweredImages,
    errors: &mut ErrorBag,
) -> Option<FlowNode> {
    let mut bag = ErrorBag::default();
    let mut items = Vec::new();
    for item in spec.items {
        let mark = bag.mark();
        let nodes = lower_blocks(item.blocks, paint, fonts, doc, images, &mut bag);
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
    let marker_style = resolve_run(&ResolvedFamily::first_face(), paint.plain(), &mut bag);
    let failed = !bag.is_empty();
    errors.absorb(bag);
    if failed {
        return None;
    }
    let layout_spec = LayoutListSpec {
        kind,
        marker_width: spec.marker_width,
        nest_indent: spec.nest_indent,
        marker_style,
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
    style: GlyphStyle,
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
    let (runs, uri, key, style) = match slot {
        BandSlot::Text { runs, font, style } => (runs, None, font, style),
        BandSlot::Link {
            uri,
            text,
            font,
            style,
        } => (text, Some(uri), font, style),
    };
    let family = family_of(fonts, key, errors)?;
    let style = resolve_run(family, InheritedPaint::seed().patch(style), errors);
    Some(OwnedSlot { runs, uri, style })
}

fn band_content(spec: OwnedBand) -> crate::layout::decorate::BandContent<'static> {
    Box::new(move |info, errors| Ok(lower_band(&spec, info, errors)))
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

fn lower_band(spec: &OwnedBand, info: &PageInfo, errors: &mut ErrorBag) -> Vec<FlowNode> {
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
        let style = Style {
            font: Some(slot.style.font),
            font_size: slot.style.font_size,
            color: slot.style.color,
            align,
            ..default_style()
        };
        let node = if slot.text.is_empty() {
            TextNode::from_items(Vec::new(), style)
        } else {
            TextNode::from_runs(
                vec![LayoutTextRun::new(slot.text, slot.style).with_link(slot.link)],
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
    style: GlyphStyle,
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
        style: slot.style,
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
        at: Point,
        style: GlyphStyle,
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
                let Some(family) = family_of(fonts, font, errors) else {
                    continue;
                };
                let style = resolve_run(family, InheritedPaint::seed().patch(style), errors);
                owned.push(OwnedMark::Text {
                    when,
                    text,
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
    doc: DocToken,
    images: LoweredImages,
) -> Option<LayerContent<'static>> {
    if marks.is_empty() {
        return None;
    }
    Some(Box::new(move |info, errors| {
        Ok(lower_layer_marks(&marks, info, doc, &images, errors))
    }))
}

fn lower_layer_marks(
    marks: &[OwnedMark],
    info: &PageInfo,
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
                text, at, style, ..
            } => {
                items.push(LayerItem::Text(LayerText {
                    text: render_text(text, &page),
                    at: *at,
                    style: *style,
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::error::{DecorationKind, ErrorBag, ErrorKind};
    use crate::fonts::{fixture_bytes, EmOffset, FontFace, FontStore};
    use crate::geom::Positive;
    use crate::layout::style::Decoration;
    use crate::spec::RunStyle;

    use super::{register_face, resolve_run, InheritedPaint, ResolvedFamily};

    #[test]
    fn scaled_decoration_offset_is_the_centerline_below_the_baseline() {
        let decoration =
            Decoration::at(EmOffset::new(-0.125).unwrap(), Positive::pt(16.0).unwrap()).unwrap();
        assert_eq!(Decoration::THICKNESS.as_pt().to_bits(), 1.0f32.to_bits());
        assert_eq!(decoration.centerline().as_pt().to_bits(), 2.5f32.to_bits());
    }

    #[test]
    fn missing_decoration_metrics_fail_without_a_default_offset() {
        let mut errors = ErrorBag::default();
        let paint = InheritedPaint::seed().patch(RunStyle {
            underline: Some(true),
            strikethrough: Some(true),
            ..RunStyle::default()
        });
        let glyph = resolve_run(&ResolvedFamily::first_face(), paint, &mut errors);
        assert_eq!(glyph.underline, None);
        assert_eq!(glyph.strikethrough, None);
        assert_eq!(
            errors
                .as_slice()
                .iter()
                .map(|error| error.kind.clone())
                .collect::<Vec<_>>(),
            [
                ErrorKind::MissingDecorationMetrics {
                    font: 0,
                    decoration: DecorationKind::Underline,
                },
                ErrorKind::MissingDecorationMetrics {
                    font: 0,
                    decoration: DecorationKind::Strikethrough,
                },
            ]
        );
    }

    #[test]
    fn register_face_keeps_the_callers_arc() {
        let Some(bytes) = fixture_bytes() else {
            return;
        };
        let face = FontFace::from_shared(Arc::clone(&bytes), 0);
        let mut store = FontStore::new();
        let mut errors = ErrorBag::default();
        let id = register_face(&mut store, face, &mut errors).expect("face");
        assert!(Arc::ptr_eq(
            &bytes,
            &store.face(id).expect("loaded").source.bytes
        ));
    }
}
