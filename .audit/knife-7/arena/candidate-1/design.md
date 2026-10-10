# 刀 7 候选 1：FlowNode 原生节点 + PagePlan 装饰带

基线 `feat/flow-paginate @ a6ff61e`。只动 crate 内部布局，不写任何公共 DSL builder。
权威顺序：`docs/提案书.md` > `AGENTS.md` > 设计稿未确认部分。

---

## 问题

刀 7 要在现有的"消费式切分"协议（`FlowNode::split(ctx, Frame) -> Split::{Fit, Partial, MoveToNextPage}`）上加 columns、table、list 三种正文节点和页眉页脚。形状不显然，原因有四个：

1. **宽度在错误的时刻定死。** `ParagraphNode::layout(fonts, text, style, width)` 构造时就断好了行，`split` 不看 `avail.width`。columns、表格单元格、列表正文都要求"W = 当前框宽"（提案书 §2.1），如果宽度只靠调用方在构造时自己算对，传错了文字会溢出到框外，而且不报错（how.md Gotcha 1）。
2. **`with_offset` 只平移 y，而且是私有的**；`FragmentCommand` 只有 `Glyphs`，表格边框画不出来。
3. **零进展死循环。** 复合节点在整页框上要么有进展，要么报错。columns 里所有列都放不下、表格"表头 + 首行"放不下，都是容易出问题的地方（Gotcha 3、4）。
4. **页眉页脚不属于正文流。** 带高等于上下 margin，横向占满页宽，不改正文框（提案书 §2.10）。内容依赖总页数，只有在 `plan_pages` 结束后才能求；而且要在 paint 之前量完，保证出错时零输出。

必须保留的不变量：`Frame.height` 可以为 0；Prepared KeepTogether；`spacing_after` 放不下就丢弃；PageCursor 三态；空白正文页也要装饰、也计入页数（§2.9）；绘制顺序 header → body → footer（§2.12）；测试断言 `RecordingCanvas`。

本候选的强制方向：三个新节点都是 `FlowNode` 变体；单元格和列表项的子节点仍是 `Vec<FlowNode>`，构造时用 `InvalidCellContent` 校验；`FragmentCommand` 只加 `Stroke`；页眉页脚在 plan 之后量好，写进 `PagePlan`。

---

## 用法（调用方视角：crate 内测试）

刀 7 没有对外调用方。下面这些测试就是规格，类型签名从这里推出来。所有测试都用 `paginate.rs` 测试模块里已有的 `fonts_and_style` / `config` / `page_runs` / `page_count` 这类 helper，另外加三个小 helper：

```rust
// 测试模块内的 helper
fn text(s: &str, style: &Style) -> FlowNode {
    FlowNode::Text(TextNode::new(s, style.clone()))
}

fn strokes(page: &RecordedPage) -> Vec<&Path> {
    page.ops().iter().filter_map(|op| match op {
        CanvasCommand::StrokePath { path, .. } => Some(path),
        _ => None,
    }).collect()
}

fn run_x(page: &RecordedPage) -> Vec<(String, f32, f32)> { /* (text, x, y)，同 page_runs 加上 x */ }
```

### 1. columns：列宽解出，列 x 偏移，W = 列宽

```rust
#[test]
fn columns_place_each_column_at_its_x() {
    let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
    let cols = ColumnsNode::new(
        NonNegative::pt(12.0).unwrap(),
        vec![
            (ColumnWidth::Fixed(Positive::pt(80.0).unwrap()), vec![text("A", &style)]),
            (ColumnWidth::Auto, vec![text("X", &style)]),
        ],
    )
    .unwrap();
    let mut ctx = LayoutCtx { fonts: &mut fonts };
    let mut rec = RecordingCanvas::new();
    paginate_root(
        FlowNode::Columns(cols),
        &mut ctx,
        config(Size::try_pt(200.0, 400.0).unwrap()),
        &Decorations::none(),
        &mut rec,
    )
    .unwrap();
    let runs = run_x(&rec.pages()[0]);
    assert_eq!(runs[0].1.to_bits(), 0.0f32.to_bits());          // "A" 在第 0 列
    assert_eq!(runs[1].1.to_bits(), (80.0f32 + 12.0).to_bits()); // "X" 在第 1 列 x = 80 + gap
    assert_eq!(runs[0].2.to_bits(), runs[1].2.to_bits());        // 顶对齐，基线相同
}
```

### 2. columns 跨页：各列在剩余高度上切分，tail 是新的 Columns

```rust
#[test]
fn columns_split_across_pages_per_column() {
    // 页高 = 2 行。左列 3 行，右列 1 行。
    // 第 1 页：左 A,B 右 X；第 2 页：左 C 在顶部，右列为空。
    let cols = ColumnsNode::new(gap12(), vec![
        (ColumnWidth::Ratio(Ratio::new(0.5).unwrap()), vec![text("A\nB\nC", &style)]),
        (ColumnWidth::Auto, vec![text("X", &style)]),
    ]).unwrap();
    // ... paginate_root ...
    let pages = rec.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(page_texts(&pages[0]), ["A", "B", "X"]);
    assert_eq!(page_runs(&pages[1]), [("C".into(), metrics.baseline)]);
}

#[test]
fn columns_all_columns_stuck_on_full_page_errors() {
    // 某列首个 keep_together 高于整页正文框 → UnsplittableTallerThanFrame，且 rec 为空
}

#[test]
fn columns_width_overflow_is_column_widths_error() {
    // Fixed(150) + gap 12 + Fixed(150) 放在 body_w = 200 → Error::ColumnWidths
}

#[test]
fn pre_laid_paragraph_wider_than_column_errors() {
    // 用 ParagraphNode::layout(.., 200.0) 预排后塞进 80pt 的列 → Error::InvalidGeometry（不静默溢出）
}
```

### 3. table：表头放在 tail 上重复、rowSpan 块原子、边框

```rust
#[test]
fn table_repeats_all_header_rows_on_continuation_page() {
    let mut spec = TableSpec::new(vec![ColumnWidth::Ratio(Ratio::new(0.5).unwrap()), ColumnWidth::Auto]);
    spec.header = vec![
        vec![Cell::new(vec![text("H1", &style)]).with_span(2, 1).unwrap()],
        vec![Cell::new(vec![text("a", &style)]), Cell::new(vec![text("b", &style)])],
    ];
    spec.body = (0..4)
        .map(|i| vec![Cell::new(vec![text(&format!("r{i}"), &style)]), Cell::new(vec![text("1", &style)])])
        .collect();
    let table = TableNode::new(spec).unwrap();
    // 页高 = 4 行：表头 2 行 + 正文 2 行。
    // ... paginate_root(FlowNode::Table(table), ..) ...
    let pages = rec.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(page_texts(&pages[0]), ["H1", "a", "b", "r0", "1", "r1", "1"]);
    assert_eq!(page_texts(&pages[1]), ["H1", "a", "b", "r2", "1", "r3", "1"]);
    assert_eq!(page_runs(&pages[1])[0].1.to_bits(), metrics.baseline.to_bits()); // 续页表头贴顶
}

#[test]
fn table_repeat_header_off_continues_body_only() { /* spec.repeat_header = false */ }

#[test]
fn table_row_span_block_moves_together() {
    // r1 第 0 列 rowSpan 2 → r1+r2 是一个原子块。本页剩 2 行高时 r0 放下，
    // 剩 1 行不够这个 2 行块 → 块整体去下一页（第 1 页只有表头 + r0）。
}

#[test]
fn table_header_plus_block_taller_than_page_errors() {
    // 表头高 + 任一正文块高 > full_height → UnsplittableTallerThanFrame；rec 为空
}

#[test]
fn table_cell_rejects_columns() {
    let mut spec = TableSpec::new(vec![ColumnWidth::Auto]);
    spec.body = vec![vec![Cell::new(vec![FlowNode::Columns(two_cols())])]];
    let err = TableNode::new(spec).unwrap_err();
    assert!(matches!(err, Error::InvalidCellContent { container: "table cell", node: "columns" }));
}
// 同样地：PageBreak、KeepTogether、嵌套 Table → InvalidCellContent

#[test]
fn table_overlapping_spans_rejected() {
    // 第 0 行 rowSpan 2 占住 (1,0)，第 1 行又给出 2 个单元格 → Error::InvalidCellSpan
}

#[test]
fn table_borders_stroke_each_cell_rect_centered_on_grid() {
    // border = 2pt，padding = 4pt，2x2 表格放在 body 原点 (10, 15)
    let rects = strokes(&rec.pages()[0]);
    assert_eq!(rects.len(), 4);
    // 第一个单元格：闭合矩形 (10,15)-(10+w0,15)-(10+w0,15+h0)-(10,15+h0)
    // 文字 x = 10 + border/2 + padding = 10 + 1 + 4
}
```

### 4. list：悬挂缩进、标记只在首个片段、自定义串、罗马数字

```rust
#[test]
fn list_hanging_indent_and_marker_on_first_fragment_only() {
    let list = ListNode::new(ListSpec {
        kind: MarkerKind::Decimal,
        start: NonZeroU32::MIN,
        marker_width: Positive::pt(20.0).unwrap(),
        nest_indent: None, // 默认 = marker_width
        marker_style: style.clone(),
        items: vec![
            ListItem::new(vec![text("A\nB\nC", &style)]), // 3 行，页高 2 行
            ListItem::new(vec![text("D", &style)]),
        ],
    })
    .unwrap();
    // ...
    let p0 = run_x(&pages[0]);
    assert_eq!(p0[0], ("1.".into(), 0.0, baseline));        // 标记在 x = 0
    assert_eq!((p0[1].0.as_str(), p0[1].1), ("A", 20.0));    // 正文列 x = 20
    assert_eq!((p0[2].0.as_str(), p0[2].1), ("B", 20.0));    // 续行不顶回标记下方
    let p1 = run_x(&pages[1]);
    assert_eq!((p1[0].0.as_str(), p1[0].1), ("C", 20.0));    // 续页：没有标记，缩进不变
    assert_eq!((p1[1].0.as_str(), p1[1].1), ("2.", 0.0));
}

#[test]
fn list_nested_indents_by_nest_indent() {
    // 外层 marker_width 20，内层 marker_width 16：内层标记 x = 20，内层正文 x = 36
}

#[test]
fn list_upper_roman_from_start() {
    // kind UpperRoman, start 4 → 标记 "IV.", "V."
}

#[test]
fn list_custom_marker_too_wide_errors() {
    let item = ListItem::custom("(1000)", vec![text("A", &style)]);
    // marker_width 10pt → Error::UnbreakableWiderThanFrame { width, avail: 10.0 }
}

#[test]
fn list_item_rejects_table() { /* → InvalidCellContent { container: "list item", node: "table" } */ }
```

### 5. 装饰：全页宽带、全文页码、空白页、绘制顺序、出错零输出

```rust
#[test]
fn header_footer_bands_use_full_page_width_and_document_page_numbers() {
    let hs = style.clone();
    let fs = style.clone();
    let decorations = Decorations {
        header: Some(Box::new(move |page: &PageInfo| {
            Ok(vec![text(&format!("Page {} / {}", page.number.get(), page.count.get()), &hs)])
        })),
        footer: Some(Box::new(move |_| Ok(vec![text("F", &fs)]))),
    };
    let config = PaginateConfig {
        page_size: Size::try_pt(200.0, 400.0).unwrap(),
        margins: Margins::new(pt(20.0), pt(30.0), pt(20.0), pt(30.0)),
    };
    // 正文：A, PageBreak, PageBreak, B → 3 页，中间是空白正文页
    paginate_root(body, &mut ctx, config, &decorations, &mut rec).unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 3);
    assert_eq!(page_texts(&pages[1]), ["Page 2 / 3", "F"]);    // 空白页也装饰
    let p0 = run_x(&pages[0]);
    assert_eq!(p0[0].0, "Page 1 / 3");
    assert_eq!(p0[0].1.to_bits(), 0.0f32.to_bits());            // 页眉从 x = 0 起，不吃 margin.left
    assert_eq!(p0[1].0, "A");                                    // 顺序：header → body → footer
    assert_eq!(p0[2].0, "F");
    assert_eq!(p0[2].2.to_bits(), (380.0 + metrics.baseline).to_bits()); // 页脚带顶 = page_h − mb
}

#[test]
fn header_with_zero_margin_errors_before_any_paint() {
    // margins 默认 0 + header 有内容 → DecorationTallerThanMargin；rec.commands().is_empty()
}

#[test]
fn header_taller_than_band_errors() { /* 2 行文字，margin.top = 1 行高 → DecorationTallerThanMargin */ }

#[test]
fn header_wider_than_page_errors() { /* 一个不可断的长串 → DecorationWiderThanFrame { width, avail: page_w } */ }
```

现有 14 个 `paginate_root(...)` 测试调用机械地补一个参数 `&Decorations::none()`。

---

## 形状

### 模块图

```text
src/
  geom.rs              + Path::translated(dx, dy)               pub(crate)
  error.rs             + ColumnWidths, InvalidCellContent{..}, InvalidCellSpan,
                         DecorationTallerThanMargin, DecorationWiderThanFrame{..}
  layout/
    mod.rs             + pub mod columns; pub mod list;
    flow.rs            FlowNode + {Text, Columns, Table, List}
                       FragmentCommand + Stroke
                       Fragment::with_offset(dx, dy)  pub(crate)
                       PlacedFragment（从 paginate.rs 移来，pub(crate)）
                       draw_fragment 处理 Stroke
    paragraph.rs       + TextNode（split 时按 avail.width 惰性 layout）
                       ParagraphNode + width 字段 + 宽度守卫
                       + single_line_run（列表标记用）
    columns.rs   新    ColumnWidth / Ratio / ColumnTracks / resolve_column_widths
                       ColumnsNode
    table.rs           TableSpec / Cell / Border / TableNode(Open|Measured)
    list.rs      新    MarkerKind / ListSpec / ListItem / ListNode
    lower.rs           Container + check_content（唯一一份内容规则矩阵）
    decorate.rs        PageNumber / PageCount / PageInfo / Decorations / PageBands
    paginate.rs        PagePlan + header/footer；paginate_root(.., &Decorations, ..)
                       decorate_pages；paint 顺序 header → body → footer
    dump.rs            不动
```

依赖方向不变：`columns / table / list / decorate → flow → (paragraph, canvas, geom)`；`paginate → flow + decorate`。不引入 krilla。

### 决定性设计 1：`FlowNode::Text`，宽度由 Frame 带进来

```rust
// paragraph.rs
/// Unwrapped text. Wraps at the width of the frame it is first split against.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextNode {
    text: String,
    style: Style,
}

impl TextNode {
    pub(crate) fn new(text: impl Into<String>, style: Style) -> Self { todo!() }

    /// Lays out at `avail.width`, then splits; tails are `FlowNode::Paragraph`.
    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split> {
        // ParagraphNode::layout(ctx.fonts, &self.text, &self.style, avail.width)?.split(avail)
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParagraphNode {
    pub(crate) lines: Vec<LineBox>,
    pub(crate) line_height: f32,
    pub(crate) spacing_after: f32,
    width: f32, // layout width; split rejects frames narrower than this
}

impl ParagraphNode {
    pub(crate) fn split(self, avail: Frame) -> Result<Split> {
        // 新增首行：if self.width > avail.width { return Err(Error::InvalidGeometry) }
        // 其余不变
        todo!()
    }
}

/// One unwrapped line of `text` at origin (0, baseline). Errors when wider than `max_width`.
pub(crate) fn single_line_run(
    fonts: &mut FontStore,
    text: &str,
    style: &Style,
    max_width: f32,
) -> Result<GlyphRun> {
    todo!() // advance > max_width → Error::UnbreakableWiderThanFrame { width, avail: max_width }
}
```

这样列宽、格内宽、列表正文宽、页眉带宽都只是 `Frame.width`。提案书 §2.1 的"W = 当前框"成了协议本身的性质，不再靠每个构造点各自算对（per boundary-discipline）。`Paragraph` 仍然是已排好的续排形态，和 `KeepTogether::Open → Prepared` 是同一个套路。宽度守卫把"预排段落塞进更窄的框"这种静默溢出变成报错。现有测试按 body 宽或更窄预排，守卫不会让它们失败。

### 决定性设计 2：协议层改动只做边框需要的部分

```rust
// flow.rs
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FlowNode {
    Paragraph(ParagraphNode),
    Text(TextNode),
    Spacer(SpacerNode),
    PageBreak,
    KeepTogether(KeepTogetherNode),
    Stack(Vec<FlowNode>),
    Columns(ColumnsNode),
    Table(TableNode),
    List(ListNode),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FragmentCommand {
    Glyphs(GlyphRun),
    Stroke { path: Path, paint: Paint, stroke: Stroke },
}

impl Fragment {
    /// Moves every command by `(dx, dy)`. Path points move too.
    pub(crate) fn with_offset(self, dx: f32, dy: f32) -> Result<Self> { todo!() }
}

/// A fragment pinned to page coordinates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlacedFragment {
    pub(crate) origin: Point,
    pub(crate) fragment: Fragment,
}

// geom.rs
impl Path {
    pub(crate) fn translated(&self, dx: f32, dy: f32) -> Result<Path> { todo!() }
}
```

`split_stack` 改成调用 `with_offset(0.0, y)`。`draw_fragment` 的 `Stroke` 分支调用 `canvas.stroke_path(&path.translated(ox, oy)?, &paint, &stroke)`。不加 `Fill`，因为提案书没有单元格底色（per subtract-before-you-add）。`FragmentCommand` 上的 match 是穷尽的，加变体时编译器会逼着 `with_offset` 和 `draw_fragment` 同步改。

### 决定性设计 3：内容规则只有一份矩阵，用穷尽 match 写

```rust
// lower.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Container {
    Column,
    TableCell,
    ListItem,
    Band,
}

/// Rejects nodes `container` may not hold, recursing into `Stack` and `List`.
pub(crate) fn check_content(container: Container, nodes: &[FlowNode]) -> Result<()> {
    todo!() // 违规 → Error::InvalidCellContent { container: container.name(), node: node_name(n) }
}
```

| 节点 \ 容器 | Column | TableCell | ListItem | Band |
| --- | --- | --- | --- | --- |
| Text / Paragraph / Spacer | ✓ | ✓ | ✓ | ✓ |
| Stack | ✓（递归） | ✓（递归） | ✓（递归） | ✓（递归） |
| List | ✓ | ✓（§2.6 随列表放开） | ✓（嵌套） | ✓ |
| KeepTogether | ✓（§2.5） | ✗ | ✗（不在 §2.8 允许清单） | ✓ |
| Columns | ✓（§2.4 互嵌） | ✗ | ✗ | ✓ |
| Table | ✓（§2.6） | ✗（先禁止嵌套） | ✗ | ✓ |
| PageBreak | ✗ | ✗ | ✗ | ✗ |

`ColumnsNode::new`、`TableNode::new`、`ListNode::new` 以及装饰带量取都调用 `check_content`。规则由 `match (container, node)` 穷尽写出，没有通配分支，所以加 FlowNode 变体时编译会失败，直到这张表补上新的一行（per hand-synced list 红旗）。嵌套位置的 pageBreak 在这里就报出 `InvalidCellContent`，不会走到 split 里那个泛化的 `InvalidGeometry`。

### Columns

```rust
// columns.rs
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ColumnWidth {
    Fixed(Length<Positive>),
    Ratio(Ratio),
    Auto,
}

/// Share of the content width after gaps, in (0, 1].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ratio(f32);

impl Ratio {
    pub(crate) fn new(value: f32) -> Result<Self> { todo!() } // 不在 (0,1] → ColumnWidths
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ColumnTracks {
    pub(crate) xs: Vec<f32>,
    pub(crate) widths: Vec<f32>,
}

/// Subtracts gaps first, then Fixed, then Ratio of the remaining content width; Auto takes the rest.
/// Without Auto the leftover stays empty on the right.
pub(crate) fn resolve_column_widths(
    avail_width: f32,
    gap: f32,
    specs: &[ColumnWidth],
) -> Result<ColumnTracks> { todo!() } // 超宽、Auto ≤ 0、任一列 ≤ 0 → ColumnWidths

pub(crate) fn default_column_gap() -> Length<NonNegative> { todo!() } // 12pt，§2.5

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ColumnsNode {
    widths: Vec<ColumnWidth>,
    gap: Length<NonNegative>,
    columns: Vec<FlowNode>, // 每列一个 FlowNode::Stack
}

impl ColumnsNode {
    /// Rejects empty input, more than one Auto, and content `check_content(Column, ..)` refuses.
    pub(crate) fn new(
        gap: Length<NonNegative>,
        columns: Vec<(ColumnWidth, Vec<FlowNode>)>,
    ) -> Result<Self> { todo!() }

    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split> { todo!() }
}
```

`split` 的伪代码：

```text
tracks = resolve_column_widths(avail.width, gap, widths)        // 每次 split 都解，无缓存
for k: out[k] = columns[k].split(ctx, Frame{tracks.widths[k], avail.height, avail.full_height})
progress = 存在 Fit{h>0} 或 Partial{head_height>0}
if 全部 Fit:
    Fit{ height = max h_k, fragment = ∪ frag_k.with_offset(xs[k], 0),
         spacing_after = max frag_k.spacing_after }               // Gotcha 6：取最大值，有测试固定
elif !progress:
    MoveToNextPage(Columns{ 每列：Move→原节点，Partial(0)→tail，Fit(0)→Stack[] })
else:
    Partial{ head_height = max 已放高度, head = ∪ 已放部分.with_offset(xs[k], 0), spacing_after = 0,
             tail = Columns{ 同样宽度和 gap；Fit→Stack[]，Partial→tail，Move→原节点 } }
```

终止性：在整页框上，"有列卡住且没有任何进展"会返回 `MoveToNextPage`，分页器在 `y == 0` 时报 `UnsplittableTallerThanFrame`；只要有进展就是 `Partial`，下一页继续消耗。不会出现零进展的 `Partial`。

### Table

```rust
// table.rs
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TableSpec {
    pub(crate) widths: Vec<ColumnWidth>,
    pub(crate) header: Vec<Vec<Cell>>,
    pub(crate) body: Vec<Vec<Cell>>,
    pub(crate) repeat_header: bool,   // new() 里为 true
    pub(crate) padding: Length<NonNegative>,
    pub(crate) border: Border,
}

impl TableSpec {
    pub(crate) fn new(widths: Vec<ColumnWidth>) -> Self { todo!() }
}

/// Grid-line stroke. Width zero draws nothing (§2.6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Border {
    pub(crate) width: Length<NonNegative>,
    pub(crate) color: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cell {
    content: Vec<FlowNode>,
    col_span: NonZeroU16,
    row_span: NonZeroU16,
}

impl Cell {
    pub(crate) fn new(content: Vec<FlowNode>) -> Self { todo!() }
    pub(crate) fn with_span(self, cols: u16, rows: u16) -> Result<Self> { todo!() } // 0 → InvalidCellSpan
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TableNode(TableState);

#[derive(Debug, Clone, PartialEq)]
enum TableState {
    Open(GridTable),
    Measured(MeasuredTable),
}

/// Spans placed on the grid; body rows grouped into blocks no page seam may cut.
#[derive(Debug, Clone, PartialEq)]
struct GridTable {
    widths: Vec<ColumnWidth>,
    padding: f32,
    border: Border,
    header: Option<RowBlock>,
    body: Vec<RowBlock>,
    repeat_header: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct RowBlock {
    rows: usize,
    cells: Vec<PlacedCell>,
}

#[derive(Debug, Clone, PartialEq)]
struct PlacedCell {
    col: usize,
    row: usize, // 块内相对行号
    col_span: usize,
    row_span: usize,
    content: Vec<FlowNode>,
}

#[derive(Debug, Clone, PartialEq)]
struct MeasuredTable {
    width: f32,                       // 测量时的 avail.width；之后不一致 → InvalidGeometry
    header: Option<MeasuredBlock>,
    body: VecDeque<MeasuredBlock>,
    repeat_header: bool,
    header_pending: bool,             // 本次 split 是否先画表头
}

#[derive(Debug, Clone, PartialEq)]
struct MeasuredBlock {
    height: f32,
    fragment: Fragment,               // 内容和边框都已在块内定位
}

impl TableNode {
    /// Places spans (overlap, overflow, uncovered slot, header rowSpan into body → InvalidCellSpan),
    /// checks every cell with `check_content(TableCell, ..)`, groups body rows into rowSpan blocks.
    pub(crate) fn new(spec: TableSpec) -> Result<Self> { todo!() }

    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split> { todo!() }
}

fn measure(grid: GridTable, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<MeasuredTable> { todo!() }
```

测量（第一次 split 时，`Open → Measured`，之后不再重新测）：

```text
tracks = resolve_column_widths(avail.width, 0, widths)
inset = border/2 + padding（每侧）
for 每个单元格：inner_w = 跨列宽之和 − 2·inset（≤ 0 → ColumnWidths）
    (h, frag) = natural_height(Stack(content), Frame{inner_w, full−2·inset, full−2·inset})
    cell_h = h + 2·inset；格内最后一个子节点的 spacing_after 丢弃
row_h：先取 row_span = 1 的单元格高度的最大值；再按 row_span 从小到大处理跨行单元格，不够的差额补到其最后一行
块 fragment：内容 with_offset(x_col + inset, y_row + inset)；border > 0 时每格输出一个闭合矩形 Stroke
校验：任一 (repeat ? header_h : 0) + block_h > full_height → UnsplittableTallerThanFrame
```

切分（`Measured`）：

```text
y = 0
if header_pending && header:
    if header_h + body[0].h > avail.height: return MoveToNextPage(self)     // 不留孤立表头
    放表头；y = header_h
逐块：y + block.h ≤ avail.height 就放下
没有放下任何正文块 → MoveToNextPage(self)（第一块都放不下）
全部放完 → Fit{y, spacing_after: 0}
否则 → Partial{head = 已放部分, tail = Table(Measured{剩余块, header_pending: repeat_header})}
```

表头放在 **tail** 上：上一页的 head 不追加任何内容，续页的第一次 split 先画表头。死循环防线是测量时的那条全局校验，再加上分页器的 `y == 0` 规则。

### List

```rust
// list.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerKind {
    Bullet,      // "•"
    Decimal,     // "1."
    UpperAlpha,  // "A."
    LowerAlpha,  // "a."
    UpperRoman,  // "I."
    LowerRoman,  // "i."
}

impl MarkerKind {
    fn label(self, number: NonZeroU32) -> String { todo!() }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListSpec {
    pub(crate) kind: MarkerKind,
    pub(crate) start: NonZeroU32,
    pub(crate) marker_width: Length<Positive>,
    pub(crate) nest_indent: Option<Length<Positive>>, // None = marker_width；必须 ≥ marker_width
    pub(crate) marker_style: Style,
    pub(crate) items: Vec<ListItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListItem {
    marker: Option<String>, // Some = 该项自定义标记，不走枚举算法
    content: Vec<FlowNode>,
}

impl ListItem {
    pub(crate) fn new(content: Vec<FlowNode>) -> Self { todo!() }
    pub(crate) fn custom(marker: impl Into<String>, content: Vec<FlowNode>) -> Self { todo!() }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListNode {
    kind: MarkerKind,
    marker_width: f32,
    indent: f32,
    marker_style: Style,
    items: VecDeque<(NonZeroU32, ListItem)>, // 序号 = start + 位置，在 new() 里定好
    head_started: bool,                       // 队首项是否已在前面的页画过标记
}

impl ListNode {
    /// Checks each item with `check_content(ListItem, ..)` and `nest_indent ≥ marker_width`.
    pub(crate) fn new(spec: ListSpec) -> Result<Self> { todo!() }

    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split> { todo!() }
}
```

切分的结构与 `split_stack` 相同，只是以"项"为单位：

```text
body_w = avail.width − indent（≤ 0 或 marker_width ≥ avail.width → InvalidGeometry）
for 每项：Stack(content).split(Frame{body_w, remain, full})
    放下了内容（Fit 或 Partial）且 !head_started：
        run = single_line_run(fonts, label, marker_style, marker_width)   // 过宽 → UnbreakableWiderThanFrame
        run.origin.y = 该项 head fragment 中第一个 Glyphs 的基线（没有字形时用标记自己的基线）
        输出 Glyphs(run) 于 (0, y)
    正文 with_offset(indent, y)
    项间 spacing 规则同 Stack；Partial → tail = List{该项 tail 在队首, head_started: true}
    MoveToNextPage 且尚未放过任何内容 → MoveToNextPage(self)
```

嵌套列表就是项内容里的 `FlowNode::List`。它在父项的正文列里排，自然多缩进一级 `indent`（默认 = `marker_width`，§2.8）。各层编号互相独立，不做自动的 `1.1.1`。

### 装饰

```rust
// decorate.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageNumber(NonZeroU32);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageCount(NonZeroU32);
// 都有 get() -> u32；构造方法 from_index(usize) / from_len(usize) -> Result

/// What a band closure sees. Section counters arrive in knife 8.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PageInfo {
    pub(crate) number: PageNumber,
    pub(crate) count: PageCount,
    pub(crate) size: Size,
    pub(crate) margins: Margins,
}

pub(crate) type BandContent = Box<dyn Fn(&PageInfo) -> Result<Vec<FlowNode>>>;

#[derive(Default)]
pub(crate) struct Decorations {
    pub(crate) header: Option<BandContent>,
    pub(crate) footer: Option<BandContent>,
}

pub(crate) struct PageBands {
    pub(crate) header: Option<PlacedFragment>,
    pub(crate) footer: Option<PlacedFragment>,
}

impl Decorations {
    pub(crate) fn none() -> Self { todo!() }

    /// Header band: origin (0, 0), frame page_w × margin.top.
    /// Footer band: origin (0, page_h − margin.bottom), frame page_w × margin.bottom.
    pub(crate) fn bands(&self, ctx: &mut LayoutCtx<'_>, page: &PageInfo) -> Result<PageBands> { todo!() }
}

/// Empty content → None. Non-empty content with band height 0, non-Fit, or UnsplittableTallerThanFrame
/// → DecorationTallerThanMargin. UnbreakableWiderThanFrame → DecorationWiderThanFrame.
fn measure_band(
    ctx: &mut LayoutCtx<'_>,
    nodes: Vec<FlowNode>,
    width: f32,
    height: f32,
) -> Result<Option<Fragment>> { todo!() }
```

```rust
// paginate.rs
#[derive(Debug, Clone, PartialEq)]
struct PagePlan {
    size: Size,
    placed: Vec<PlacedFragment>,
    header: Option<PlacedFragment>, // 只由 decorate_pages 写入
    footer: Option<PlacedFragment>,
}

pub(crate) fn paginate_root(
    root: FlowNode,
    ctx: &mut LayoutCtx<'_>,
    config: PaginateConfig,
    decorations: &Decorations,
    canvas: &mut dyn Canvas,
) -> Result<()> {
    // let mut pages = plan_pages(root, ctx, config)?;
    // decorate_pages(&mut pages, ctx, config, decorations)?;   // 全部量完、校验完才 paint
    // paint_pages(&pages, canvas)
    todo!()
}

fn decorate_pages(
    pages: &mut [PagePlan],
    ctx: &mut LayoutCtx<'_>,
    config: PaginateConfig,
    decorations: &Decorations,
) -> Result<()> { todo!() } // 遍历全部页（包括 placed 为空的页），count = pages.len()

// paint_pages: start_page → header → placed… → footer → finish_page
```

带的几何（原点、宽 = 页宽、高 = margin）只由 `decorate.rs` 决定，算好的结果放进 `PlacedFragment`。`paint_pages` 不知道 margin，也不知道带是什么（per information-hiding）。`plan_pages` 一行都不改，只是新页的 header/footer 初始化为 `None`，正文断点因此保证不变。

### 错误（`error.rs`，公共 `Error` 上 `#[non_exhaustive]` 新增）

```rust
/// Column widths overflow the frame, leave a column at or below zero, or use more than one Auto.
ColumnWidths,
/// `node` is not allowed inside `container`.
InvalidCellContent { container: &'static str, node: &'static str },
/// Table spans overlap, overflow the grid, leave a slot uncovered, or cross header into body.
InvalidCellSpan,
/// Header or footer content is taller than its margin band.
DecorationTallerThanMargin,
/// Header or footer content is wider than the page.
DecorationWiderThanFrame { width: f32, avail: f32 },
```

### 接口深度

每个新节点对外只有两个入口：`new(spec) -> Result<Self>` 和 `split(self, ctx, Frame)`。宽度解算、grid 占位、rowSpan 分块、行高分配、边框几何、表头重复状态、标记编号和基线对齐，都藏在 `split` 和 `measure` 里面。调用方（刀 8 的 lower）只做两件事：把 DSL 映射成 spec，以及用 `Text` 交出文字，不需要知道任何宽度。装饰对 paginate 暴露一个 `bands(ctx, &PageInfo)`。这套接口不能再小了：再小就得把 spec 字段打散成 builder，那是刀 8 的事。

### 有意不做

不加 `Fill`、不支持栏间竖线、不做单元格垂直对齐（只顶对齐）、不做表级 `spacing_after`、不做 section 页码、不做 behind/front、不加分隔线节点（§2.3 单独一刀）、不改 `dump.rs`、不碰 `limits.rs`，也不加公共 DSL。

---

## 综合决定

由 arena 填写。

---

## 接受的取舍

- 我们接受 `Text`（未排）和 `Paragraph`（已排）两个变体并存，换来宽度由 Frame 自然带入。`Paragraph` 只作为续排形态和现有测试的入口，宽度守卫保证误用会报错而不是静默溢出。
- 我们接受每个单元格各画一个闭合矩形、共享边被描两次，换来跨页、rowSpan、colSpan 下边框几何都不需要特判。像素结果相同，只是 `StrokePath` 记录更多。
- 我们接受表格在第一次 split 时一次测完全部行（O(行数) 的前期开销），换来后续每页 split 只是按块高累加，死循环校验也能一次做全。
- 我们接受 columns 每次 split 都重新解列宽而不缓存，换来不需要额外状态，也不会出现缓存失效的问题。
- 我们接受 columns 跨页时各列"各自断开"：短列可能本页就结束，长列续到下一页。这正是提案书 §2.5 的语义；想粘在一页的用 keep_together。
- 我们接受 `paginate_root` 加一个参数，代价是机械地改 14 个测试调用点，换来 config（几何）与 decorations（内容闭包）分开，并且 `PaginateConfig` 能继续是 `Copy`。
- 我们接受新增 `columns.rs`、`list.rs` 两个文件（AGENTS.md 的目录树要补一行），换来 `flow.rs` 只承载协议本身。

---

## 考虑过的备选

- **构造时传宽度（闭包拿到解出的列宽或格宽再预排段落）。** 每个构造点都要知道宽度，接口变浅，宽度这个知识在 columns、table、list、decorate 四处泄漏；agent 照着最近的例子用错宽度预排，编译能过，文字溢出也不报错。被 `Text` + 宽度守卫取代。
- **装饰作为 paint 阶段的旁路 `Vec<PageDecoration>`（how.md 的建议）。** 两张按页下标对齐的平行列表，属于手工同步的列表；挂在 `PagePlan` 上，每页的状态就只有一个持有者。
- **装饰做成正文里的伪节点，或者收缩正文框。** 违反 §2.10（带高 = margin，不改正文）。而且页码要等 plan 完成才知道，放进流里就得两遍分页。
- **表格每页重新测行高（不分 `Open/Measured`）。** 每页都重复一遍 natural_height，死循环校验也只能逐页局部做。缓存的套路仓库里已经有（`KeepTogether::Prepared`）。
- **collapsed border：在 grid 上按线段画边框，跳过合并区内部。** 绘制指令更少，但跨页的上下边和合并区的线段裁剪都要特判，复杂度留在实现里，却换不来更小的接口。
- **列表复用 `split_stack`，新增一个内部变体 `FlowNode::ListItem`。** 少写约 60 行，但多出一个可以在任何地方构造的变体，agent 会把它塞进别的容器，内容规则表也要多一行。
- **表头加在 head 上（在上一页结尾追加）。** 不符合重复表头的语义，强制方向也指定放在 tail 上。
- **现在就加 `FragmentCommand::Fill`。** 刀 7 没有使用者（per laziness rubric）。

---

## 未决问题与风险

- 非法合并要不要用一个新的错误名 `InvalidCellSpan`？frame 只列了 4 个错误，提案书要求"非法合并报错"，但没给名字。
- 表格要不要求每一行都被完全覆盖（没有空槽）？本稿要求全覆盖，否则报 `InvalidCellSpan`，pdfmake 也是这样；HTML 允许空槽。
- 有序列表标记的后缀是否固定为 `.`（`1.` / `A.` / `IV.`）？标记在标记列内是左对齐（本稿）还是右对齐（`9.` 和 `10.` 的点能对齐）？
- 自定义标记传空串时，是当作"空白标记"（本稿）还是构造时报错（更符合 §2.0 的精神）？
- 列表项里的 `keep_together`：§2.8 的允许清单里没有它，本稿禁止。是否应该放开？
- 装饰内容在带内的垂直对齐：本稿是顶对齐，页脚要贴底就由调用方自己加 spacer。要不要加 Start/End？
- 装饰带里出现 pageBreak 时报 `InvalidCellContent { container: "header/footer band" }`，这个错误名在带里语义有点勉强，能接受吗？
- columns 在本页"短列已结束、长列卡住"的情况下会返回 `Partial`，续页上长列从顶部开始，左右不再对齐。这符合 §2.5，但是否需要一个夹具来展示给用户确认？
- `Ratio` 定义为"扣掉 gap 之后内容宽的比例"，取值 (0,1]，而不是 pdfmake 的 `*` 权重。这个语义需要确认。
- 风险：`TextNode` 每次构造都持有一份 `String` 和 `Style` 的拷贝，大表格的内存会翻倍，直到测量完成。限额 PR 应把单元格数纳入。

---

## 下一步实现

先做 `geom::Path::translated`、`FragmentCommand::Stroke`、二维 `with_offset`、`draw_fragment` 的 Stroke 分支，以及 `TextNode` 加宽度守卫，并配上 `RecordingCanvas` 测试；columns、table、list、decorate 都在这一层上推进。
