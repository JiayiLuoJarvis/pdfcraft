# 流内分隔线：`Rule` 值 + `Flow::rule`

块级水平分隔线（提案书 §2.3）。内部类型叫 `HLineNode`，紧挨 `SpacerNode` 放在 `src/layout/flow.rs`。公共 DSL 叫 `rule`。线色和线粗合成一支已有的 `Pen`。调用方先用链式 setter 拼出一个 `Rule` 值，再交给 `flow.rule`。单元格用 `Cell::flow` 接一段与正文相同的 `Flow`，分隔线、spacer、段落走同一套方法。

`publish = false` 的 crate `quire-pdf` 保持不动。不新增依赖，不新增模块。

## Usage

调用方只接触 `Rule`、`Flow::rule`、`Cell::flow`。`HLineNode` 不重导出。`TextAlign` 保持 `pub(crate)`；对齐用 `start` / `center` / `end`。

默认：黑、1 pt、实线、宽等于当前框 `W`、对齐 Center、`space_before` 与 `space_after` 为 0。`Pen` 实现 `Into<Rule>`，所以单色单粗的通栏线不用再写 builder。

```rust
use quire_pdf::{document, Color, NonNegative, PageSize, Pen, Positive, Rule};

let rendered = document(PageSize::A4, |doc| {
    doc.paragraph(|p| p.text("正文"));
    doc.rule(Rule::default());
    doc.rule(Pen::new(Color::rgb(180, 180, 180), Positive::pt(0.5)?));
    doc.rule(
        Rule::new(Pen::new(Color::rgb(0, 0, 0), Positive::pt(1.0)?))
            .width(Positive::pt(72.0)?)
            .start()
            .space_before(NonNegative::pt(6.0)?)
            .space_after(NonNegative::pt(6.0)?),
    );
    Ok(())
})?;
```

列表项已经是嵌套 `Flow`。分隔线直接写进 `item`，列表 API 不增方法。

```rust
doc.ul(|list| {
    list.item(|flow| {
        flow.paragraph(|p| p.text("附件"));
        flow.rule(Rule::default());
    });
});
```

单元格里要堆段落、空白和线时，用一条嵌套流。没有 `Cell::rule`，也没有 `Cell::spacer`。

```rust
doc.table(|table| {
    table.row([
        Cell::new("名称"),
        Cell::flow(|flow| {
            flow.paragraph(|p| p.text("说明"));
            flow.spacer(NonNegative::pt(4.0).unwrap());
            flow.rule(Rule::default());
        })
        .col_span(NonZeroU16::new(2).unwrap()),
    ]);
});
```

`Flow::rule` 与 `Flow::spacer` 一样立刻把节点推进当前流，返回 `()`。链式调用发生在 `Rule` 上，发生在推进之前。`width` 长于最终框宽时，setter 不报错：框宽要到分页才知道。

## Module map

```text
src/layout/flow.rs     HLineNode 放在 SpacerNode 旁边；FlowNode::HLine；split
src/layout/lower.rs    check_content 与 node_name 各加一条
src/document.rs        Rule、Flow::rule、CellContent::Flow、Cell::flow、Table::lower
src/lib.rs             重导出 Rule
tests/document.rs      列表、单元格、to_pdf 冒烟

不新增 src/layout/hline.rs
不改 layout/mod.rs 的模块列表
不改 canvas、backend、geom、Band、Layer
```

依赖方向不变：`document` 把 `Rule` 降成 `HLineNode`；`layout` 不认识 `Rule`。`Pen` / `Path` / `Stroke` 仍来自 `geom`。

## Types

`Rule` 是 `Copy` 值。可选宽度用 `Option`：`None` 表示当前框宽，不用哨兵长度。对齐有具体默认值，不用 `Option`。

```rust
/// 当前框里的一条水平实线。
///
/// 未设置宽度时线长等于落点框宽。短于框宽时默认居中。
/// 节点高度是 `space_before + pen 线宽 + space_after`，整段不可拆。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Rule {
    pen: Pen,
    width: Option<Length<Positive>>,
    align: TextAlign,
    space_before: Length<NonNegative>,
    space_after: Length<NonNegative>,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            pen: Pen::new(Color::rgb(0, 0, 0), Positive::pt(1.0).expect("1 pt")),
            width: None,
            align: TextAlign::Center,
            space_before: NonNegative::ZERO,
            space_after: NonNegative::ZERO,
        }
    }
}

impl Rule {
    /// 通栏、居中、上下空白为 0，笔为 `pen`。
    pub fn new(pen: Pen) -> Self { /* pen, 其余同 Default */ }

    pub fn pen(self, pen: Pen) -> Self { /* 替换笔 */ }
    /// 定长。长于落点框宽时在分页报 `UnbreakableWiderThanFrame`。
    pub fn width(self, width: Length<Positive>) -> Self { /* Some(width) */ }
    pub fn start(self) -> Self { /* TextAlign::Start */ }
    pub fn center(self) -> Self { /* TextAlign::Center */ }
    pub fn end(self) -> Self { /* TextAlign::End */ }
    pub fn space_before(self, space: Length<NonNegative>) -> Self { /* … */ }
    pub fn space_after(self, space: Length<NonNegative>) -> Self { /* … */ }

    fn lower(self) -> FlowNode {
        FlowNode::HLine(HLineNode {
            pen: self.pen,
            width: self.width,
            align: self.align,
            space_before: self.space_before,
            space_after: self.space_after,
        })
    }
}

impl From<Pen> for Rule {
    fn from(pen: Pen) -> Self {
        Self::new(pen)
    }
}

impl Flow {
    /// 追加一条水平分隔线。
    pub fn rule(&mut self, rule: impl Into<Rule>) {
        self.push(Ok(rule.into().lower()));
    }
}
```

`Into<Rule>` 只为 `Rule` 与 `Pen` 实现。没有从长度或颜色单独转 `Rule` 的实现。

布局节点与 `SpacerNode` 同文件，`split` 不拿 `LayoutCtx`（线不碰字体）。

```rust
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HLineNode {
    pen: Pen,
    width: Option<Length<Positive>>,
    align: TextAlign,
    space_before: Length<NonNegative>,
    space_after: Length<NonNegative>,
}

pub(crate) enum FlowNode {
    // …现有变体…
    HLine(HLineNode),
}

// FlowNode::split 的 match：
// FlowNode::HLine(node) => node.split(avail),
```

`HLineNode::split` 的顺序与 `ImageNode::split` 相同：先宽，再整页高，再本页剩余高。宽等于框宽算放得下（比较用 `>`）。`None` 宽度取 `avail.width`，因此默认线不会因超宽失败。

```rust
impl HLineNode {
    pub(crate) fn split(self, avail: Frame) -> Result<Split> {
        let line_w = match self.width {
            Some(width) => width.as_pt(),
            None => avail.width,
        };
        if line_w > avail.width {
            return Err(Error::UnbreakableWiderThanFrame {
                width: line_w,
                avail: avail.width,
            });
        }
        let height =
            self.space_before.as_pt() + self.pen.width().as_pt() + self.space_after.as_pt();
        // 三项各自有限，和可能溢出成无穷。
        if !height.is_finite() {
            return Err(Error::InvalidGeometry);
        }
        if height > avail.full_height {
            return Err(Error::UnsplittableTallerThanFrame);
        }
        if height > avail.height {
            return Ok(Split::MoveToNextPage(FlowNode::HLine(self)));
        }
        let x = match self.align {
            TextAlign::Start => 0.0,
            TextAlign::Center => (avail.width - line_w) / 2.0,
            TextAlign::End => avail.width - line_w,
        };
        // 线画在所占厚度的几何中线，与表边框一致。
        let y = self.space_before.as_pt() + self.pen.width().as_pt() / 2.0;
        let path = Path::line(Point::try_pt(x, y)?, Point::try_pt(x + line_w, y)?);
        Ok(Split::Fit {
            height,
            fragment: Fragment {
                height,
                spacing_after: 0.0,
                commands: vec![FragmentCommand::Stroke {
                    path,
                    paint: Paint::solid(self.pen.color()),
                    stroke: Stroke::new(self.pen.width()),
                }],
            },
        })
    }
}
```

`spacing_after` 固定为 0。上下空白在节点高度里面，stack 在放不下段后距时丢掉的是段落的 `spacing_after`，丢不掉这条线自己的空白。`split` 不返回 `Partial`。`MoveToNextPage` 把同一个 `HLineNode` 交回去。

节点只看见传给它的 `Frame`。栏里这个框的宽是列宽，单元格里是扣过 inset 的内容宽（`table::measure` 已有的 `inner_w` / `inner_full`）。线不自己算页边距或列间距。

## 单元格嵌套流

`CellContent` 增加一条臂，装着推迟到 `Table::lower` 才运行的流闭包。`Cell::new` 与 `Cell::image` 保留，现有调用点不动。

```rust
enum CellContent {
    Text(String),
    Image(Image, ImageBox),
    Flow(Box<dyn FnOnce(&mut Flow)>),
}

impl Cell {
    /// 格内一段流。段落、spacer、分隔线、图片、列表都用 `Flow` 上已有的方法。
    ///
    /// 闭包在 `Table::lower` 时运行，因此是 `'static`：捕获拥有的字符串、`Image`、`Rule`。
    /// 栏、表、pageBreak、keep_together 能写进闭包，随后被 `check_content(TableCell)` 拒绝。
    pub fn flow(f: impl FnOnce(&mut Flow) + 'static) -> Self { /* CellContent::Flow */ }
}
```

`Table::lower` 里这一臂：

```rust
CellContent::Flow(build) => {
    let mut child = flow.child();
    build(&mut child);
    child.into_nodes()?
}
```

然后照旧 `table::Cell::new(content)`，由 `CellBody::new` 跑 `check_content`。

`FnOnce` 不能 `Clone`，也不能 `Eq`。`Cell` 去掉 `Clone`、`PartialEq`、`Eq` 派生，改手写 `Debug`（流臂印成 `Flow(..)`）。`col_span` / `row_span` 仍按值消费 `self`，闭包跟着走。

## 允许矩阵

`check_content` 把 `HLine` 跟 spacer、段落放在同一条允许臂上：Column、TableCell、ListItem、Band 都通过。

```rust
FlowNode::Text(_) | FlowNode::Paragraph(_) | FlowNode::Spacer(_) | FlowNode::HLine(_) => true,
```

`node_name` 返回 `"rule"`，与 DSL 用词一致。Band 的公共方法仍只有 `left` / `right`，调用方写不出分隔线；允许臂与 spacer 对齐，避免为 Band 单写一条拒绝。列表项、栏、单元格因此不用各自再开一个入口。

根上的 `Flow::rule` 不经过 `check_content`（与 spacer 相同）。非法组合只在嵌套容器 lower 时出现，错误仍是 `InvalidCellContent`。

## 数据流

```text
Rule（或 Pen）
  → Flow::rule / 列表 item 的 Flow / Cell::flow 在 lower 时展开的 Flow
  → FlowNode::HLine
  → plan_pages 里 FlowNode::split
  → Fit 的 FragmentCommand::Stroke | MoveToNextPage | Err
  → paint_pages → draw_fragment → Canvas::stroke_path
  → RecordingCanvas 的 StrokePath
  → to_pdf 重放，字节以 %PDF 开头
```

超宽、超高、高度和非有限，都从 `HLineNode::split` 返回。`plan_pages` 在 `paint_pages` 之前；`paint_pages` 才调用 `Canvas::start_page`。失败路径不进入 `start_page`，`RecordingCanvas` 保持没有页。单元格内容在 `table::measure` 里经 `natural_height` 再 `split`，仍在 `plan_pages` 内。格内测量框的 `height` 与 `full_height` 都是 `inner_full`，线高于该框时直接 `UnsplittableTallerThanFrame`，不会在格内变成 `MoveToNextPage`。整行放不下仍由表自己把行挪到下一页。

## 测试

`HLineNode::split` 单测放在 `flow.rs` 的测试模块，框直接构造，不断言 PDF 字节。

| 输入 | 结果 |
| --- | --- |
| 框宽 200，默认笔，宽度 `None` | 高 1；线从 `(0, 0.5)` 到 `(200, 0.5)`；`Stroke` 宽 1、色黑；`spacing_after` 为 0 |
| 定长 100，默认 Center | x = 50，线到 x = 150 |
| 定长 100，`start` / `end` | x = 0 / x = 100 |
| `space_before` 4、笔宽 2、`space_after` 6 | 高 12；y = 5 |
| 定长 201，框宽 200，同时高度也超过 `full_height` | `UnbreakableWiderThanFrame { width: 201.0, avail: 200.0 }` |
| 高度大于 `full_height`、宽放得下 | `UnsplittableTallerThanFrame` |
| 高度大于 `avail.height`、不大于 `full_height` | `MoveToNextPage`，节点字段不变 |
| 任何放得下的线 | 不是 `Partial` |

绘制：用 `layout::test_util::render` 把节点分页，再用 `strokes` 断言落在正文原点上的端点（页边距已知，点等于原点加 fragment 内坐标）。笔的颜色和线宽从 `StrokePath` 的 paint / stroke 读出。

失败：同一 `render`，超宽或超高时 `Result` 为对应错误，且 `rec.commands()` 为空（`start_page` 未调用）。

允许矩阵：`CellBody::new` / `ItemBody::new` 接受单个 `FlowNode::HLine`。现有单元格拒绝（columns、pageBreak、keep_together、嵌套 table）和列表项拒绝（table、pageBreak、image）保持原列表。

`tests/document.rs`：

- 正文 `rule`、列表项里的 `rule`、`Cell::flow` 里的段落 + spacer + `rule`，在 `backend-krilla` 下 `to_pdf()` 的字节以 `%PDF` 开头。不断言内容流数字。
- 定长宽于页正文框时 `document(...).unwrap_err()` 为 `UnbreakableWiderThanFrame`，不调用 `to_pdf`。

## 不做

虚线、双线、竖线、行内填空、`Layer::line`、流内 canvas、`Flow::hline`、`Cell::rule`。`Stroke` 不加重字段。
