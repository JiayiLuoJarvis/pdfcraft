# Knife 6 — candidate-1：design-faithful flow / paginate

分支：`feat/flow-paginate` @ `b9de080`。范围：`Paragraph` / `Spacer` / `PageBreak` / `KeepTogether` / `Stack` + `paginate`。无公共 `Document` builder、无 `Table`/`Columns`/`List`/`Image`/`decorate`。

---

## 1. Usage sketch（crate-internal，测试优先）

测试不经过 `document` DSL，直接组 `FlowNode` + `LayoutCtx`，断言 `RecordingCanvas` 的 `DrawGlyphRun.origin` 与 `StartPage`/`FinishPage` 次数。

### 1.1 单页段落：对齐与基线 y

```rust
#[cfg(test)]
mod paragraph_single_page {
    use crate::canvas::{CanvasCommand, RecordingCanvas};
    use crate::fonts::{fixture_bytes, FontSource, FontStore};
    use crate::geom::{Positive, Size, pt};
    use crate::layout::flow::{
        FlowNode, Frame, LayoutCtx, ParagraphNode, ResolvedStyles, Stack, TextAlign,
    };
    use crate::layout::paginate::{paginate_root, PaginateConfig};
    use crate::layout::style::{Style, resolve_style};

    #[test]
    fn center_align_baseline_y_on_one_page() {
        let mut fonts = FontStore::new();
        let font = fonts
            .register(FontSource { bytes: fixture_bytes(), index: 0 })
            .unwrap();
        let default = Style::fixture_body(font);
        let mut ctx = LayoutCtx {
            fonts: &mut fonts,
            styles: &ResolvedStyles {
                default_style: default,
                named: Default::default(),
            },
            images: &Default::default(),
        };
        let style = resolve_style(&default, &ctx.styles.named, None, &Default::default()).unwrap();
        let root = FlowNode::Stack(vec![FlowNode::Paragraph(ParagraphNode {
            text: "AB".into(),
            style,
            resume: None,
        })]);
        let mut rec = RecordingCanvas::new();
        paginate_root(
            root,
            &mut ctx,
            &PaginateConfig::a4_zero_margins(),
            &mut rec,
        )
        .unwrap();
        // 断言：一行、Center 时 origin.x = body.x + (W - line_width)/2；
        // origin.y = body.y + line_metrics.baseline（首行 line_top = 0）。
        let runs: Vec<_> = rec
            .commands()
            .iter()
            .filter_map(|c| match c {
                CanvasCommand::DrawGlyphRun { run } => Some(run.origin),
                _ => None,
            })
            .collect();
        assert_eq!(runs.len(), 1);
        // …与预先 measure + break_lines 算出的 x/y 逐位比较 to_bits()
    }
}
```

### 1.2 跨页段落 + 段后距放不下则丢弃

```rust
#[cfg(test)]
mod paragraph_paginate_spacing {
    use crate::layout::flow::{FlowNode, ParagraphNode, SpacerNode, Stack};
    use crate::layout::paginate::{paginate_root, PaginateConfig};
    // …同上构造 ctx、RecordingCanvas

    #[test]
    fn splits_lines_and_drops_trailing_paragraph_spacing() {
        let tall_para = ParagraphNode {
            text: "行一\n行二\n行三".into(), // 或足够长的 ASCII 触发多行
            style,                          // paragraph_spacing = 6.0
            resume: None,
        };
        let root = FlowNode::Stack(vec![
            FlowNode::Paragraph(tall_para),
            FlowNode::Paragraph(ParagraphNode {
                text: "下一页首段".into(),
                style,
                resume: None,
            }),
        ]);
        // PaginateConfig：正文框高度刚好放两行 + 放不下 6pt 段后距
        paginate_root(root, &mut ctx, &config_two_line_body, &mut rec).unwrap();
        // 页 1 末段后无 spacing 绘制；页 2 首字形 y == body.y（不从 spacing 留白开始）
        assert_page_count(&rec, 2);
    }
}
```

### 1.3 PageBreak 四条轨迹 + Spacer 可拆

```rust
#[cfg(test)]
mod page_break_and_spacer {
    use crate::geom::Length;
    use crate::layout::flow::{FlowNode, PageBreak, SpacerNode, Stack};

    #[test]
    fn leading_page_break_is_no_op_one_blank_body_page_at_eof() {
        let root = FlowNode::Stack(vec![FlowNode::PageBreak]);
        paginate_root(root, &mut ctx, &PaginateConfig::a4_zero_margins(), &mut rec).unwrap();
        assert_page_count(&rec, 1); // Fresh + EOF → 一页空正文
    }

    #[test]
    fn content_then_two_breaks_then_content_is_three_pages() {
        let root = FlowNode::Stack(vec![
            FlowNode::Paragraph(/* … */),
            FlowNode::PageBreak,
            FlowNode::PageBreak,
            FlowNode::Paragraph(/* … */),
        ]);
        paginate_root(root, &mut ctx, &config, &mut rec).unwrap();
        assert_page_count(&rec, 3); // 内容 | 空白正文 | 内容
    }

    #[test]
    fn spacer_splits_across_pages() {
        let root = FlowNode::Stack(vec![
            FlowNode::Spacer(SpacerNode { height: Length::/* nn pt */ }),
        ]);
        // 正文框高度 < spacer.height → 两页；第二页开头仍是 Spacer 余量
    }
}
```

---

## 2. Type sketch + signatures

坐标：左上原点，y 向下。`Frame.size.width` = 换行宽 `W`（knife 仅正文框，无 columns/cell）。

### 2.1 样式（边界校验，layout 热路径信任）

```rust
// src/layout/style.rs
pub(crate) enum TextAlign { Start, Center, End }

pub(crate) struct Style { /* font, font_size: Length<Positive>, line_height_factor, color, paragraph_spacing: Length<NonNegative>, align */ }

pub(crate) struct StyleOverride { /* Option 字段 */ }

pub(crate) fn resolve_style(
    default_style: &Style,
    named: &BTreeMap<String, Style>,
    name: Option<&str>,
    override_style: &StyleOverride,
) -> Result<Style, Error>;
```

### 2.2 Flow 核心（与 `quire-design.md` 内部 flow 一致）

```rust
// src/layout/flow.rs

pub(crate) struct LayoutCtx<'a> {
    pub fonts: &'a mut FontStore,
    pub styles: &'a ResolvedStyles,
    pub images: &'a ImageTable, // knife：空表占位，无 krilla
}

pub(crate) struct ResolvedStyles {
    pub default_style: Style,
    pub named: BTreeMap<String, Style>,
}

pub(crate) struct Frame {
    pub size: Size, // width = W；height = 当前页剩余正文高度
}

pub(crate) struct Fragment {
    pub height: f32,           // 内容高，不含 spacing_after
    pub spacing_after: f32,
    pub commands: Vec<FragmentCommand>,
}

pub(crate) enum FragmentCommand {
    Glyphs(GlyphRun),
    Fill { .. },
    Stroke { .. },
    Image { .. },
    Link { .. },
}

pub(crate) enum Split {
    Fit { height: f32, fragment: Fragment },
    Split { head_height: f32, head: Fragment, tail: FlowNode },
    MoveToNextPage(FlowNode),
}

pub(crate) enum FlowNode {
    Paragraph(ParagraphNode),
    Spacer(SpacerNode),
    PageBreak,
    KeepTogether(Vec<FlowNode>),
    Stack(Vec<FlowNode>),
    // knife 不构造：Table / Image / Columns / List
}

impl FlowNode {
    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split, Error> {
        match self {
            FlowNode::PageBreak => Err(Error::InvalidGeometry),
            FlowNode::Paragraph(n) => n.split(ctx, avail),
            FlowNode::Spacer(n) => n.split(avail),
            FlowNode::KeepTogether(kids) => split_keep_together(ctx, avail, kids),
            FlowNode::Stack(kids) => split_stack(ctx, avail, kids),
        }
    }
}

pub(crate) fn draw_fragment(
    fragment: &Fragment,
    canvas: &mut dyn Canvas,
    origin: Point,
) -> Result<(), Error> {
    for cmd in &fragment.commands {
        match cmd {
            FragmentCommand::Glyphs(run) => {
                let mut placed = run.clone();
                placed.origin = Point {
                    x: origin.x + run.origin.x,
                    y: origin.y + run.origin.y,
                };
                canvas.draw_glyph_run(&placed)?;
            }
            // knife：其它 variant  unreachable 或 UnsupportedCanvasOperation
            _ => return Err(Error::UnsupportedCanvasOperation { operation: "draw_fragment" }),
        }
    }
    Ok(())
}

pub(crate) fn natural_height(
    node: FlowNode,
    ctx: &mut LayoutCtx<'_>,
    full: Frame,
) -> Result<(f32, Fragment), Error> {
    match node.split(ctx, full)? {
        Split::Fit { height, fragment } => Ok((height, fragment)),
        Split::Split { .. } | Split::MoveToNextPage(_) => {
            Err(Error::UnsplittableTallerThanFrame)
        }
    }
}
```

### 2.3 段落节点与测量管线

```rust
// src/layout/paragraph.rs

pub(crate) struct ParagraphNode {
    pub text: String,
    pub style: Style,
    /// 跨页续排：Some 表示从 BrokenLines 的该行索引继续，不再重复已画行。
    pub resume: Option<usize>,
}

impl ParagraphNode {
    pub(crate) fn split(
        self,
        ctx: &mut LayoutCtx<'_>,
        avail: Frame,
    ) -> Result<Split, Error> {
        todo!("见 §4 宽度/基线/对齐")
    }
}

/// 共享：ClusteredText → measure_text(glyph_text) → width_of → break_lines → line_metrics
pub(crate) struct ParagraphLayout {
    pub clustered: ClusteredText,
    pub measured: MeasuredText,
    pub broken: BrokenLines<'static>, // 实现时用 owned 包装或 ParagraphLayout 持有 String
    pub metrics: LineMetrics,
    pub line_height: f32,
}

pub(crate) fn layout_paragraph(
    ctx: &mut LayoutCtx<'_>,
    text: &str,
    style: &Style,
    width: f32,
) -> Result<ParagraphLayout, Error> {
    let clustered = ClusteredText::new(text);
    let measured = measure_text(ctx.fonts, style.font, clustered.glyph_text())?;
    let font_size = style.font_size.as_pt();
    let width_of = |cluster: Cluster| -> f32 {
        cluster_width(&measured, cluster, font_size) // 按 glyph_range 累加 advance_norm * font_size
    };
    let broken = break_lines(&clustered, width, width_of)?;
    let face = ctx.fonts.metrics(style.font)?;
    let metrics = line_metrics(face, style.font_size, style.line_height_factor)?;
    Ok(ParagraphLayout {
        line_height: metrics.line_height,
        // …
    })
}

pub(crate) fn align_line_x(body_left: f32, width_w: f32, line_width: f32, align: TextAlign) -> f32 {
    match align {
        TextAlign::Start => body_left,
        TextAlign::Center => body_left + (width_w - line_width) / 2.0,
        TextAlign::End => body_left + (width_w - line_width),
    }
}

pub(crate) fn build_glyph_run_for_line(
    layout: &ParagraphLayout,
    line: BrokenLine<'_>,
    style: &Style,
    origin: Point,
) -> Result<GlyphRun, Error> {
    todo!("MeasuredText 子集 + GlyphPlacement byte_range 映射到 line.glyph_byte_range")
}
```

**段落 `split` 逻辑（伪码，非 impl body）：**

```text
W = avail.size.width；校验有限且 > 0
layout = layout_paragraph(全文或 resume 起点的子串逻辑 — 实现上 resume 用 line index 切片 broken)
lines = broken[resume..]
line_h = layout.line_height
max_lines = floor(avail.size.height / line_h)  // 若 avail.height < line_h → max_lines = 0

若 max_lines >= lines.len():
  Fit：fragment = 所有行的 Glyphs，height = len * line_h，spacing_after = style.paragraph_spacing
若 max_lines == 0:
  MoveToNextPage(self)   // 当前页装不下一整行
否则:
  Split：head = lines[0..max_lines]；tail = Paragraph { resume: Some(start+max_lines), .. }
  head spacing_after = 0
```

单簇/不可断行宽于 `W` 时 `break_lines` 返回 `UnbreakableWiderThanFrame`（不在 paginate 吞掉）。

### 2.4 Spacer / Stack / KeepTogether

```rust
pub(crate) struct SpacerNode {
    pub height: Length<NonNegative>,
}

impl SpacerNode {
    pub(crate) fn split(self, avail: Frame) -> Result<Split, Error> {
        let h = self.height.as_pt();
        if h == 0.0 {
            return Ok(Split::Fit { height: 0.0, fragment: Fragment::empty() });
        }
        let take = h.min(avail.size.height);
        let rest = h - take;
        if rest <= 0.0 {
            Ok(Split::Fit { height: take, fragment: Fragment::empty() })
        } else {
            Ok(Split::Split {
                head_height: take,
                head: Fragment::empty(),
                tail: FlowNode::Spacer(SpacerNode { height: Length::from_trusted_pt(rest) }),
            })
        }
    }
}

fn split_stack(
    ctx: &mut LayoutCtx<'_>,
    mut avail: Frame,
    mut kids: Vec<FlowNode>,
) -> Result<Split, Error> {
    todo!("左到右；合并 head Fragment；spacing_after 规则；见 design Stack::split")
}

fn split_keep_together(
    ctx: &mut LayoutCtx<'_>,
    avail: Frame,
    kids: Vec<FlowNode>,
) -> Result<Split, Error> {
    let full = Frame { size: Size { width: avail.size.width, height: /* 调用方传入的整框高 */ } };
    // knife：KeepTogether::split 额外参数 full_body_height: f32，由 Stack/paginate 注入
    todo!()
}
```

**KeepTogether 自然高（与设计稿一致）：**

```rust
fn keep_together_natural_fragment(
    ctx: &mut LayoutCtx<'_>,
    kids: Vec<FlowNode>,
    full: Frame,
) -> Result<(f32, Fragment), Error> {
    let inner = FlowNode::Stack(kids);
    let (h, mut frag) = natural_height(inner, ctx, full)?;
    // 组内最后一个孩子的 spacing_after 从 frag 挪到组片段（若 design 要求）；knife 测试至少覆盖「组高含孩子间 spacing、不含组外最后 spacing」
    Ok((h, frag))
}
```

`KeepTogether::split`：`natural_height` 得 `Fit` 且 `h <= avail.height` → `Fit`；`h <= full.height` 但 `h > avail.height` → `MoveToNextPage(KeepTogether(kids))`；`natural_height` 失败 → `UnsplittableTallerThanFrame`。

### 2.5 Paginate + PageCursor

```rust
// src/layout/paginate.rs

pub(crate) struct PaginateConfig {
    pub page_size: Size,
    pub margins: Margins, // 默认 0；knife 不画 header/footer 带，仅定义 body rect
}

impl PaginateConfig {
    pub(crate) fn body_size(&self) -> Size { todo!() }
    pub(crate) fn body_origin(&self) -> Point { todo!() }
}

pub(crate) enum PageCursor {
    Fresh,
    HasBody,
    PendingBlank,
}

pub(crate) fn paginate_root(
    root: FlowNode,
    ctx: &mut LayoutCtx<'_>,
    config: &PaginateConfig,
    canvas: &mut dyn Canvas,
) -> Result<(), Error> {
    let stack = match root {
        FlowNode::Stack(k) => k,
        other => vec![other],
    };
    paginate_stack(stack, ctx, config, canvas)
}

fn paginate_stack(
    mut nodes: Vec<FlowNode>,
    ctx: &mut LayoutCtx<'_>,
    config: &PaginateConfig,
    canvas: &mut dyn Canvas,
) -> Result<(), Error> {
    let mut cursor = PageCursor::Fresh;
    let mut page_open = false;
    let mut y = 0.0f32;
    let body = config.body_rect();
    let full_h = body.size.height;

    while !nodes.is_empty() {
        let node = nodes.remove(0);
        match node {
            FlowNode::PageBreak => {
                cursor = apply_page_break(cursor, &mut page_open, canvas, config)?;
                y = 0.0;
                continue;
            }
            _ => {
                ensure_page(&mut cursor, &mut page_open, canvas, config, /* content incoming */)?;
                let avail = Frame {
                    size: Size { width: body.size.width, height: full_h - y },
                };
                match node.split(ctx, avail)? {
                    Split::Fit { height, fragment } => {
                        draw_fragment(&fragment, canvas, Point { x: body.origin.x, y: body.origin.y + y })?;
                        y += height;
                        y = consume_spacing_after(&mut avail, y, full_h, fragment.spacing_after);
                        cursor = PageCursor::HasBody;
                    }
                    Split::Split { head_height, head, tail } => {
                        draw_fragment(&head, canvas, Point { x: body.origin.x, y: body.origin.y + y })?;
                        finish_page(canvas)?;
                        page_open = false;
                        nodes.insert(0, tail);
                        y = 0.0;
                        cursor = PageCursor::HasBody;
                    }
                    Split::MoveToNextPage(tail) => {
                        if y == 0.0 && avail.size.height >= full_h - f32::EPSILON {
                            return Err(Error::UnsplittableTallerThanFrame);
                        }
                        if page_open {
                            finish_page(canvas)?;
                            page_open = false;
                        }
                        nodes.insert(0, tail);
                        y = 0.0;
                    }
                }
            }
        }
    }
    finalize_eof(cursor, page_open, canvas, config)
}

fn apply_page_break(
    cursor: PageCursor,
    page_open: &mut bool,
    canvas: &mut dyn Canvas,
    config: &PaginateConfig,
) -> Result<PageCursor, Error> { todo!() }

fn consume_spacing_after(
    avail: &Frame,
    y: f32,
    full_h: f32,
    spacing: f32,
) -> f32 {
    let remaining = full_h - y;
    if spacing > 0.0 && remaining >= spacing {
        y + spacing
    } else {
        y // 丢弃，不带到下一页
    }
}
```

**`ensure_page` / `finalize_eof`：** 负责 `Fresh`+EOF 开一页空正文、`PendingBlank`+EOF 丢弃挂起页、不在 `PendingBlank` 上误 `FinishPage` 等。

### 2.6 Error

```rust
// src/error.rs — 新增
UnsplittableTallerThanFrame,
```

Display：`"unsplittable block is taller than the page body frame"`。与 `UnbreakableWiderThanFrame` 区分：后者是水平断行失败；前者是整页高度仍 `Split`/`MoveToNextPage` 在满框上无解（KeepTogether、或 paginate 在 `y==0` 满框仍 `MoveToNextPage`）。

### 2.7 Lower（knife 最小）

```rust
// src/layout/lower.rs — 仅 #[cfg(test)] 或 pub(crate) for tests
pub(crate) fn lower_test_blocks(/* Block 子集 */) -> Result<FlowNode, Error> {
    Err(Error::InvalidGeometry) // PR9 前测试直接 FlowNode
}
```

---

## 3. Module map

| 文件 | 职责 |
| --- | --- |
| `src/layout/style.rs` | `Style` / `TextAlign` / `resolve_style`；构造期拒绝非法字号、负 `paragraph_spacing` |
| `src/layout/flow.rs` | `FlowNode` / `Split` / `Fragment` / `LayoutCtx` / `draw_fragment` / `natural_height`；`Stack`/`KeepTogether`/`PageBreak` 分支 |
| `src/layout/paragraph.rs` | `ParagraphNode::split`；`layout_paragraph`；`align_line_x`；`build_glyph_run_for_line`；行盒 y 累加 |
| `src/layout/paginate.rs` | `PageCursor` 状态机；`PaginateConfig`；`paginate_root`；页内 `y`；`spacing_after` 丢弃；`StartPage`/`FinishPage` |
| `src/layout/lower.rs` | 占位；knife 测试不依赖 |
| `src/layout/mod.rs` | `pub mod style;` re-export `pub(crate)` 给测试 |
| `src/text/{cluster,line_break,measure}.rs` | **只读复用**，不改算法 |
| `src/canvas.rs` | `GlyphRun` / `RecordingCanvas` |
| `src/error.rs` | `+ UnsplittableTallerThanFrame` |

依赖方向：`paginate → flow → paragraph → text/fonts`；禁止 `layout/*` `use krilla`.

---

## 4. Rationale

### 4.1 曾考虑但拒绝的方案

| 方案 | 拒绝原因 |
| --- | --- |
| **两阶段** `LineBox` 列表 + 扁平 `FlowItem`（arena candidate-2） | 与已写 internal flow 协议不一致；分页后再画会失去 `FragmentCommand` 与 `split` 消耗语义的单一路径 |
| **`Flowable` trait / trait object** | 提案书 / AGENTS：不向调用方暴露；内部 enum 方法更利于非法状态编译期排除 |
| **PageBreak 用 bool / 双标志** | 提案书 §2.9 与四条轨迹测试要求三态 `PageCursor` |
| **段落分页缓存整份 `BrokenLines` 在 paginate** | tail 用 `ParagraphNode.resume: usize` 足够；避免 Stack 与孩子之间的共享缓存生命周期 |
| **在 paginate 里重新测量而非 `FragmentCommand::Glyphs`** | 设计明确分页器绘制时不测量；断点一次在 `split` 决定 |
| **Justify / 首行缩进在本 knife 顺手做** | 提案书 §3 不做 Justify；首行缩进可后续 PR 在 `layout_paragraph` 加 `first_line_width` 而不改 paginate |
| **public `paginate` / Document builder** | frame 约束；仅 `pub(crate)` + 模块内测试 |
| **one-caller 包装**如 `split_paragraph_then_draw` | 保持 `split` + `draw_fragment` 两跳，与 design mermaid 一致 |

### 4.2 段落宽度 `W`、基线 y、对齐 x

| 量 | 计算 |
| --- | --- |
| **W** | `Frame.size.width` = 正文框宽 = `page_size.width - margin.left - margin.right`（knife 无 columns/cell） |
| **行顶** | 片段内第 `i` 行：`line_top = i * line_height`（`line_height = line_metrics(...).line_height`） |
| **基线 y** | `baseline_y = line_top + metrics.baseline`（与设计 PR8、`measure.rs` 中 `baseline == ascent` 一致） |
| **对齐 x** | 断行后每行 `line_width = BrokenLine::width()`（不含被丢行首 U+0020）；`Start`: `x = 0`（片段内相对 body 左）；`Center`: `(W - line_width)/2`；`End`: `W - line_width`。`draw_fragment` 时加 `body.origin` |
| **GlyphRun.origin** | `(align_x, baseline_y)` 相对片段原点；字形水平推进用已测量的 `advance_norm * font_size` |

**管线（强制顺序）：** `ClusteredText::new` → `measure_text(fonts, font, clustered.glyph_text())` → `break_lines(..., W, width_of)` → `line_metrics` → 按行 `build_glyph_run_for_line`。

### 4.3 PageCursor 转移表

| 当前状态 | 输入 | 动作 | 下一状态 |
| --- | --- | --- | --- |
| `Fresh` | 首个非 PageBreak 内容 | `StartPage`（若尚未开页），放置内容 | `HasBody` |
| `Fresh` | `PageBreak` | no-op | `Fresh` |
| `Fresh` | 流结束 | 输出 **1** 页空正文（`StartPage` + `FinishPage`） | （结束） |
| `HasBody` | `PageBreak` | `FinishPage` 当前页；**不**立即 `StartPage` | `PendingBlank` |
| `HasBody` | 流结束 | `FinishPage`；无额外空白页 | （结束） |
| `PendingBlank` | 非 PageBreak 内容 | `StartPage`（挂起页变实页），放置 | `HasBody` |
| `PendingBlank` | `PageBreak` | `StartPage` + `FinishPage` 空白正文；再挂起 | `PendingBlank` |
| `PendingBlank` | 流结束 | **丢弃**挂起页，不 `StartPage` | （结束） |

四条命名轨迹（必须与测试一一对应）：

1. 仅开头 `PageBreak` → 1 页空正文  
2. 开头连续两个 `PageBreak` → 仍 1 页（均 `Fresh` no-op）  
3. 内容 + 一个 `PageBreak` + EOF → 1 页有内容，挂起页丢弃  
4. 内容 + 两个 `PageBreak` + EOF → 第 1 页内容，第 2 页空白正文，挂起丢弃  

扩展：`内容 + PageBreak + 内容` → 2 页均有内容；`内容 + PageBreak + PageBreak + 内容` → 3 页（中页空白）。

### 4.4 测试清单（`RecordingCanvas`）

| 测试名（建议） | 断言要点 |
| --- | --- |
| `paragraph_align_start_center_end_x` | 三用例 `DrawGlyphRun.origin.x` 与 `to_bits` 预期 |
| `paragraph_baseline_y_first_line` | `origin.y == body.y + metrics.baseline` |
| `paragraph_wraps_unbreakable_wider_than_w` | `Err(UnbreakableWiderThanFrame { .. })` |
| `paragraph_splits_two_pages` | 页数 2；第二页文本续接；无重复行 |
| `paragraph_spacing_after_dropped_when_no_room` | 页 1 末无额外 y 间隙；页 2 首行 y == body.y |
| `spacer_zero_fit` | `Fit` 高度 0 |
| `spacer_splits_partial_height` | 两页；第二页仅余量高度 |
| `keep_together_move_to_next_page` | 组高 < 整框、> 剩余 → 下一页完整组 |
| `keep_together_taller_than_frame_errors` | `UnsplittableTallerThanFrame` |
| `page_break_leading_no_op` | 轨迹 1 |
| `page_break_leading_double_no_op` | 轨迹 2 |
| `page_break_trailing_single_discarded` | 轨迹 3 |
| `page_break_trailing_double_blank_page` | 轨迹 4 |
| `page_break_content_sandwich_two_pages` | 无中页空白 |
| `page_break_content_blank_content_three_pages` | 中页空白 |
| `flow_node_page_break_split_invalid` | `PageBreak.split()` → `InvalidGeometry` |
| `move_to_next_on_empty_full_frame_errors` | 满框 `y==0` 仍 `MoveToNextPage` → `UnsplittableTallerThanFrame` |
| `empty_stack_eof_one_page` | `Fresh` + EOF |

字体：优先 `fonts::fixture_bytes()`，与 `measure.rs` / `line_break.rs` 测试一致。

---

## 5. 实现顺序（建议 PR 切分）

1. `error` + `layout/style.rs` + `Fragment::empty()` 等小类型  
2. `paragraph.rs` 单页 `split` → `Fit`（不分页），`draw_fragment` + RecordingCanvas 对齐/基线测试（PR8 形）  
3. `flow.rs` `Stack` / `Spacer` / `KeepTogether` / `natural_height`  
4. `paginate.rs` + `PageCursor` + 段后距 + 四条 PageBreak  
5. 段落 `Split` + `resume` + 跨页测试  

Knife 结束标准：`cargo test`（无 feature）全绿；layout 内无 `krilla`；无 `document` 公共 builder。
