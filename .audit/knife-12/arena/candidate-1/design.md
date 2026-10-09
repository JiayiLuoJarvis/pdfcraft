# 刀 12 候选 1：专用 `hline.rs` + `HLine` 构建器（镜像 ImageNode）

Tip：`feat/hline` @ `7cb1654`。产品依据 `docs/提案书.md` §2.3（270-285）；先例 `src/layout/image.rs`、`document.rs` 的 `ImageBox` / `Flow::image` / `Cell::image`、`lower.rs::check_content`。

## Problem

§2.3 要一个**块级流节点**：当前框内一条水平实线。宽默认吃满框宽 `W`，可以定一个更短的长度并对齐（默认 Center）；占位高 `space_before + thickness + space_after`；整块不可拆，放不下就整块去下一页；绘制是 Fragment 里的一条水平 `Stroke`，在栏里只画到栏宽。

形状不是一眼就能定的，原因是三处已有代码都有些“像”，但都不对：

- `Layer::line`（§2.12）能画线，但它在页面固定坐标上画，不参与分页 → 违反“块级流节点”。
- `SpacerNode` 有 `NonNegative` 高度，但**可拆** → 违反“整条不可拆”。
- `ImageNode` 的 split 协议（宽超报错 / 高超整页报错 / 高超剩余下页 / 对齐算 x）正好是要的，但它的尺寸在**构建时**定，而分隔线的默认宽是**分页时**的 `avail.width`。

必须遵守的既有约束：`FlowNode` 只在 crate 内；失败要在 `plan_pages` / `measure_bands` 阶段，早于 `start_page`（`ImageNode` 已证明这条路成立）；`check_content` 是容器规则唯一矩阵；公共 API 不暴露 krilla；`Pen` / `Color` / `Positive` / `NonNegative` 已在 geom 且已重导出。

## Usage (caller's view)

README 片段：

```rust
use quire::{document, Cell, Color, NonNegative, Numbering, Positive, MM_TO_PT};

let short = Positive::pt(60.0 * MM_TO_PT)?;
let gap = NonNegative::pt(12.0)?;

let pdf = document(|doc| {
    doc.paragraph(|p| p.text("关于 2026 年度审计安排的通知"));
    // 标题下一条通栏细线：默认 1 pt、黑色、宽 = 当前框宽、上下无空白
    doc.hline(|_| {});

    doc.paragraph(|p| p.text("正文……"));

    // 落款前：居中 60 mm 短线，上下各留 12 pt
    doc.hline(|h| {
        h.width(short);
        h.space_before(gap);
        h.space_after(gap);
    });
    doc.paragraph(|p| p.text("某某公司（盖章）"));
})?;
```

调用点 1：附件前的灰色粗线，靠左。

```rust
let gray = Color::rgb(0x99, 0x99, 0x99);
let thick = Positive::pt(1.5)?;
let gap = NonNegative::pt(8.0)?;

doc.hline(|h| {
    h.thickness(thick);
    h.color(gray);
    h.space_before(gap);
    h.space_after(gap);
    h.start();
});
doc.paragraph(|p| p.text("附件：1. 审计范围清单"));
```

调用点 2：列表项里分隔两段说明（经嵌套 `Flow` 自然到达，无新 API）。

```rust
doc.ol(Numbering::Decimal, |list| {
    list.item(|item| {
        item.paragraph(|p| p.text("第一阶段：资料收集"));
        item.hline(|h| h.space_before(small_gap)); // small_gap: Length<NonNegative>，闭包外构造
        item.paragraph(|p| p.text("负责人：张三"));
    });
});
```

调用点 3：表格里的签字栏（单元格只有一条线，宽吃单元格内容宽）。

```rust
doc.table(|t| {
    t.row(["甲方签字", "乙方签字"]);
    t.row([Cell::hline(|_| {}), Cell::hline(|_| {})]);
});
```

栏内（`columns` 的 `col`）同样就是 `col.hline(...)`，线只画到栏宽。

调用方**看不到**：`HLineNode`、`FlowNode`、`TextAlign`、`Stroke`、`Paint`、高度计算、分页判定。

## Shape

### 模块地图（diff 面）

| 文件 | 变化 |
| --- | --- |
| `src/layout/hline.rs` | **新建**：`HLineNode` + `split` + 单测 |
| `src/layout/mod.rs` | `pub mod hline;` |
| `src/layout/flow.rs` | `FlowNode::HLine(HLineNode)` + `split` 一支 |
| `src/layout/lower.rs` | `check_content` 把 `HLine` 并入 `Text \| Paragraph \| Spacer` 那一支；`node_name` 加 `"horizontal line"` |
| `src/document.rs` | `pub struct HLine` 构建器；`Flow::hline`；`Cell::hline`；`CellContent::HLine` |
| `src/lib.rs` | 重导出 `HLine` |
| `tests/document.rs` | 集成：RecordingCanvas 断言 + `to_pdf` `%PDF` 冒烟 |

调用链 `Flow::hline → HLine::lower → HLineNode::split`，三个文件，不再深。

### 内部节点（`src/layout/hline.rs`）

```rust
//! Atomic horizontal rule. Width resolves against the frame at split time.

use crate::error::{Error, Result};
use crate::geom::{Color, Length, NonNegative, Paint, Path, Point, Positive, Stroke};
use crate::layout::flow::{FlowNode, Fragment, FragmentCommand, Frame, Split};
use crate::layout::style::TextAlign;

/// A solid horizontal line in the flow.
///
/// Invariants carried by the field types: thickness > 0, spaces >= 0, width (when set) > 0.
/// Height is derived, never stored.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HLineNode {
    /// `None` = full width of whichever frame the node lands in.
    width: Option<Length<Positive>>,
    align: TextAlign,
    thickness: Length<Positive>,
    color: Color,
    space_before: Length<NonNegative>,
    space_after: Length<NonNegative>,
}

impl HLineNode {
    /// Total. Every invalid input is unrepresentable in the argument types.
    pub(crate) fn new(
        width: Option<Length<Positive>>,
        align: TextAlign,
        thickness: Length<Positive>,
        color: Color,
        space_before: Length<NonNegative>,
        space_after: Length<NonNegative>,
    ) -> Self {
        todo!("not implemented")
    }

    /// `space_before + thickness + space_after`. Single source of the node height.
    /// May be +inf for absurd inputs; `split` turns that into `UnsplittableTallerThanFrame`.
    fn height(&self) -> f32 {
        todo!("not implemented")
    }

    /// Mirrors `ImageNode::split`; no `LayoutCtx` needed.
    pub(crate) fn split(self, avail: Frame) -> Result<Split> {
        // let width  = self.width.map_or(avail.width, Length::as_pt);
        // let height = self.height();
        // if width  > avail.width       { Err(UnbreakableWiderThanFrame { width, avail: avail.width }) }
        // if height > avail.full_height { Err(UnsplittableTallerThanFrame) }
        // if height > avail.height      { Ok(MoveToNextPage(FlowNode::HLine(self))) }
        // let x = match align { Start => 0, Center => (W - width)/2, End => W - width };
        // let y = space_before + thickness / 2;          // stroke on the geometric centre line,
        //                                                // same convention as table borders
        // Fit { height, fragment: Fragment {
        //     height, spacing_after: 0.0,
        //     commands: vec![Stroke { path: Path::line((x, y), (x + width, y)),
        //                              paint: Paint::solid(color), stroke: Stroke::new(thickness) }] } }
        todo!("not implemented")
    }
}
```

判定顺序与 `ImageNode` 一致：**宽先于高**（`wider_beats_taller` 同款测试）。

### 接线（`flow.rs` / `lower.rs`）

```rust
// flow.rs
pub(crate) enum FlowNode {
    // ...existing...
    Image(ImageNode),
    HLine(HLineNode),
}
// FlowNode::split:
//     FlowNode::HLine(node) => node.split(avail),

// lower.rs::check_content
//     FlowNode::Text(_) | FlowNode::Paragraph(_) | FlowNode::Spacer(_) | FlowNode::HLine(_) => true,
// lower.rs::node_name
//     FlowNode::HLine(_) => "horizontal line",
```

Band 内部也放行：公共 `Band` 只有 left/right 文本，放不进线；内部与 Spacer 同支，免一个特例分支。

### 公共构建器（`src/document.rs`）

```rust
/// A solid horizontal line in the flow, cell, or list item it is added to.
///
/// Defaults: full width of the current frame, centered when shorter, 1 pt thick, black,
/// no space above or below. The line never splits; it moves to the next page whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct HLine {
    width: Option<Length<Positive>>,
    align: TextAlign,
    thickness: Length<Positive>,
    color: Color,
    space_before: Length<NonNegative>,
    space_after: Length<NonNegative>,
}

impl Default for HLine {
    fn default() -> Self { todo!("not implemented") } // None, Center, 1 pt, rgb(0,0,0), 0, 0
}

impl HLine {
    /// Sets the line length. Longer than the frame is an error when laid out.
    pub fn width(&mut self, width: Length<Positive>) { todo!("not implemented") }
    /// Line thickness.
    pub fn thickness(&mut self, thickness: Length<Positive>) { todo!("not implemented") }
    /// Line color, including alpha.
    pub fn color(&mut self, color: Color) { todo!("not implemented") }
    /// Blank space above the line. Counts toward the line's height.
    pub fn space_before(&mut self, space: Length<NonNegative>) { todo!("not implemented") }
    /// Blank space below the line. Counts toward the line's height.
    pub fn space_after(&mut self, space: Length<NonNegative>) { todo!("not implemented") }
    /// Places a shorter line at the start of the frame.
    pub fn start(&mut self) { todo!("not implemented") }
    /// Places a shorter line at the end of the frame.
    pub fn end(&mut self) { todo!("not implemented") }

    /// Infallible: nothing here can be invalid, and there is no document-bound handle to check.
    fn lower(self) -> FlowNode {
        // FlowNode::HLine(HLineNode::new(self.width, self.align, self.thickness,
        //                                self.color, self.space_before, self.space_after))
        todo!("not implemented")
    }
}

impl Flow {
    /// Appends a horizontal line that moves to the next page whole instead of splitting.
    ///
    /// Longer than the frame, or taller than the page body, is an error. The line never shrinks.
    pub fn hline(&mut self, f: impl FnOnce(&mut HLine)) {
        // let mut line = HLine::default(); f(&mut line); self.push(Ok(line.lower()));
        todo!("not implemented")
    }
}

enum CellContent {
    Text(String),
    Image(Image, ImageBox),
    HLine(HLine),
}

impl Cell {
    /// A one-by-one cell holding a horizontal line as wide as the cell content.
    pub fn hline(f: impl FnOnce(&mut HLine)) -> Self { todo!("not implemented") }
}
// Table::lower:   CellContent::HLine(line) => vec![line.lower()],
```

```rust
// lib.rs
pub use document::{ /* ... */ HLine, /* ... */ };
```

### 关键决定

1. **宽度延迟到 split 解析。** `Option<Length<Positive>>` 里的 `None` 就是“框宽”，这和 ImageBox 在构建时定尺寸不同，是唯一的结构性偏离。同一个节点放进正文、栏、单元格、列表项时宽度都对，不需要知道自己在哪个容器里（per single-source-of-truth：宽只来自 `avail.width`）。
2. **高度派生，不存。** `height()` 是三项之和；不存 `height` 字段，构建器改了某一项也不会和总高脱节。
3. **不变量全在类型里。** thickness / width 用 `Positive`，空白用 `NonNegative`，颜色用 `Color`（全域合法）。所以 `HLine::lower` 和 `HLineNode::new` 是**全函数**，DSL 组装期没有任何运行期校验（per encode-lessons-in-structure / boundary-discipline）。只有依赖框的检查（宽超、高超）留在 `split`，它们本来就只能在那时判。
4. **溢出走已有错误。** 三个 f32 相加溢出成 `+inf` 时，`height > full_height` 成立，返回 `UnsplittableTallerThanFrame`，不会走到 `Point::try_pt` 的 `InvalidGeometry`。不需要新错误变体。
5. **`space_after` 计入 `height`，`spacing_after = 0`。** 提案明确“计入节点高度”。如果走 stack 的 `spacing_after` 通道，空白在页底会被吞掉，节点也就不再是“整条不可拆”的。
6. **线画在几何中线** `y = space_before + thickness/2`。线条恰好占满 `[space_before, space_before + thickness]` 这一段；krilla 默认 butt cap，所以 x 方向正好是 `[x, x+width]`，不会冒出框。
7. **容器矩阵只改一处。** `check_content` 里 HLine 与 Spacer 同权（Column / TableCell / ListItem / Band 全放行）。列表项能用线是**零额外 API**：`List::item` 本来就给嵌套 `Flow`。
8. **`Cell::hline` 只为线开一个入口**，不顺手做通用 cell stack（how.md Gotchas）。

### 接口深度

公共面新增 1 个类型 + 7 个 setter + `Flow::hline` + `Cell::hline`。背后藏着的是：框宽解析、对齐取 x、中线 y、高度派生、三档分页判定、Stroke 组装、栏和单元格内的偏移（由既有 `with_offset` 免费提供）。调用方只表达“要什么线”，不碰几何。

### 不做

虚线 / 双线 / 竖线；线帽、线连接配置；默认色跟随当前文字色（见 Open questions）；通用单元格 stack；Band 公共 API 上的线；流内 canvas；填空下划线。

### 测试计划（全部断言 RecordingCanvas）

`hline.rs` 单测：
- `default_spans_frame_on_centre_line`：`Frame{w:300}`、thickness 2、before 5、after 3，得到 `Fit.height == 10`，路径点 `[(0,6),(300,6)]`，`spacing_after == 0`。
- `short_line_aligns_center_start_end`：width 100 / W 300，三种对齐的 x 分别为 100 / 0 / 200。
- `wider_beats_taller_and_short_frame_moves`：宽超 → `UnbreakableWiderThanFrame{width, avail}`；高 > full → `UnsplittableTallerThanFrame`；高 > 剩余 → `MoveToNextPage(FlowNode::HLine(_))`。
- `overflowing_spaces_are_taller_than_frame`：`space_before = f32::MAX` 时 → `UnsplittableTallerThanFrame`。
- `column_offsets_the_line`：两栏，`test_util::strokes` 得到的线只画到各自栏宽。
- `recording_pins_stroke_paint_and_width`：`CanvasCommand::StrokePath` 的 paint 颜色与 stroke 宽度 bits。

`lower.rs` / `list.rs` / `table.rs`：既有 reject 测里补一条“HLine 在 ListItem / TableCell 中通过”。

`tests/document.rs`：
- 页底剩余空间小于线高时，线整块去第 2 页（断言 `rec.pages()[1]` 的 stroke y）。
- 线高 > 正文框 → `Err(UnsplittableTallerThanFrame)`，`rec.pages()` 为空（证明失败早于 `start_page`）。
- `Cell::hline` 与 `item.hline` 各一例，断言 stroke 坐标。
- `#[cfg(feature = "backend-krilla")]` `to_pdf` 以 `%PDF` 开头。

## Synthesis decision

(arena fills)

## Tradeoffs accepted

- 我们接受**每次构建都要写闭包**（`doc.hline(|_| {})`），换来与 `Flow::image(&img, |_| {})` 同一种调用形态，并且以后加选项不会破坏签名。
- 我们接受 `HLine` 只有 `start()` / `end()`、没有 `center()`，而 `ImageBox` 只有 `center()` / `end()`，两者不对称。原因是默认值不同（§2.3 Center、§2.7 Start），setter 只表达“离开默认”。复制 ImageBox 的 agent 写出 `h.center()` 会直接编译失败，而不是悄悄写出一个冗余调用。
- 我们接受 thickness 和 color 拆成两个 setter，而不复用 `Pen`，换来每一项都有独立默认值：只改颜色时不必顺带指定线粗。
- 我们接受 `space_after` 在页底**不被吞**（与段落 `spacing_after` 行为不同），换来提案字面“占位高 = 三项之和、整条不可拆”。
- 我们接受 Band 内部矩阵放行一个公共 API 造不出来的节点，换来 `check_content` 少一个特例分支。

## Alternatives considered

- **复用 `Pen`：`h.pen(Pen::new(color, width))`。** 与 `Layer::line` 词汇一致，但颜色和线粗被绑成一组，丢掉了各自的默认值，调用方只想改色也得选线粗。对外面没缩小，反而泄露了组合方式。
- **`HLineNode` 并入 `flow.rs`（像 `SpacerNode`）。** 省一个文件，但 flow.rs 已经承担协议和 stack / keep_together；分隔线的判定和单测体量与 image 相当，image 有独立文件，这里同样应该独立。从 image 先例复制的 agent 也会自然落到 `hline.rs`。
- **构建时把宽度解析成 `Size`（完全照抄 ImageNode）。** 构建时拿不到框宽，只能强迫调用方给宽，这违反“默认 = 框宽”，在栏里也会出错。

## Open questions and risks

- 默认颜色固定为黑，还是跟随当前 `Flow` 的 `style.color`？今天公共 API 改不了文字色，两者结果相同；等文字色可配后才会分叉。是否现在就钉成“跟随文字色”？
- 默认线粗 1 pt 是实现自定，提案没写。是否在提案 §2.3 表里补一行？
- 单元格里的线吃的是**内容宽**（扣掉 padding），不是单元格边框宽。签字栏场景接受这个结果吗？
- DSL 名 `hline` 与 `Layer::line` 只差前缀；文档注释里是否需要互相指一下，说明流内线和页面固定坐标线的区别？
- 线以 alpha < 255 绘制时，与表格边框叠加时的观感未验证（低风险）。

## Next implementation step

先写 `src/layout/hline.rs` 的 `HLineNode::split` 和它的单测（中线 y、三种对齐、宽先于高），再接上 `FlowNode::HLine`，最后接 DSL。
