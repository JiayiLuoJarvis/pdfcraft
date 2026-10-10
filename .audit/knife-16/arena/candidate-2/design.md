# Knife 16 设计：`Cell::spacer`

## 结论

在现有单内容单元格模型中增加 `Cell::spacer(height)`。它复用正文已经存在的 `FlowNode::Spacer` 与 `SpacerNode`，只补齐公共 `CellContent` 到布局节点的桥接，不增加布局类型，也不改变表格按整行分页的规则。

本设计遵循 arena frame 的全部 autonomy defaults，没有启用任何 reverse word。

## 1. 调用方用法

基本用法与 `Cell::hline`、`Cell::image` 并列：

```rust
let gap = NonNegative::pt(24.0)?;

let rendered = document(PageSize::A4, |doc| {
    doc.font(font_bytes)?;
    doc.table(|t| {
        t.row([Cell::new("上方内容")]);
        t.row([Cell::spacer(gap)]);
        t.row([Cell::new("下方内容")]);
    });
    Ok(())
})?;
```

零高度合法：

```rust
t.row([Cell::spacer(NonNegative::ZERO)]);
```

跨度继续使用现有修饰方法，不需要 spacer 专用 API：

```rust
t.row([
    Cell::spacer(NonNegative::pt(12.0)?)
        .col_span(NonZeroU16::new(2).unwrap()),
]);
```

单元格仍只有一个公开 payload。因此本刀可以表达“只有竖直空白的单元格”，不能表达“文字 + spacer + 文字”的单个单元格；后者不是本 API 的隐含能力。

## 2. 类型、枚举分支与签名

### `src/document.rs`

私有存储枚举增加一个分支：

```rust
enum CellContent {
    Text(String),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>),
}
```

公共构造函数：

```rust
impl Cell {
    /// A one-by-one cell holding vertical space that paints nothing.
    pub fn spacer(height: Length<NonNegative>) -> Self;
}
```

实现与 `Cell::hline` 的一乘一初始化保持一致：

```rust
Self {
    content: CellContent::Spacer(height),
    col_span: NonZeroU16::MIN,
    row_span: NonZeroU16::MIN,
}
```

签名不返回 `Result`：负值、`NaN` 和无穷值已经无法构造成 `Length<NonNegative>`；零值由 `NonNegative::ZERO` 合法表达。`Length<NonNegative>` 已实现 `Clone + Copy + PartialEq + Eq`，所以 `Cell` 和 `CellContent` 现有 derive 无需调整。

不增加 `From<Length<NonNegative>> for Cell`。图片和水平线都使用命名构造函数，命名的 `Cell::spacer` 也比裸长度到单元格的隐式转换更清楚。

### `Table::lower`

现有穷尽匹配增加：

```rust
CellContent::Spacer(height) => {
    vec![FlowNode::Spacer(SpacerNode { height })]
}
```

`SpacerNode` 已因 `Flow::spacer` 被 `src/document.rs` 导入，不需要新 import。每个公开 `Cell` 仍 lower 成恰好一个元素的 `Vec<FlowNode>`。

### 不增加的类型

- 不增加 `CellSpacer` 或第二种 spacer 布局节点。
- 不增加公共 cell-flow builder、闭包或 `Vec<CellContent>`。
- 不增加错误枚举；过高情况沿用 `Error::UnsplittableTallerThanFrame`。
- 不增加 re-export；`Cell`、`Length`、`NonNegative` 已在 crate 根导出。

## 3. 模块与文件变更图

| 文件 | 计划变更 |
| --- | --- |
| `src/document.rs` | 增加 `CellContent::Spacer`、`Cell::spacer`、`Table::lower` 的匹配分支和简短 rustdoc。 |
| `tests/document.rs` | 增加经 `document(...).pages()` 的公共 API、几何、无绘制和错误测试。 |
| `src/layout/table.rs` | 仅在现有 `#[cfg(test)]` 模块补一个 `cell_allows_spacer` 单元测试；不改布局实现。 |

下列文件保持不动：

- `src/layout/flow.rs`：继续复用 `FlowNode::Spacer`、`SpacerNode::split`、`natural_height`。
- `src/layout/lower.rs`：`Container::TableCell` 的 allowlist 已允许 `FlowNode::Spacer`。
- `src/layout/table.rs` 的非测试代码：测量、row-span、整行分页均无需变化。
- `src/geom.rs`、`src/error.rs`、`src/lib.rs`、`src/canvas.rs` 与全部 backend 文件。
- `Cargo.toml` 和依赖版本。

依赖路径仍是 `document -> layout -> canvas/fonts/text`；没有 krilla 类型越过 backend 边界。

## 4. Lower、测量与分页行为

### 4.1 Lower 路径

路径为：

```text
Cell::spacer(height)
  -> CellContent::Spacer(height)
  -> Table::lower
  -> vec![FlowNode::Spacer(SpacerNode { height })]
  -> table::Cell::new
  -> CellBody::new
  -> check_content(Container::TableCell)
```

`check_content` 已允许 spacer，因此不应为本刀增加或复制 allowlist 规则。columns、page break、keep-together 和嵌套 table 的拒绝规则保持原样。

### 4.2 `natural_height`

`CellBody::into_node` 会把单元素 vector 包成 `FlowNode::Stack`。表格测量先算：

```text
inset = border_width / 2 + padding
inner_full = page_body_full_height - 2 * inset
cell_frame = Frame {
    width: inner_width,
    height: inner_full,
    full_height: inner_full,
}
```

随后调用：

```text
natural_height(Stack([Spacer(height)]), cell_frame)
```

结果如下：

| 高度 | `SpacerNode::split` | 单元格测量结果 |
| --- | --- | --- |
| `0` | `Split::Fit { height: 0 }` | 成功；内容高 0；无绘制命令 |
| `0 < h <= inner_full` | `Split::Fit { height: h }` | 成功；cell 高为 `h + 2 * inset` |
| `h > inner_full` | `Split::Partial` | `natural_height` 转成 `Error::UnsplittableTallerThanFrame` |

这里必须诚实地区分正文 spacer 与格内 spacer。正文分页器保留 `Partial`，所以正文 spacer 可以跨页继续；表格必须先得到每个 cell 的自然高，再按物理整行分页，`natural_height` 只接受 `Fit`。因此格内 spacer 不会在单元格中途跨页拆分。

### 4.3 行高、分页和绘制

- 普通行高取该行各 cell 高度的最大值；spacer 只通过高度影响行几何。
- 对 row-span，现有算法把不足高度补到 span 的最后一行；spacer 不引入例外。
- 测量成功后，当前页剩余空间不够时，现有 `split_measured` 把整个行或 rowspan 连通组移到下一页。
- spacer fragment 的 `commands` 为空；它不产生 glyph、stroke、fill、image 或 link 命令。
- 表头 cell 也可使用同一构造函数；若重复表头包含 spacer，其空白高度随整个表头重复，不需要专门逻辑。
- 当前公共表默认 padding 和 border 都是 0，因此目前 `inner_full` 等于正文框全高；设计仍按未来非零 inset 后的真实公式描述。

## 5. 测试计划

### 5.1 公共几何与零高度：`tests/document.rs`

新增 `cell_spacer_shifts_following_row_without_painting`：

1. 建一个单列表；第一行是 `Cell::spacer(h)`，第二行是文字 `"B"`。
2. 分别以 `h = NonNegative::ZERO` 和 `h = 30 pt` 经 `document(...).pages()` 渲染。
3. 两份结果都应只有一页。
4. 用现有 `runs`/`glyph` helper 找到 `"B"`，以 `pt_near` 断言：

   ```text
   y(30 pt) - y(0 pt) == 30 pt
   ```

5. 断言每页只有 `"B"` 对应的 `DrawGlyphRun`，没有 `StrokePath`、`FillPath`、图片或链接命令。这样同时证明 spacer 改变几何但不制造 ink；不以页数代替几何证明。

使用单 cell 的 spacer 行，避免同行文字高度通过 `max(cell heights)` 掩盖预期的 30 pt 行高。

### 5.2 过高错误：`tests/document.rs`

新增 `cell_spacer_taller_than_body_is_unsplittable`：

1. 使用 `PageSize::custom(Size::try_pt(100.0, 20.0)?)`，默认 margin/padding/border 为 0。
2. 表中只放一个 `Cell::spacer(21 pt)`。
3. 断言 `document(...)` 返回 `Error::UnsplittableTallerThanFrame`，而不是生成两页或静默裁切。

这项测试专门锁住 `natural_height` 拒绝 `Partial` 的现状，防止误把正文 spacer 的跨页语义套进 cell。

### 5.3 非法长度边界：`tests/document.rs`

新增或就近扩展长度错误测试，在外层 document build closure 中先执行
`let height = NonNegative::pt(-1.0)?;`，再把 `height` 传给表闭包中的
`Cell::spacer(height)`。断言错误原样为 `Error::NegativeLength { value: -1.0 }`。
长度构造在进入返回 `()` 的 table closure 前失败；错误来自品牌长度边界，而不是
`Cell::spacer` 或 layout，不为 cell 重复运行时校验。

### 5.4 内部 allowlist 回归：`src/layout/table.rs`

在现有 `cell_allows_hline` 旁增加：

```rust
#[test]
fn cell_allows_spacer() {
    Cell::new(vec![FlowNode::Spacer(crate::layout::flow::SpacerNode {
        height: NonNegative::pt(1.0).unwrap(),
    })])
    .unwrap();
}
```

这只验证既有 seam，布局生产代码不变。现有 `cell_rejects_columns_page_break_keep_together_and_tables` 必须继续通过，证明本刀没有顺带放开禁用内容。

### 5.5 验证命令

实现后运行：

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

本刀不涉及 backend，不需要新增 PDF 压缩字节断言；`RecordingCanvas` 是几何和无绘制行为的权威测试面。

## 6. 明确不在本刀范围

- Cell 富段落、blank、URI link。
- Cell 列表。
- 单个 cell 的多内容 stack API，或接收 `Flow` 的 cell 闭包。
- columns、pageBreak、keep_together、嵌套 table 在 cell 中的开放。
- 修改 `natural_height`，或让 cell spacer 在单元格内部跨页拆分。
- 修改正文 `Flow::spacer` 的既有跨页行为。
- 新 spacer 类型、背景色、可见占位、margin 或 paint 命令。
- canvas、QR、SVG、TOC、具名样式。
- 表格 padding、border、列宽或分页算法的公共扩展。
- krilla/backend、字体、字形测量与 PDF 冒烟测试变更。

