# Knife 17 设计：单元格富段落与列表

## 结论

公共调用面增加 `Cell::paragraph`、`Cell::ul`、`Cell::ol`，分别复用现有
`Paragraph` 与 `List`，不建立 cell 专用富文本或列表 builder。

段落闭包立即执行，保存未断行的 `Paragraph`；列表闭包延迟到
`Table::lower(&Flow)` 执行，此时用 `flow.child()` 给现有 `List` 提供正确的
style、document token 和嵌套上下文。两条路径最终仍经过既有
`CellBody::new` allowlist。

实施拆成 17a（段落）和 17b（列表）。原因不是布局缺能力，而是 17b 的
`FnOnce` 延迟闭包会给 `Cell`/`Table` 引入作用域生命周期，并使 `Cell` 无法继续
诚实实现 `Clone`/`PartialEq`/`Eq`；这应单独评审和编译验证。

本设计遵循 autonomy defaults，没有启用 reverse word。

## 1. 调用方用法

调用方式与正文同名，并与现有 cell 工厂并列：

```rust
doc.table(|t| {
    t.row([
        Cell::paragraph(|p| {
            p.text("金额：");
            p.bold("100");
            p.blank_em(2);
            p.link("https://example.test/detail", |p| p.text("明细"));
        }),
        Cell::ul(|list| {
            list.item(|item| {
                item.paragraph(|p| p.text("第一项"));
                item.hline(|line| line.start());
            });
            list.item(|item| {
                item.paragraph(|p| p.text("第二项"));
                item.ul(|nested| {
                    nested.item(|item| item.paragraph(|p| p.text("子项")));
                });
            });
        }),
    ]);

    t.row([
        Cell::ol(Numbering::UpperRoman, |list| {
            list.item(|item| item.paragraph(|p| p.text("one")));
            list.item_marked("（二）", |item| {
                item.paragraph(|p| p.text("custom"));
            });
        }),
        Cell::hline(|line| line.start()),
    ]);
});
```

`Cell::new("plain")` 继续走纯字符串快捷路径，不改成富段落。一个公开 cell
仍只有一个 payload；例如“段落 + spacer + 列表”不在本刀表达。

列表项仍是现有 `Flow`，所以项内可用段落、填空、水平线、spacer 和嵌套列表；
`columns`、page break、keep-together、table 仍由 `ListItem::new` 拒绝。

## 2. 类型与签名

### 2.1 17a：富段落

公共签名：

```rust
impl Cell {
    pub fn paragraph(f: impl FnOnce(&mut Paragraph)) -> Self;
}
```

私有存储增加：

```rust
enum CellContent {
    Text(String),
    Paragraph(Paragraph),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>),
}
```

`Cell::paragraph` 创建 `Paragraph::default()`、立即运行闭包并把完整值存下。
不能只存 `items`：还必须保留 `first_line_indent` 和首个 builder error，尤其是
`EmptyLink`、`RunAfterFillBlank`。

抽出唯一的私有 lower：

```rust
impl Paragraph {
    fn lower(self, parent: &Style) -> Result<FlowNode>;
}
```

它先返回已记录的 error，再复制 parent style、覆盖 `first_line_indent`，最终返回：

```rust
FlowNode::Text(TextNode::from_items(self.items, style))
```

`Flow::paragraph` 和 `Table::lower` 都调用这个 helper。不能生成
`FlowNode::Paragraph`，因为 cell 内宽在 table 测量时才确定。

为让 17a 的 `CellContent::Paragraph(Paragraph)` 继续支持现有调试和克隆用途，
`Paragraph` 可补 `Debug + Clone + PartialEq`；`Cell` 保留这三项，但去掉 `Eq`。
`Paragraph` 会保存带浮点几何的 inline blank 和 `Error`，不能诚实派生 `Eq`。

### 2.2 17b：作用域延迟列表

不要求 `'static`。引入作用域生命周期：

```rust
type DeferredList<'a> = Box<dyn FnOnce(&mut List) + 'a>;

pub struct Cell<'a> {
    content: CellContent<'a>,
    col_span: NonZeroU16,
    row_span: NonZeroU16,
}

enum CellContent<'a> {
    Text(String),
    Paragraph(Paragraph),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>),
    List(ListKind, DeferredList<'a>),
}

pub struct Table<'a> {
    header: Vec<Vec<Cell<'a>>>,
    body: Vec<Vec<Cell<'a>>>,
    repeat_header: bool,
}
```

公共签名方向：

```rust
impl<'a> Cell<'a> {
    pub fn ul(f: impl FnOnce(&mut List) + 'a) -> Self;

    pub fn ol(
        numbering: Numbering,
        f: impl FnOnce(&mut List) + 'a,
    ) -> Self;
}

impl Flow {
    pub fn table<'a>(&mut self, f: impl FnOnce(&mut Table<'a>));
}

impl<'a> Table<'a> {
    pub fn header<I>(&mut self, cells: I)
    where
        I: IntoIterator,
        I::Item: Into<Cell<'a>>;

    pub fn row<I>(&mut self, cells: I)
    where
        I: IntoIterator,
        I::Item: Into<Cell<'a>>;
}
```

`From<&str>`、`From<String>`、`From<&String>` 对任意 `Cell<'a>` 实现；普通调用依靠
推断，无须写生命周期。测试必须额外锁住列表闭包可以借用 table 调用之外仍存活的
局部数据，避免实现退化成 `'static`。

`CellContent::List` 在 `Table::lower(&Flow)` 中执行：

```rust
let mut list = List {
    parent: flow.child(),
    items: Vec::new(),
};
build(&mut list);
vec![list.lower(kind)?]
```

随后照常调用 `table::Cell::new(content)`，因此路径是：

```text
Cell::ul / Cell::ol
  -> deferred FnOnce
  -> Table::lower(&Flow)
  -> List { parent: flow.child(), ... }
  -> List::item -> Flow::nested -> ListItem::new
  -> List::lower -> FlowNode::List
  -> table::Cell::new -> CellBody::new
```

闭包本身不可比较，也不能普遍克隆。因此 17b 后 `Cell` 不再实现
`Clone`/`PartialEq`/`Eq`；保留一个手写 `Debug`，将列表 payload 显示为
`List(..)`，不得伪造共享 one-shot closure 的 Clone 语义。此项是 17b 的明确
公共兼容性变更，不能藏在 derive 调整里。

## 3. 模块图

| 文件 | 17a | 17b |
| --- | --- | --- |
| `src/document.rs` | `Cell::paragraph`、paragraph arm、共享 paragraph lower | `Cell<'a>`/`Table<'a>`、deferred list arm、`Cell::ul/ol`、手写 Debug |
| `tests/document.rs` | 字体、填空、链接、宽度、过高测试 | marker/item 坐标、嵌套、借用 capture、非法项内容、过高测试 |
| `src/layout/lower.rs` | 不改；TableCell 已允许 Text | 不改；TableCell 已允许 List，ListItem 规则已存在 |
| `src/layout/paragraph.rs` | 不改；继续使用 `TextNode::from_items` | 不改 |
| `src/layout/list.rs` | 不改 | 不改；继续使用现有 `ListNode`/`ListItem` |
| `src/layout/table.rs` | 不改生产代码 | 不改生产代码；继续测量 `CellBody` |
| `src/layout/flow.rs` | 不改 | 不改；`natural_height` 语义不变 |
| backend / canvas / fonts / text | 不改 | 不改 |

不新增依赖，不出现 backend 之外的 krilla 类型。

## 4. list ↔ Flow seam

选择 frame 默认的“延迟 list builder closure 到 `Table::lower(&Flow)`”，不选择
给 cell 伪造默认 `Flow`。

这个 seam 保留三件关键事实：

1. marker style 来自 cell 所在正文 flow，而不是另造的 default style；
2. item 内 image/document handle 检查仍使用同一 `DocToken`；
3. `List::item` 仍通过 `Flow::nested` 和 `ListItem::new`，所以错误容器名仍是
   `"list item"`，不会被改写为 `"table cell"`。

错误时序是延后到 table lower，但仍发生在 `Flow::table` 组装该节点时，早于分页和
绘制。若第一项含 columns，结果仍为：

```rust
Error::InvalidCellContent {
    container: "list item",
    node: "columns",
}
```

## 5. `natural_height` 与分页

段落以 `FlowNode::Text` 进入 cell，第一次 split 才按 cell 的真实 `inner_w`
断行。列表也先按 cell 内宽扣除 18 pt marker column，再排 item body。因此
fill-to-end blank 的终点、link rectangle、marker 和正文 x 都以 cell frame 为准。

表格现有测量不变：

```text
inner_full = page_body_full_height - 2 * inset
frame = { width: inner_w, height: inner_full, full_height: inner_full }
natural_height(CellBody::into_node(), frame)
```

`natural_height` 只接受 `Split::Fit`：

- 富段落或列表完整 fit：得到 cell 自然高，随后仍按整行/rowspan group 分页；
- 在当前页剩余高放不下、但完整行不高于 full frame：整个行组移到下一页；
- cell child 在 full frame 中返回 `Partial` 或 `MoveToNextPage`：
  `Error::UnsplittableTallerThanFrame`；
- 不保存 list item 或 paragraph 的 tail，不允许单元格中途续页。

这与正文列表“item 可跨页”并不矛盾：正文直接交给 paginator；格内列表先被
table 的 fit-only 自然高测量包住。本刀不修改该边界。

## 6. 测试计划

所有正向行为从公共 `document(...).pages()` 进入，坐标与字体通过
`RecordingCanvas` 断言，不检查压缩 PDF bytes。

### 17a

1. `cell_rich_paragraph_uses_body_and_bold_fonts`
   - 注册 body/bold 两个 font id；
   - 一个 cell 中 `text("A")`、`bold("B")`；
   - 断言 glyph run 分别使用 font 0、font 1。
2. `cell_fill_blank_stops_at_cell_edge`
   - 200 pt 页面、两等宽列；
   - 第一格段落写 `"A"` 后 `blank_to_end()`；
   - 断言 stroke 起于 A 末端、止于 `x = 100`，不是页面 `x = 200`。
3. `cell_link_rect_has_cell_offset`
   - link paragraph 放第二列；
   - 断言 link rect 与链接 glyph 同宽同 x，且 x 从第二格 `cell_x = 100` 起算。
4. `cell_paragraph_preserves_builder_error`
   - 空 URI 或 fill blank 后追加 text；
   - 断言原始 `EmptyLink`/`RunAfterFillBlank`，无 `Rendered` 结果。
5. `cell_paragraph_taller_than_body_is_unsplittable`
   - 小页面、多行 cell paragraph；
   - 断言 `UnsplittableTallerThanFrame`，不产生续页 cell。
6. 保留 `Cell::new` 及 `&str`/`String` 行的现有测试，证明纯文本路径未变。

### 17b

1. `cell_ul_marker_and_body_use_cell_coordinates`
   - 200 pt 页面、列表放第二个等宽 cell；
   - 断言 marker `x = 100`、item body `x = 118`。
2. `cell_ol_and_nested_list_reuse_public_list`
   - 覆盖 `Numbering`、`item_marked`、嵌套 ul；
   - 断言标签文本及每层再增加 18 pt 的正文左缘。
3. `cell_list_closure_may_borrow_non_static_data`
   - 列表闭包借用 table 调用外的 `Vec<String>`；
   - 这是编译面回归测试，同时断言实际文本。
4. `invalid_content_inside_cell_list_names_list_item`
   - item 内调用 `columns`（另可覆盖 page break/table）；
   - 精确断言 `InvalidCellContent { container: "list item", node: "columns" }`。
5. `cell_list_taller_than_body_is_unsplittable`
   - 多项或多行 item 超过 full cell frame；
   - 断言 `UnsplittableTallerThanFrame`，而不是 item 跨到第二页。
6. 窄于或等于 18 pt marker column 的 cell 继续断言 `InvalidGeometry`。
7. 既有 layout allowlist 拒绝测试继续覆盖 cell 的 columns、page break、
   keep-together、nested table；公共 `Cell` 不新增这些构造入口。

实现后每个子刀均运行：

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

## 7. 明确不在本刀范围

- 多 payload / `stack-cell`，以及段落与列表混装在同一 cell。
- cell columns、pageBreak、keep_together、nested table。
- 修改 `natural_height`、保存 cell tail 或 `split-in-cell`。
- 新 paragraph/list builder、items-only 富文本入口、具名样式。
- flow canvas、QR、SVG、TOC、加密。
- list marker width/start/nest indent 的新公共配置。
- table padding、border、列宽、rowspan 或重复表头算法改造。
- backend、krilla、字体测量或第二 PDF 后端变更。

## 8. 17a / 17b 拆分

建议拆分并顺序堆叠：

### 17a：cell paragraph

1. `refactor(document): share paragraph lowering`
2. `feat(document): add Cell::paragraph`
3. `test(document): cover rich paragraph cells`

该子刀只涉及 owned data，能独立交付 §2.6 的富段落、bold、blank 和 link。

### 17b：cell list

1. `refactor(document): scope deferred cell content`
2. `feat(document): add deferred Cell::ul and Cell::ol`
3. `test(document): cover list cells and fit-only height`

17b 单独评审 `Cell<'a>` 推断、借用 capture、现有 `From` 实现以及公开 trait 移除。
只有编译夹具和公共渲染测试都通过后才合并。两刀之间不改 layout allowlist，也不把
17b 的生命周期 scaffolding 提前塞入 17a。
