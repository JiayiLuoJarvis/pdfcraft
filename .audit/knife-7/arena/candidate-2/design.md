# candidate-2：Typed `CellContent` / `ListBody` + paint-time decorate

刀 7 内部协议。基线 `feat/flow-paginate @ a6ff61e`。不写 `src/`，不增加公共 DSL。签名实现时函数体保持 `todo!()`。权威是 `docs/提案书.md`；设计稿里与提案书冲突的「栏不可拆 / 表不合并 / 装饰按正文宽」不进入本形状。

调用方是 `src/layout/` 里的 `#[cfg(test)]`。夹具字体用已有的 `fonts::fixture_bytes`（ASCII、数字、U+2022、`第` / `页` / `共`）。坐标断言用 `to_bits()`，与现有 `paginate` 测试一致。不断言 krilla 内容流。

`plan_pages` 的参数表保持今天的三个参数。装饰只出现在 `paint_pages`。

---

## Usage (caller's view)

测试与这些项同 crate，直接调用，不经过 `document`。

```rust
use crate::canvas::{CanvasCommand, RecordingCanvas};
use crate::error::Error;
use crate::fonts::{fixture_bytes, FontSource, FontStore};
use crate::geom::{Color, Margins, NonNegative, Positive, Size};
use crate::layout::column_width::{resolve_column_widths, ColumnWidth};
use crate::layout::columns::ColumnsNode;
use crate::layout::decorate::{BandLine, DecorationInline, DecorationSpec};
use crate::layout::flow::{FlowNode, LayoutCtx};
use crate::layout::limited::{admit_cell, CellContent, ItemMarker, ListBody, ListKind};
use crate::layout::paginate::{paginate_root, PaginateConfig};
use crate::layout::paragraph::ParagraphNode;
use crate::layout::style::{Style, TextAlign};
use crate::layout::table::{CellSpec, RowSpec, TableNode};
use crate::text::measure::line_metrics;
```

已有 `paginate_root` 调用点补上 `&DecorationSpec::none()`。`none` 不画带、不改正文原点。

测试辅助只读 `DrawGlyphRun`：`texts` 返回源字符串，`glyph_run(page, text)` 按整段文字找那一条 run。坐标在断言处用 `to_bits()`。

### 1. 并排栏跨页，列 x 在续页保持

两栏等分，`gap = 12`。正文框高只够一行，每栏两行。续页两栏的 x 与首页相同，y 回到正文框顶。

```rust
#[test]
fn columns_partial_keeps_each_column_x_on_the_next_page() {
    let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
    let face = fonts.metrics(style.font).unwrap();
    let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
    let line_h = metrics.line_height;

    let margin_l = 20.0;
    let margin_t = 10.0;
    let body_w = 160.0;
    let body_h = line_h; // 一页只放得下每栏一行
    let page = Size::try_pt(body_w + 40.0, body_h + 20.0).unwrap();
    let config = PaginateConfig {
        page_size: page,
        margins: Margins::new(
            NonNegative::pt(margin_t).unwrap(),
            NonNegative::pt(20.0).unwrap(),
            NonNegative::pt(10.0).unwrap(),
            NonNegative::pt(margin_l).unwrap(),
        ),
    };

    let gap = 12.0;
    let inner = body_w - gap;
    let widths = resolve_column_widths(
        &[ColumnWidth::Ratio(1.0), ColumnWidth::Ratio(1.0)],
        inner,
    )
    .unwrap();
    let left = ParagraphNode::layout(&mut fonts, "AB", &style, widths[0]).unwrap();
    let right = ParagraphNode::layout(&mut fonts, "CD", &style, widths[1]).unwrap();
    // 每栏两行：用两段，每段一行。第二段在本页剩余高度 0 时进入 tail。
    let left_2 = ParagraphNode::layout(&mut fonts, "EF", &style, widths[0]).unwrap();
    let right_2 = ParagraphNode::layout(&mut fonts, "GH", &style, widths[1]).unwrap();
    let columns = ColumnsNode::assemble(
        widths,
        gap,
        vec![
            vec![FlowNode::Paragraph(left), FlowNode::Paragraph(left_2)],
            vec![FlowNode::Paragraph(right), FlowNode::Paragraph(right_2)],
        ],
    )
    .unwrap();

    let mut ctx = LayoutCtx { fonts: &mut fonts };
    let mut rec = RecordingCanvas::new();
    paginate_root(
        FlowNode::Columns(columns),
        &mut ctx,
        config,
        &DecorationSpec::none(),
        &mut rec,
    )
    .unwrap();

    let pages = rec.pages();
    assert_eq!(pages.len(), 2);
    let x0 = margin_l;
    let x1 = margin_l + widths[0] + gap;
    let y0 = margin_t + metrics.baseline;
    assert_eq!(texts(&pages[0]), vec!["AB", "CD"]);
    assert_eq!(texts(&pages[1]), vec!["EF", "GH"]);
    assert_eq!(glyph_run(&pages[0], "AB").origin.x().as_pt().to_bits(), x0.to_bits());
    assert_eq!(glyph_run(&pages[0], "CD").origin.x().as_pt().to_bits(), x1.to_bits());
    assert_eq!(glyph_run(&pages[1], "EF").origin.x().as_pt().to_bits(), x0.to_bits());
    assert_eq!(glyph_run(&pages[1], "GH").origin.x().as_pt().to_bits(), x1.to_bits());
    assert_eq!(glyph_run(&pages[0], "AB").origin.y().as_pt().to_bits(), y0.to_bits());
    assert_eq!(glyph_run(&pages[0], "CD").origin.y().as_pt().to_bits(), y0.to_bits());
    assert_eq!(glyph_run(&pages[1], "EF").origin.y().as_pt().to_bits(), y0.to_bits());
}
```

首页两条基线的 y 相等（顶对齐）。续页 x 与首页同一列相同。

### 2. 表：rowSpan 整块换页，续页重复全部表头

一列。表头两行 `H0`、`H1`。正文锚点格 `PAIR` 的 `row_span = 2`，下一物理行没有格子（被占用）。再下一行是 `TAIL`。`padding = 0`，`border = 1`。单行绘制高 = `line_height + border`。跨两行的组：首行没有「只占一行」的内容，内容高记 0，差额落到跨度最后一行，组高 = `line_height + 2 * border`。`body_h` 等于两行表头加这一组，放不下再加 `TAIL`。`repeat_header = true`。续页文字以 `H0`、`H1` 开头。

```rust
#[test]
fn row_span_block_moves_together_and_tail_repeats_every_header_row() {
    let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
    let face = fonts.metrics(style.font).unwrap();
    let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
    let pad = 0.0;
    let border = 1.0;
    let line = metrics.line_height;
    let body_h = 3.0 * line + 4.0; // 2 * (line + border) + (line + 2 * border)
    let body_w = 120.0;
    let margin_l = 16.0;
    let margin_t = 8.0;
    let page = Size::try_pt(body_w + 32.0, body_h + 16.0).unwrap();
    let config = page_config(page, margin_t, 16.0, 8.0, margin_l);

    let widths = resolve_column_widths(&[ColumnWidth::Ratio(1.0)], body_w).unwrap();
    let content_w = widths[0] - 2.0 * pad - border;
    let cell = |text: &str| -> CellSpec {
        let para = ParagraphNode::layout(&mut fonts, text, &style, content_w).unwrap();
        CellSpec {
            col_span: 1,
            row_span: 1,
            content: vec![CellContent::Paragraph(para)],
        }
    };
    let mut spanned = cell("PAIR");
    spanned.row_span = 2;
    let table = TableNode::assemble(
        widths,
        vec![RowSpec { cells: vec![cell("H0")] }, RowSpec { cells: vec![cell("H1")] }],
        vec![
            RowSpec { cells: vec![spanned] },
            RowSpec { cells: vec![] },
            RowSpec { cells: vec![cell("TAIL")] },
        ],
        true,
        pad,
        border,
        Color::rgb(0, 0, 0),
        body_h,
        &mut fonts,
    )
    .unwrap();

    let mut ctx = LayoutCtx { fonts: &mut fonts };
    let mut rec = RecordingCanvas::new();
    paginate_root(FlowNode::Table(table), &mut ctx, config, &DecorationSpec::none(), &mut rec)
        .unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(texts(&pages[0]), vec!["H0", "H1", "PAIR"]);
    assert_eq!(texts(&pages[1]), vec!["H0", "H1", "TAIL"]);

    let content_x = margin_l + border / 2.0 + pad;
    for page in &pages {
        for label in texts(page) {
            assert_eq!(
                glyph_run(page, &label).origin.x().as_pt().to_bits(),
                content_x.to_bits()
            );
        }
    }
    assert!(pages[0].ops().iter().any(|op| matches!(op, CanvasCommand::StrokePath { .. })));
    assert!(matches!(pages[0].ops().last(), Some(CanvasCommand::StrokePath { .. })));
}
```

`repeat_header = false` 的对照：第二页文字是 `["R3"]`，没有 `H0` / `H1`。

表头 `row_span` 伸进行文：

```rust
#[test]
fn header_row_span_into_body_is_invalid_cell_content() {
    // header 最后一行 row_span = 2，body 首行留空槽。
    let err = TableNode::assemble(/* ... */).unwrap_err();
    assert!(matches!(err, Error::InvalidCellContent { kind: "header-body-span" }));
}
```

表头总高 + 第一块正文高于 `body_h`：`Error::UnsplittableTallerThanFrame`，`RecordingCanvas` 无 `StartPage`。

### 3. 列表：悬挂缩进，标记只在该项第一片

`marker_width = 18`，缩进默认等于它。正文用 `body_w - 18` 断行。同一项有两段，正文框只够一段。第一页画出 `•` 和 `AB`；第二页只有 `CD`，x 仍是 `margin_l + 18`。

```rust
#[test]
fn list_marker_only_on_first_fragment_and_body_keeps_indent() {
    let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
    let face = fonts.metrics(style.font).unwrap();
    let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
    let marker_width = 18.0;
    let margin_l = 20.0;
    let margin_t = 10.0;
    let body_w = 200.0;
    let body_h = metrics.line_height;
    let page = Size::try_pt(body_w + 40.0, body_h + 20.0).unwrap();
    let config = page_config(page, margin_t, 20.0, 10.0, margin_l);
    let text_w = body_w - marker_width;

    let list = limited::assemble_list(
        ListKind::Bullet,
        marker_width,
        None, // nest indent = marker_width
        body_w,
        vec![limited::ItemInput {
            marker: ItemMarker::Auto,
            body: vec![
                ListBody::Paragraph(ParagraphNode::layout(&mut fonts, "AB", &style, text_w).unwrap()),
                ListBody::Paragraph(ParagraphNode::layout(&mut fonts, "CD", &style, text_w).unwrap()),
            ],
        }],
        &style,
        &mut fonts,
    )
    .unwrap();

    let mut ctx = LayoutCtx { fonts: &mut fonts };
    let mut rec = RecordingCanvas::new();
    paginate_root(FlowNode::List(list), &mut ctx, config, &DecorationSpec::none(), &mut rec)
        .unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(texts(&pages[0]), vec!["•", "AB"]);
    assert_eq!(texts(&pages[1]), vec!["CD"]);
    assert_eq!(glyph_run(&pages[0], "•").origin.x().as_pt().to_bits(), margin_l.to_bits());
    assert_eq!(
        glyph_run(&pages[0], "AB").origin.x().as_pt().to_bits(),
        (margin_l + marker_width).to_bits()
    );
    assert_eq!(
        glyph_run(&pages[1], "CD").origin.x().as_pt().to_bits(),
        (margin_l + marker_width).to_bits()
    );
    let y0 = margin_t + metrics.baseline;
    assert_eq!(glyph_run(&pages[0], "•").origin.y().as_pt().to_bits(), y0.to_bits());
    assert_eq!(glyph_run(&pages[0], "AB").origin.y().as_pt().to_bits(), y0.to_bits());
    assert_eq!(glyph_run(&pages[1], "CD").origin.y().as_pt().to_bits(), y0.to_bits());
}
```

有序 + 自定义串，同一标记列：

```rust
#[test]
fn ordered_and_custom_markers_use_the_same_column() {
    // start = 1，三项：Auto、Custom("注")、Auto → 文字 "1."、"注"、"3."
    // 标记 advance 都 ≤ marker_width。正文 x 都是 margin_l + marker_width。
    // Custom 不另加句点。
}
```

标记 advance 大于 `marker_width`：`Error::UnbreakableWiderThanFrame { width, avail: marker_width }`，不裁剪。嵌套列表的内层段落宽度是 `body_w - indent - child_indent`，内层标记 x = `margin_l + indent`。

### 4. 装饰在 paint 里量，正文断点与原点不动

页眉槽横跨页宽，x 从 0 起。正文 x 仍是 `margin.left`。页脚在下边带。换一套页脚再画一遍，正文基线 y 的 `to_bits` 相同。

```rust
#[test]
fn decoration_uses_full_page_width_and_does_not_move_body() {
    let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
    let face = fonts.metrics(style.font).unwrap();
    let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
    let margin_l = 40.0;
    let margin_t = 36.0;
    let margin_b = 36.0;
    let body_w = 200.0;
    let body_h = metrics.line_height * 3.0;
    let page = Size::try_pt(body_w + margin_l + 24.0, body_h + margin_t + margin_b).unwrap();
    let config = page_config(page, margin_t, 24.0, margin_b, margin_l);
    let para = ParagraphNode::layout(&mut fonts, "AB", &style, body_w).unwrap();

    let spec = DecorationSpec {
        style: style.clone(),
        header: Some(vec![BandLine {
            left: vec![DecorationInline::Text("第".into()), DecorationInline::PageNumber],
            center: vec![],
            right: vec![DecorationInline::PageCount],
        }]),
        footer: Some(vec![BandLine {
            left: vec![],
            center: vec![DecorationInline::Text("共".into()), DecorationInline::PageCount],
            right: vec![],
        }]),
    };

    let mut ctx = LayoutCtx { fonts: &mut fonts };
    let mut with = RecordingCanvas::new();
    paginate_root(FlowNode::Paragraph(para.clone()), &mut ctx, config, &spec, &mut with).unwrap();
    let mut bare = RecordingCanvas::new();
    paginate_root(
        FlowNode::Paragraph(para),
        &mut ctx,
        config,
        &DecorationSpec::none(),
        &mut bare,
    )
    .unwrap();

    let page_w = page.width().as_pt();
    let page_h = page.height().as_pt();
    let drawn = texts(&with.pages()[0]);
    assert_eq!(drawn, vec!["第1", "1", "AB", "共1"]);
    // 左槽贴页左缘。正文 x 仍是 margin.left。
    assert_eq!(glyph_run(&with.pages()[0], "第1").origin.x().as_pt().to_bits(), 0.0f32.to_bits());
    // 右槽 PageCount 的右缘贴页右缘：origin_x + advance = page_w。
    let count_run = glyph_run(&with.pages()[0], "1");
    let advance = count_run.glyphs.iter().map(|g| g.advance_norm * style.font_size.as_pt()).sum::<f32>();
    assert_eq!((count_run.origin.x().as_pt() + advance).to_bits(), page_w.to_bits());
    // 正文仍在四边 margin 里。
    let body = glyph_run(&with.pages()[0], "AB");
    assert_eq!(body.origin.x().as_pt().to_bits(), margin_l.to_bits());
    assert_eq!(body.origin.y().as_pt().to_bits(), (margin_t + metrics.baseline).to_bits());
    // 页脚带顶 = page_h - margin.bottom，文字从带顶往下排。
    let footer = glyph_run(&with.pages()[0], "共1");
    assert_eq!(footer.origin.y().as_pt().to_bits(), (page_h - margin_b + metrics.baseline).to_bits());
    let footer_advance = footer.glyphs.iter().map(|g| g.advance_norm * style.font_size.as_pt()).sum::<f32>();
    assert_eq!(
        (footer.origin.x().as_pt() + footer_advance / 2.0).to_bits(),
        (page_w / 2.0).to_bits()
    );

    let bare_body = glyph_run(&bare.pages()[0], "AB");
    assert_eq!(bare_body.origin.x().as_pt().to_bits(), body.origin.x().as_pt().to_bits());
    assert_eq!(bare_body.origin.y().as_pt().to_bits(), body.origin.y().as_pt().to_bits());
}
```

空白正文页（`内容 + pageBreak + pageBreak + 内容`）两套页码都计入该页。三页时中间页 `placed` 为空，页眉文字仍是 `第2`，右槽 `PageCount` 是 `3`。

溢出在任何 `start_page` 之前返回：

```rust
#[test]
fn decoration_taller_than_margin_writes_nothing() {
    // margins 全 0，header 有一个 Text。body 是一段能放下的段落。
    let err = paginate_root(/* ... */, &spec, &mut rec).unwrap_err();
    assert!(matches!(
        err,
        Error::DecorationTallerThanMargin { height, margin }
            if margin.to_bits() == 0.0f32.to_bits() && height > 0.0
    ));
    assert!(rec.commands().is_empty());
}

#[test]
fn decoration_wider_than_page_writes_nothing() {
    // 单槽字符串按页宽断行后多于一行。avail 是页宽，不是正文宽。
    let err = paginate_root(/* ... */).unwrap_err();
    assert!(matches!(
        err,
        Error::DecorationWiderThanFrame { avail, .. }
            if avail.to_bits() == page.width().as_pt().to_bits()
    ));
    assert!(rec.commands().is_empty());
}
```

多行页眉：两行 `BandLine` 的行盒之和大于 `margin.top` 时同样是 `DecorationTallerThanMargin`，且日志为空。两行之和小于带高时，第二行基线 y = `line_height + baseline`（从带顶往下）。

### 5. 单元格 / 列表项接不住栏、表、换页、keep

```rust
#[test]
fn admit_cell_rejects_flow_nodes_the_enum_cannot_store() {
    for (node, kind) in [
        (FlowNode::Columns(sample_columns()), "columns"),
        (FlowNode::Table(sample_table()), "table"),
        (FlowNode::PageBreak, "page-break"),
        (FlowNode::KeepTogether(sample_keep()), "keep-together"),
        (FlowNode::Stack(vec![]), "stack"),
    ] {
        let err = admit_cell(node).unwrap_err();
        assert!(matches!(err, Error::InvalidCellContent { kind: k } if k == kind));
    }
    let ok = admit_cell(FlowNode::Paragraph(sample_para())).unwrap();
    assert!(matches!(ok, CellContent::Paragraph(_)));
}

#[test]
fn admit_list_body_rejects_the_same_set() {
    let err = admit_list_body(FlowNode::Columns(sample_columns())).unwrap_err();
    assert!(matches!(err, Error::InvalidCellContent { kind: "columns" }));
    let nested = admit_list_body(FlowNode::List(sample_list())).unwrap();
    assert!(matches!(nested, ListBody::List(_)));
}
```

列宽每个 reason 一个测试，名字 `column_widths_<reason>`，reason 字符串见下方形状。`two_autos` 是其中之一：

```rust
#[test]
fn column_widths_more_than_one_auto() {
    let err = resolve_column_widths(&[ColumnWidth::Auto, ColumnWidth::Auto], 100.0).unwrap_err();
    assert!(matches!(err, Error::ColumnWidths { reason: "more-than-one-auto" }));
}
```

---

## Shape

### 协议：`Split<T>` 与二维平移

叶子的 tail 保持叶子类型。分页器只看见 `Split<FlowNode>`（默认类型参数，现有 `Split::Fit` / `Partial` 写法保留）。

```rust
pub(crate) enum Split<T = FlowNode> {
    Fit { height: f32, fragment: Fragment },
    Partial { head_height: f32, head: Fragment, tail: T },
    MoveToNextPage(T),
}

pub(crate) enum FragmentCommand {
    Glyphs(GlyphRun),
    Stroke { path: Path, paint: Paint, stroke: Stroke },
}

impl Fragment {
    pub(crate) fn with_offset(self, dx: f32, dy: f32) -> Result<Self> { todo!() }
}
```

`with_offset` 平移 `GlyphRun::origin`，并按 `path.verbs()` 重建路径：`MoveTo` / `LineTo` 的点加 `(dx, dy)`，`Close` 原样。`Stack` 调用 `with_offset(0.0, y)`。`draw_fragment` 对 `Stroke` 再加页面原点后调用 `Canvas::stroke_path`。路径平移写在 `flow.rs` 私有函数里，`geom::Path` 不新增方法。

`ParagraphNode::split`、`SpacerNode::split`、`RuleNode::split` 返回 `Split<Self>`。`FlowNode::split` 把它们抬成 `Split<FlowNode>`。没有 `Fill` 变体。

```rust
pub(crate) enum FlowNode {
    Paragraph(ParagraphNode),
    Spacer(SpacerNode),
    Rule(RuleNode),
    PageBreak,
    KeepTogether(KeepTogetherNode),
    Stack(Vec<FlowNode>),
    Columns(ColumnsNode),
    Table(TableNode),
    List(ListNode),
}
```

`RuleNode` 与 `SpacerNode` 一样放在 `flow.rs`。字段：`width`、`thickness: Length<Positive>`、`space_before` / `space_after: Length<NonNegative>`、`paint: Paint`。高度 = `space_before + thickness + space_after`。整条不可拆：放得下则 `Fit`，剩余高不够但整页够则 `MoveToNextPage`，高于 `full_height` 则 `UnsplittableTallerThanFrame`。水平线在 `y = space_before + thickness / 2`，从 `x = 0` 到 `x = width`。`width` 在组装时定死，`split` 不读 `avail.width`。

### 受限内容：一个宏，两个枚举

`PageBreak`、`Columns`、`Table`、`KeepTogether`、`Stack` 没有对应变体。`Image` 本刀不加。

```rust
macro_rules! limited_variants {
    ($enum_name:ident) => {
        pub(crate) enum $enum_name {
            Paragraph(ParagraphNode),
            Spacer(SpacerNode),
            Rule(RuleNode),
            List(ListNode),
        }
    };
}
limited_variants!(CellContent);
limited_variants!(ListBody);
```

同一宏再生成私有 `LimitedKind` 以及 `CellContent` / `ListBody` 与 `LimitedKind` 的私有转换。`split_seq` 只匹配 `LimitedKind`。新增变体只改这一处宏。两个枚举之间不实现 `From`。

```rust
pub(crate) fn admit_cell(node: FlowNode) -> Result<CellContent> { todo!() }
pub(crate) fn admit_list_body(node: FlowNode) -> Result<ListBody> { todo!() }

pub(crate) fn measure_contents(
    items: Vec<CellContent>,
    ctx: &mut LayoutCtx<'_>,
    width: f32,
    full_height: f32,
) -> Result<(f32, Fragment)> { todo!() }
```

`admit_cell` 与 `admit_list_body` 共用私有 `admit_kind`，这是唯一把 `FlowNode` 放进受限枚举的匹配。允许 `Paragraph` / `Spacer` / `Rule` / `List`。其余 `InvalidCellContent`，`kind` 为 `"columns"`、`"table"`、`"page-break"`、`"keep-together"`、`"stack"`。

`measure_contents` 在整框 `{ width, full_height, full_height }` 上跑 `split_seq`，只接受 `Fit`。`Partial` 或 `MoveToNextPage` 变成 `UnsplittableTallerThanFrame`。它不把内容包成 `FlowNode::Stack`。段后距规则与 `split_stack` 相同：放不下就丢，最后一个孩子的 `spacing_after` 向上冒。

`split_seq` 的进度规则与 `Stack` 相同，tail 的类型是 `Vec<CellContent>` 或 `Vec<ListBody>`，不会变回 `FlowNode`。

### 列宽（栏与表共用一个函数）

```rust
pub(crate) enum ColumnWidth {
    Fixed(f32),
    Ratio(f32),
    Auto,
}

pub(crate) fn resolve_column_widths(columns: &[ColumnWidth], inner: f32) -> Result<Vec<f32>> {
    todo!()
}
```

`inner` 是已经扣过 `(n - 1) * gap` 的宽度。规则：

| 条件 | `ColumnWidths.reason` |
| --- | --- |
| 非有限 Fixed / Ratio / inner | `non-finite` |
| Fixed ≤ 0 | `non-positive-fixed` |
| Ratio ≤ 0 | `non-positive-ratio` |
| Auto 多于一个 | `more-than-one-auto` |
| Fixed 之和 > inner | `fixed-sum-exceeds` |
| 有 Auto 且 inner − Fixed 之和 ≤ 0 | `no-remaining-width` |

无 Auto 时 Fixed 不拉伸，Ratio 按权重分剩余。Auto 只吃剩余，不按内容收缩。

`ColumnsNode::assemble` 另报：`n < 2` 或孩子数不等于宽数 → `column-count`；`gap` 非有限或 &lt; 0，或扣完 gap 后 `inner ≤ 0` → `gap`。默认间距常量 `DEFAULT_COLUMN_GAP = 12.0`，组装参数显式传入，测试传 `12.0`。

### Columns

```rust
pub(crate) struct ColumnsNode {
    gap: f32,
    columns: Vec<ColumnSlot>,
}

struct ColumnSlot {
    x: f32,
    width: f32,
    kids: Vec<FlowNode>,
}

impl ColumnsNode {
    pub(crate) fn assemble(widths: Vec<f32>, gap: f32, kids: Vec<Vec<FlowNode>>) -> Result<Self> {
        todo!()
    }
}
```

`x_0 = 0`，`x_{i+1} = x_i + width_i + gap`。段落在进 `assemble` 之前已用对应 `widths[i]` 调用过 `ParagraphNode::layout`。

`split` 对每列使用 `Frame { width: slot.width, height: avail.height, full_height: avail.full_height }`，顶对齐：

- 全部 `Fit`：`Fit`，高度取各列最大，`spacing_after` 取各列 `spacing_after` 的最大。命令按列 `with_offset(slot.x, 0.0)`。
- 全部 `MoveToNextPage`：`MoveToNextPage(self)`。
- 其余：`Partial`。`head_height` 是各列已放高度的最大（未放的列记 0）。head 的 `spacing_after` 为 0。tail 仍是同列数的 `ColumnsNode`：`Fit` 的列变成空 `kids`，`Partial` / `MoveToNextPage` 的列留下返回的节点。空列在续页占位，x 不重排。
- 整页框上 `head_height == 0` 且仍有列 `MoveToNextPage`：返回 `MoveToNextPage(tail)`，让根分页器报 `UnsplittableTallerThanFrame`，不返回零高 `Partial`。

列内 `keep_together` 的 `full_height` 仍是整页正文高。栏内可以再放 `Columns`、`Table`、`List`、`Stack`。

### Table

```rust
pub(crate) struct CellSpec {
    pub(crate) col_span: u32,
    pub(crate) row_span: u32,
    pub(crate) content: Vec<CellContent>,
}

pub(crate) struct RowSpec {
    pub(crate) cells: Vec<CellSpec>,
}

pub(crate) struct TableNode { /* 私有字段 */ }

impl TableNode {
    pub(crate) fn assemble(
        col_widths: Vec<f32>,
        header: Vec<RowSpec>,
        body: Vec<RowSpec>,
        repeat_header: bool,
        padding: f32,
        border_width: f32,
        border_color: Color,
        full_body_height: f32,
        ctx: &mut LayoutCtx<'_>,
    ) -> Result<Self> { todo!() }
}
```

`col_span` / `row_span` 小于 1、越界、重叠、覆盖格里又出现 `CellSpec`：`InvalidCellContent { kind: "span" }`。表头行的 `row_span` 伸进正文：`kind: "header-body-span"`。`padding < 0` 或非有限、`border_width < 0` 或非有限：`InvalidGeometry`。

格内容宽 = 所跨列宽之和 − `2 * padding` − `border_width`（跨列只扣外侧那一个 `border_width`，不扣被合并掉的内线）。`content_width ≤ 0`：`ColumnWidths { reason: "content-width" }`。段落在这个宽度上 `layout`，然后 `measure_contents`。

行高：单行格的内容高计入该行；`row_span` 格的内容高计入它覆盖的行高之和。每行先取「只占该行的格子」的最大内容高，再把跨行格超出的部分加到该跨度的最后一行。行的绘制高 = 内容高 + `2 * padding` + `border_width`。

跨页原子是 **rowSpan 组**：从某行起，直到没有任何 `row_span` 伸过该行为止，这些行合成一组。组内不切开。组高 &gt; `full_body_height`：`UnsplittableTallerThanFrame`。

`repeat_header == true` 时，`header` 总高 + 第一组正文高 &gt; `full_body_height`：同一个错误（含表头自身高于整页）。`split` 把表头和随后至少一组正文放在同一页；一组都放不下则整表 `MoveToNextPage`，不留下只有表头的 head。`Partial` 的 tail 带上同一份表头行（克隆已烘焙的行）。`repeat_header == false` 时 tail 的表头为空，首页仍画原始表头。上一页的 head 不追加下一页的表头。

边框：`border_width == 0` 不产生 `Stroke`。大于 0 时，本片段里每条可见格线一条 `Stroke`（线心在格线上）。合并区域内部的线不画。线宽是 `Stroke::new(Positive::pt(border_width)?)`。片段内命令顺序：单元格字形，然后格线。

格内容原点：`(col_x + border_width / 2 + padding, row_y + border_width / 2 + padding)`。

### List

```rust
pub(crate) enum ListKind {
    Bullet,
    Ordered { style: EnumStyle, start: u32 },
}

pub(crate) enum EnumStyle {
    Decimal,
    UpperAlpha,
    LowerAlpha,
    UpperRoman,
    LowerRoman,
}

pub(crate) enum ItemMarker {
    Auto,
    Custom(String),
}

pub(crate) struct ItemInput {
    pub(crate) marker: ItemMarker,
    pub(crate) body: Vec<ListBody>,
}

pub(crate) struct ListNode { /* marker_width, indent, items */ }

pub(crate) fn assemble_list(
    kind: ListKind,
    marker_width: f32,
    nest_indent: Option<f32>,
    outer_width: f32,
    items: Vec<ItemInput>,
    marker_style: &Style,
    ctx: &mut LayoutCtx<'_>,
) -> Result<ListNode> { todo!() }
```

`indent = nest_indent.unwrap_or(marker_width)`。`marker_width ≤ 0`、`indent ≤ 0`、或 `indent ≥ outer_width`：`InvalidGeometry`。正文段落必须已经按 `outer_width - indent` 断过行；`assemble_list` 不再断行，只核对调用方传入的段落宽度与这个值一致，不一致则 `InvalidGeometry`。

标记文字：

| 来源 | 字符串 |
| --- | --- |
| `Bullet` + `Auto` | `"•"` |
| `Ordered` + `Auto` | `format_marker(style, ordinal)`，见下 |
| `Custom(s)` | `s` 原样，不加句点。空串 `InvalidGeometry` |

`format_marker`：`Decimal` → `"1."`、`"2."`、`"10."`；字母 → `"A."` … `"Z."`，`"a."` … `"z."`；罗马 → `"I."` … `"i."`。序号从 `start` 起，默认测试传 `1`。`start == 0` 为 `InvalidGeometry`。字母超出 26、罗马超出 1..=3999：`InvalidGeometry`。`Custom` 仍占用序号：Auto、Custom、Auto 得到 `1.`、自定义串、`3.`。

标记用 `marker_style` 测量成 `GlyphRun`，advance 大于 `marker_width` 则 `UnbreakableWiderThanFrame { width: advance, avail: marker_width }`。烘焙进项，`split` 不再测量。

项的结构：`marker: Option<GlyphRun>`，`body: Vec<ListBody>`。`split` 逐项走 `split_seq`。该项在本片段第一次放下内容时，把标记画在 `x = 0`、基线与正文第一行相同，然后 tail 里该项的 `marker` 为 `None`。正文命令 `with_offset(indent, y)`。续页不画标记，缩进不变。整项 `MoveToNextPage` 时标记留在 tail 上。列表的 `spacing_after` 取最后一项 body 的段后距，规则同 Stack。

嵌套：子列表是 `ListBody::List`。父项把子列表放进已经右移 `indent` 的正文列。子列表自己的 `indent` 再右移一层。

### Decorate

`PagePlan` 保持 `size` 与 `placed`。不增加页眉、页脚、页码字段。

```rust
pub(crate) enum DecorationInline {
    Text(String),
    PageNumber,
    PageCount,
}

pub(crate) struct BandLine {
    pub(crate) left: Vec<DecorationInline>,
    pub(crate) center: Vec<DecorationInline>,
    pub(crate) right: Vec<DecorationInline>,
}

pub(crate) struct DecorationSpec {
    pub(crate) style: Style,
    pub(crate) header: Option<Vec<BandLine>>,
    pub(crate) footer: Option<Vec<BandLine>>,
}

impl DecorationSpec {
    pub(crate) fn none() -> Self { todo!() }
}

pub(crate) struct PageBands {
    header: Option<Fragment>,
    footer: Option<Fragment>,
}

pub(crate) fn measure_bands(
    page_sizes: &[Size],
    margins: Margins,
    spec: &DecorationSpec,
    ctx: &mut LayoutCtx<'_>,
) -> Result<Vec<PageBands>> { todo!() }
```

`measure_bands` 只接收页尺寸序列和 margin。它看不到 `placed`。页码是全文物理页：`PageNumber` = 下标 + 1，`PageCount` = `page_sizes.len()`，十进制、无前导零。空白正文页算在内。本刀没有 section 页码，也没有闭包。

同一槽的 inline 拼成一个字符串，用 `spec.style` 按页宽走现有断行。槽内出现换行字符：`InvalidGeometry`。断行结果不是恰好一行：`DecorationWiderThanFrame { width: 该槽未折行 advance, avail: page_width }`。空槽跳过。三槽各自和页宽比，不把三槽宽度相加，也不检测互相重叠。

行盒高 = `line_metrics.line_height`。页眉行盒之和 &gt; `margin.top`，或页脚行盒之和 &gt; `margin.bottom`：`DecorationTallerThanMargin { height, margin }`。`header: None` / `footer: None` 不占带。`Some(空 vec)` 与 `None` 相同。margin 为 0 且该带有任何非空槽：带高错误。

放置：左槽 `x = 0`；右槽右缘 = 页宽；中槽以页宽居中。页眉从 `y = 0` 往下排。页脚从 `y = page_h - margin.bottom` 往下排（带顶，靠近正文的一侧）。基线用 `metrics.baseline`。不改 `body_h`，不改已写入的正文原点。

```rust
pub(crate) fn paginate_root(
    root: FlowNode,
    ctx: &mut LayoutCtx<'_>,
    config: PaginateConfig,
    decoration: &DecorationSpec,
    canvas: &mut dyn Canvas,
) -> Result<()> { todo!() }

fn paint_pages(
    pages: &[PagePlan],
    margins: Margins,
    decoration: &DecorationSpec,
    ctx: &mut LayoutCtx<'_>,
    canvas: &mut dyn Canvas,
) -> Result<()> { todo!() }
```

`paint_pages` 内部两段，都在这个函数里：

1. `let sizes: Vec<_> = pages.iter().map(|p| p.size).collect()`，然后 `measure_bands`。出错立刻返回。此前不调用 `start_page`。
2. 对每一页：`start_page` → 画 header 片段（原点 `(0, 0)`）→ 按已有 `placed` 画正文 → 画 footer 片段（原点 `(0, page_h - margin.bottom)`）→ `finish_page`。

`PageBands` 是 `paint_pages` 的局部 `Vec`，函数返回后丢弃。绘制复用 `draw_fragment`。

### 错误

`Error` 增加四个变体（已是 `#[non_exhaustive]`）：

```rust
ColumnWidths { reason: &'static str },
InvalidCellContent { kind: &'static str },
DecorationTallerThanMargin { height: f32, margin: f32 },
DecorationWiderThanFrame { width: f32, avail: f32 },
```

`Display`：`column widths: {reason}`、`invalid cell content: {kind}`、`decoration height {height} exceeds margin {margin}`、`decoration width {width} exceeds frame {avail}`。

`InvalidCellContent.kind` 全集：`"columns"`、`"table"`、`"page-break"`、`"keep-together"`、`"stack"`、`"span"`、`"header-body-span"`。前五个只在 `admit_*`，后两个只在 `TableNode::assemble`。

`lib.rs` 不重导出新的 layout 类型。

### 接口深度

crate 公共表面只多四个 `Error` 变体。内部一条入口 `paginate_root` 完成分页和绘制。列宽公式、合并组、标记序号、带内溢出都在 `resolve_column_widths`、`TableNode::assemble`、`assemble_list`、`measure_bands` 后面。测试不自己拼格线，也不自己把页码写进正文节点。

`PagePlan` 的写入者仍只有 `plan_pages`。装饰测量读不到 `placed`，所以改页脚字符串不会碰到正文断点。受限枚举的变体表只有宏那一处；`FragmentCommand` 的匹配靠穷尽检查，漏改 `with_offset` 或 `draw_fragment` 会编译失败。

本形状故意不做：`Fill`、公共 `Block`、`Image` 变体、section 双页码、装饰闭包、`behind` / `front`、报纸式 snaking 分栏、把装饰片段存进 `PagePlan`、装饰装不下时撑开正文框。

---

## Module map

```text
src/error.rs                     四个 Error 变体与 Display
src/layout/mod.rs                增加 column_width / columns / limited
src/layout/flow.rs               Split<T>、Stroke、with_offset(dx, dy)、
                                 RuleNode、FlowNode 新变体、draw_fragment
src/layout/paragraph.rs          split 返回 Split<ParagraphNode>
src/layout/column_width.rs       ColumnWidth、resolve_column_widths
src/layout/columns.rs            ColumnsNode::assemble / split
src/layout/table.rs              网格、rowSpan 组、重复表头、格线
src/layout/limited.rs            宏、CellContent、ListBody、admit_*、
                                 split_seq、measure_contents、
                                 ListNode、标记、assemble_list
src/layout/decorate.rs           DecorationSpec、measure_bands
src/layout/paginate.rs           plan_pages 参数不变；paint_pages 先量后画
src/layout/lower.rs              保持占位。刀 8 的 DSL 只能调用 admit_*
src/layout/dump.rs               仍是占位。一旦匹配 FragmentCommand，编译器会要求 Stroke 臂
src/canvas.rs  src/document.rs  src/lib.rs   不改
```

依赖：

```text
paginate → decorate（只为 measure_bands）、flow、columns/table/limited（经由 FlowNode）
table → column_width、limited::measure_contents
columns → column_width、flow
limited → flow、paragraph、column 无关
decorate → paragraph 的测量路径（槽文本）、geom
column_width → error
flow 不依赖 table / columns 的组装；FlowNode 引用三个节点类型
```

`flow` 与 `paragraph` 今天已经互相引用。`flow` 与 `limited` 同样互相引用：`FlowNode::List` 持有 `ListNode`，`ListBody` 持有段落与 `RuleNode`。不要再加 `list.rs`。列表和单元格内容的变体表如果分到两个文件，`Image` 落地时会只改到其中一个。

`lower.rs` 不转发 `admit_*`。刀 8 直接调用 `limited::admit_cell` / `admit_list_body`。

---

## Problem

刀 7 要在现有的消费式 `FlowNode::split` 上加上可跨页的并排栏、按行（以及 rowSpan 组）切开并在 tail 上重复表头的表、悬挂缩进的列表，以及计划完成后才知道总页数的页眉页脚。正文框在 `plan_pages` 里已经扣过四边 margin，装饰不能回头改断点。单元格和列表项允许段落、分隔线、spacer、列表，禁止栏、表、pageBreak、keep_together。这些禁止如果只写成 `split` 里的运行时检查，节点类型仍然能把它们嵌进去。页眉页脚如果写进 `PagePlan`，计划阶段就会拥有一份只在绘制时有意义的数据。

## Usage (caller's view)

上文五组测试是契约：栏的续页 x、表的 rowSpan 与双行表头、列表标记与缩进、装饰的页宽与「先失败、零绘制」、`admit_*` 的 `kind`。类型从这些调用长出来。`DecorationSpec::none()` 让现有分页测试只多一个参数。

## Shape

承重决定：

- 受限内容是两个枚举，变体由一个宏生成。表和列表的 `split` 不接收 `FlowNode`。
- `Split<T>` 让段落 tail 仍是段落，列表 tail 仍是列表。抬升到 `FlowNode` 只发生在 `FlowNode::split`。
- 栏可拆，tail 保持列数与 x。表的原子单位是 rowSpan 组；重复表头克隆在 tail 上。
- `plan_pages` 不读取装饰。`paint_pages` 在第一轮 `measure_bands` 全部成功之后才 `start_page`。
- 边框只增加 `FragmentCommand::Stroke`。`with_offset` 同时平移 x 和 y。

不变量在类型里的部分：非法流节点进不了单元格和列表项。在组装时检查的部分：合并区域、标记宽度、列宽、装饰带高带宽。系统不在装饰装不下时改正文，也不在单元格里重新解释 `Stack`。

## Synthesis decision

占位。由 arena 综合填写：哪份候选作底、从另一份吸收了什么、拒绝了什么。本文件不记录该决定。

## Tradeoffs accepted

- 接受 `paint_pages` 里先整份量带、再逐页画，以换取出错时画布上没有任何 `StartPage`。量带结果只活在这个函数的局部 `Vec` 里。
- 接受页脚文字从带顶（靠近正文）往下排，和页眉同一条「从带的起始边往下」规则，以换取两带只有一种放置。带内多余空白留在页脚带的底边。
- 接受三槽可以在视觉上重叠。每槽单独和页宽比较，函数不插入避让。
- 接受 `Custom` 标记仍消耗序号（`1.`、自定义、`3.`），以换取序号在组装时一次定死，续页不会重数。
- 接受字母停在 Z、罗马停在 3999 就 `InvalidGeometry`，以换取本刀不发明多字母列或加线罗马。
- 接受相邻格线各画一笔、合并区域内部不画，以换取「一条可见格线一个 Stroke」。设计稿里的双边框叠画不采用。
- 接受 `split_seq` 与 `split_stack` 各写一遍段后距规则。受限序列如果改走 `FlowNode::Stack`，单元格里就会重新出现通用流节点。
- 接受 `paginate_root` 多一个 `&DecorationSpec` 参数，现有测试传入 `none`。不保留一个不带装饰的第二入口。
- 接受单元格内的列表整段必须 `Fit` 进该行所在的整页框。行是原子的，列表在格内不再另起一页。

## Alternatives considered

- **单元格和列表项直接存 `Vec<FlowNode>`，在 `lower` 里 `match` 报 `InvalidCellContent`。** 调用方要记住一份禁止清单，而且 `split` 的 tail 类型仍是 `FlowNode`，续页可以把栏重新塞回来。枚举把这份清单收进变体表；`admit_*` 只服务刀 8 的 DSL 边界。接口更小：表模块的字段类型本身拒绝栏。
- **把量好的页眉页脚写进 `PagePlan`，或让 `plan_pages` 返回 `(Vec<PagePlan>, Vec<PageBands>)`。** 计划类型会多一份绘制数据，之后改页脚的人会打开分页器。现在计划函数的参数表里没有 `DecorationSpec`，绘制函数自己量带。代价是画布写入前多一轮测量，这轮测量不读正文片段。
- **装饰按设计稿三槽，可用宽度取正文宽，原点取 `body.x`。** 提案书 §2.10 规定横向是整页宽。三槽保留，`avail` 与左右缘改到页宽；带高仍是上下 margin。
- **栏整组 `max(natural_height)`，放不下就整组换页（设计稿）。** 提案书 §2.5 要求各列在本页剩余高上切开。tail 用空列占位，避免续页重算 x。
- **重复表头只重复第 0 行，或在上一页 head 末尾再画一次表头。** 提案书要求续页重复全部表头行，并且表头属于 tail 的下一次 `split`。head 不追加。
- **列表项用 `FlowNode::Stack`，标记用负 x 画进正文左边。** 负 x 让「续行不得伸进标记列」变成绘制时的约定，段落仍按整栏宽度断行。现在正文宽度在 `layout` 时就是 `W - indent`，标记是项上的 `Option<GlyphRun>`。
- **给 `FragmentCommand` 加 `Fill`，以便日后单元格底色。** 本刀边框只需要 `Stroke`。底色没有提案书条款，第二变体只会让 `with_offset` 和 `draw_fragment` 多一条现在没人写的臂。
- **公共 `Block` 枚举或装饰闭包。** 刀 8 的事。本刀的页码是 `DecorationInline`。

## Open questions and risks

- 页脚是否应贴页底，而不是从带顶往下排？当前按两带同一规则实现；若要贴页底，只改 `measure_bands` 里页脚片段的内部 y，正文原点仍然不动。
- 有序标记的句点是否属于标记字符串？悬挂示意是 `1.`，本形状把句点算进 `marker_width`。自定义串不加句点。若句点应改成调用方自己的自定义串，`format_marker` 可以改成不加点，测试 `1.` 要一起改。
- `Custom` 占序号是否符合「该项不再用枚举算法」？现在的读法是该项不显示枚举结果，计数器仍然 +1。若希望下一项显示 `2.`，组装时跳过计数即可，tail 结构不用改。
- 字母超过 Z、罗马超过 3999 是报错还是扩展（`AA`、加线）？当前报 `InvalidGeometry`。
- `padding` 与 `border_width` 的 DSL 默认值（设计稿 4 与 0.5）提案书没有写死。本刀组装参数必填，不在引擎里放默认。刀 8 再决定默认。
- `split_seq` 与 `split_stack` 的段后距若以后改规则，两处都要改。编译器不会提醒。若实现时发现规则开始分叉，应停下来把「段后距怎么丢」收成一个共享的小函数，而不是再复制第三遍。

## Next implementation step

先改 `FragmentCommand::Stroke`、`with_offset(dx, dy)`、`Split<T>` 和四个 `Error` 变体，并让现有分页测试传入 `DecorationSpec::none()`；栏、表、列表和装饰的绘制都走这条命令路径。
