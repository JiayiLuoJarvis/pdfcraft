# pdfcraft-pdf §12.7 公共值形状重构实现规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-03 |
| 状态 | **产品条已定稿**（提案书 §12.7，2026-11-03 逐元素确认）。开工前确认项已清零（§9） |
| 范围 | 只改 `pdfcraft-pdf` 的公共值形状、lower，以及为 Anchor 多孩子/PageBreak 与 HLine 去间距必需的 layout 内部。forms / orgchart / post 不动 |
| 版本 | 全部 PR 合入后发 `pdfcraft-pdf 0.6.0`（当前 `0.5.0`） |
| 相关 | [`提案书.md`](提案书.md) §2.3、§2.4、§2.6、§2.10、§2.12、§2.14、§4.1、§5、§8、§12.1、§12.2、§12.4、§12.5、§12.6、§12.7；[`rust-engineering-standard.md`](rust-engineering-standard.md) API-7、API-9、VER-1 至 VER-4、TOOL-5；[`module-dependency.md`](module-dependency.md) |
| 冲突 | 与提案书冲突时以提案书为准。实现中发现与代码事实冲突，先停下来改提案书或本文，不要自行裁决 |

本文是实现合同，不重开产品讨论。行号与文件以 2026-11-03 工作区（HEAD `cb61019`）为准，动手前重新核对。

## 1. 产品一句话

每种元素的字段按同一规则分成 `style`、描述性字段和孩子，调用方可以把一份 `ListStyle`、`TableStyle`、`ParagraphStyle` 定义一次，到处 `clone()` 复用。公共面仍是一棵纯值声明树；调用方可用普通 Rust 函数组合并返回 `Block` / `Span`。不引入组件实例、状态、key、生命周期、重组或 `Widget` / `Composable` trait。能力基本不变，只有 §5 三处行为变化。

## 2. 规则

| 类别 | 判据 | 例子 |
| --- | --- | --- |
| `style` | 可复用、顺序无关的外观或版式参数。类型名 `XxxStyle`，`Clone` + `Default` | `ParagraphStyle`、`ListStyle`、`TableStyle`、`CellStyle`、`InsetStyle`、`ImageStyle`、`HLineStyle`、`SectionStyle`、`BandStyle`、`RunStyle` |
| 描述性字段 | 这个节点是谁、在结构里占什么位置，每个实例各不相同 | `Anchor.id` / `outline`、`col_span` / `row_span`、`ListItemSpec.marker`、`TableHeader.repeat`、`CanvasSpec.size`、`ImageSpec.image`、`LayerMark.when` |
| 组合内容 | 通用的多个孩子叫 `children`，单个叫 `child`。好几组孩子时用具名槽位；领域集合保留语义名 | `TableSpec.header` / `body`；`SectionSpec.header` / `footer` / `behind` / `front` / `body`；`TableHeader.rows`；`CanvasSpec.marks`；`Mark::Rotated.shapes` |

- 元素有可复用的样式参数才带 `style`，纯描述性元素不带，不建空 `style`。
- `Styled`、`Inset`、`Anchor`、`KeepTogether` 是按树嵌套顺序生效的结构包装，不收进 `style`；不引入统一 `Modifier` 链。
- `XxxStyle` 不标 `#[non_exhaustive]`：标了以后 crate 外连 `..Default::default()` 的字面量都写不了。首个实现 PR 必须同步在工程规范 API-9 与 Google disposition 登记「公有字段 + `Default` + 字面量」样式结构体的统一例外；0.x 后续加字段按 VER-1/VER-2 breaking 处理。
- 类型名保留 `Spec` 后缀。
- 不加 builder、不加 `Widget` trait。

## 3. 已确认的决定

| # | 决定 |
| --- | --- |
| D1 | `ParagraphSpec { style: ParagraphStyle, children: Vec<Span<K>> }` |
| D2 | `ParagraphStyle.text: RunStyle` 取代段落 `color` 与 `font_size`；段落层可设 `RunStyle` 全部字段。解析顺序：文档种子 → 外层 `Styled` → `ParagraphStyle.text` → run 自己的 `style` |
| D3 | `line_height_factor: Option<f32>` 改为 `line_height: Option<LineHeight>`，构造时校验 |
| D4 | 删除 `CellBlock::Text` |
| D5 | 块级保留 `Paragraph`；`InlineRun` 改名 `Span`；`Span::Link.runs` 改名 `children` |
| D6 | `ColumnsSpec { style: ColumnsStyle { gap }, children }`；`ColumnSpec { style: ColumnStyle { width }, children }`；栏宽留在每一栏 |
| D7 | `ListKind::Ordered { numbering, start, format }`；`start` 与 `marker_format` 离开 `ListSpec` |
| D8 | `ListItemSpec { marker, children }`，不带 `style` |
| D9 | 表头表体是具名槽位 `header` / `body` |
| D10 | `header: Option<TableHeader<K>>`，`TableHeader { repeat, rows }`，`repeat` 默认 `true` |
| D11 | `col_span` / `row_span` 是 `CellSpec` 顶层描述性字段，`CellStyle` 只有 `fill` |
| D12 | 删除 `CellBlock`，单元格孩子是 `Block<K>`，限制全部由运行时规则矩阵检查 |
| D13 | `KeepTogether { children }`、`TableRow { children }`，不带 `style` |
| D14 | `Anchor { id, outline, children }`，由单个 `child` 改为一组；允许 `PageBreak`，锚点越过分页控制后落在后续首个有效片段 |
| D15 | `ImageSpec { image, style: ImageStyle { width, height, align } }` |
| D16 | 删除 `HLine` 的 `space_before` / `space_after`；`Block::HLine(HLineStyle)` |
| D17 | 正式保留流内 canvas：`CanvasSpec { size, style: CanvasStyle { align }, marks }`，`CanvasMark` 不变；取代提案书 §2.14 的旧「不做」结论 |
| D18 | 原 `BandMark` 改名 `Mark<K>`，页眉页脚与图层共用；`LayerMark<K> { when, child: Mark<K> }` |
| D19 | 矢量块与图层形状这次不合并，另起主题；届时文字 `at` 统一为行盒左上 |
| D20 | `BandSlot { font, style, children: Vec<PageText>, link: Option<Vec<PageText>> }` |
| D21 | `TextRun` 改名 `PageText` |
| D22 | `SectionSpec { style: SectionStyle { page_size, margins }, … }`；`BandSpec { style: BandStyle { inset }, … }` |
| D23 | `degrees: f32` 改为 `Angle`，构造时拒绝非有限值 |
| D24 | 保留 `Spec` 后缀 |

## 4. 公共 API 终态

```rust
pub enum Block<K> {
    Paragraph(ParagraphSpec<K>),
    Spacer(Length<NonNegative>),
    PageBreak,
    KeepTogether { children: Vec<Self> },
    Styled { style: RunStyle, children: Vec<Self> },
    Anchor { id: String, outline: Option<OutlineEntry>, children: Vec<Self> },
    Inset { style: InsetStyle, children: Vec<Self> },
    Columns(ColumnsSpec<K>),
    Table(TableSpec<K>),
    List(ListSpec<K>),
    Image(ImageSpec),
    Canvas(CanvasSpec<K>),
    HLine(HLineStyle),
}
impl<K> Block<K> { pub fn paragraph(font: K, text: impl Into<String>) -> Self; }

pub struct ParagraphSpec<K> { pub style: ParagraphStyle, pub children: Vec<Span<K>> }
pub struct ParagraphStyle {
    pub text: RunStyle,
    pub align: Alignment,
    pub first_line_indent: Length<NonNegative>,
    pub line_height: Option<LineHeight>,
}
impl<K> ParagraphSpec<K> { pub fn text(font: K, text: impl Into<String>) -> Self; }

pub enum Span<K> {
    Text { text: String, font: K, style: RunStyle },
    Link { target: LinkTarget, children: Vec<Self> },
    Blank(BlankSpec),
}
impl<K: Clone + PartialEq> Span<K> {
    pub fn split_latin(text: impl AsRef<str>, cjk: K, latin: K) -> Vec<Self>;
}

pub struct ColumnsSpec<K> { pub style: ColumnsStyle, pub children: Vec<ColumnSpec<K>> }
pub struct ColumnsStyle { pub gap: Length<NonNegative> }
pub struct ColumnSpec<K> { pub style: ColumnStyle, pub children: Vec<Block<K>> }
pub struct ColumnStyle { pub width: Track }

pub struct ListSpec<K> { pub style: ListStyle<K>, pub children: Vec<ListItemSpec<K>> }
pub struct ListStyle<K> {
    pub kind: ListKind,
    pub marker_width: Length<Positive>,
    pub body_indent: Option<Length<Positive>>,
    pub item_gap: Length<NonNegative>,
    pub marker_font: Option<K>,
    pub marker_align: MarkerAlign,
}
pub enum ListKind {
    Bullet,
    Ordered { numbering: Numbering, start: NonZeroU32, format: MarkerFormat },
}
pub struct ListItemSpec<K> { pub marker: Option<String>, pub children: Vec<Block<K>> }

pub struct TableSpec<K> {
    pub style: TableStyle,
    pub header: Option<TableHeader<K>>,
    pub body: Vec<TableRow<K>>,
}
pub struct TableStyle {
    pub tracks: Option<Vec<Track>>,
    pub cell_padding: Length<NonNegative>,
    pub border: TableBorder,
}
pub struct TableHeader<K> { pub repeat: bool, pub rows: Vec<TableRow<K>> }
pub struct TableRow<K> { pub children: Vec<CellSpec<K>> }
pub struct CellSpec<K> {
    pub style: CellStyle,
    pub col_span: NonZeroU16,
    pub row_span: NonZeroU16,
    pub children: Vec<Block<K>>,
}
pub struct CellStyle { pub fill: Option<Color> }

pub struct InsetStyle { pub left: Length<NonNegative>, pub right: Length<NonNegative> }
pub struct ImageSpec { pub image: ImageRef, pub style: ImageStyle }
pub struct ImageStyle {
    pub width: Option<Length<Positive>>,
    pub height: Option<Length<Positive>>,
    pub align: Alignment,
}
pub struct HLineStyle {
    pub width: Option<Length<Positive>>,
    pub align: Alignment,
    pub thickness: Length<Positive>,
    pub color: Color,
}
pub struct CanvasSpec<K> { pub size: Size, pub style: CanvasStyle, pub marks: Vec<CanvasMark<K>> }
pub struct CanvasStyle { pub align: Alignment }

pub struct SectionSpec<K> {
    pub style: SectionStyle,
    pub header: Option<BandFn<K>>,
    pub footer: Option<BandFn<K>>,
    pub behind: Vec<LayerMark<K>>,
    pub front: Vec<LayerMark<K>>,
    pub body: Vec<Block<K>>,
}
pub struct SectionStyle { pub page_size: Option<PageSize>, pub margins: Margins }

pub struct BandSpec<K> {
    pub style: BandStyle,
    pub left: Option<BandSlot<K>>,
    pub right: Option<BandSlot<K>>,
    pub marks: Vec<Mark<K>>,
}
pub struct BandStyle { pub inset: BandInset }
pub struct BandSlot<K> {
    pub font: K,
    pub style: RunStyle,
    pub children: Vec<PageText>,
    pub link: Option<Vec<PageText>>,
}
pub enum PageText { Literal(String), PageNumber, PageCount, SectionNumber, SectionCount }

pub enum Mark<K> {
    Shape(LayerShape<K>),
    Rotated { degrees: Angle, pivot: Point, shapes: Vec<LayerShape<K>> },
}
pub struct LayerMark<K> { pub when: PageWhen, pub child: Mark<K> }
```

各 `Default`：

| 类型 | 默认 |
| --- | --- |
| `ParagraphStyle` | `RunStyle::default()`、`Start`、0、`None` |
| `ColumnsStyle` | `gap` 0 |
| `ColumnStyle` | `width: Track::Auto`（现 `ColumnSpec` 没有 `Default`，这是新增） |
| `ListStyle<K>` | `Bullet`、18 pt、`None`、0、`None`、`Start`（同现 `ListSpec::default`）。`K` 不要求 `Default` |
| `ListKind` | 不实现 `Default`；`ListStyle` 的默认里写 `Bullet` |
| `TableStyle` | `None`、0、`TableBorder::default()` |
| `TableHeader<K>` | `repeat: true`、空 `rows` |
| `CellSpec<K>` | 跨度 1、空孩子、`CellStyle::default()` |
| `ImageStyle` | `None`、`None`、`Start`（现 `ImageSpec` 没有 `Default`，这是新增） |
| `HLineStyle` | `None`、`Center`、1 pt、黑（同现 `HLineSpec::default`，去掉两个间距） |
| `SectionStyle` | `None`、`Margins::default()` |
| `BandStyle` | `BandInset::default()` |

`Block` 的结构体变体（`KeepTogether`、`Anchor`、`Inset`、`Styled`）用内联字段，和现 `Styled` / `Inset` 同形；不另建 `KeepTogetherSpec` / `AnchorSpec`。

新增公共类型都遵守 API-7：至少实现 `Debug`，并按值语义实现 `Clone`；`LineHeight`、`Angle` 还实现 `Copy + PartialEq + Eq`。若将来需要 `Hash` / `Ord`，必须在 newtype 内统一 `Angle` 的 `-0.0` / `+0.0`，不得在调用点用 `to_bits()` 取巧（DET-3）。

### 4.1 新类型

| 类型 | 位置 | 构造 | 失败 |
| --- | --- | --- | --- |
| `LineHeight` | `spec`（同 `Ratio`） | `LineHeight::new(factor: f32) -> Result<Self, Error>`，`get()` 为 `pub(crate)` | 非有限或不大于 0：`GeometryKind::LineHeightFactor { factor }`（已有变体） |
| `Angle` | `geom`（`Affine::rotate_cw_about` 用它） | `Angle::degrees(value: f32) -> Result<Self, Error>`，`as_degrees()` | 非有限：`GeometryKind::NonFiniteRotation { degrees }`（已有变体） |

`LineHeight` 在 PR (1) 落地。`Angle`、`Affine::rotate_cw_about(Angle)`、公开 `Mark::Rotated.degrees: Angle` 与删除 `lower_rotated` 约 1900 行的 `NonFiniteRotation` 检查全部在 PR (5) 同时落地，不留「公开字段仍是 `f32`、内部先临时转换」的中间形态。`GeometryKind` 两个变体保留，构造点换到新类型。

### 4.2 重导出

`lib.rs` 根重导出：`Numbering`、`Ratio`、`Track` 等不变；`Angle` 随 `geom` 一起进根；`LineHeight` 只从 `spec` 公开（同 `MarkerFormat`），不扩大根面。改名的类型（`Span`、`PageText`、`Mark`）不留旧名（VER-3）。

## 5. 行为变化（CHANGELOG `Changed`）

1. **单元格文字继承全部字段。** 原 `CellBlock::Text` 在 `lower.rs:1129` 用 `paint.plain()`，只继承外层 `Styled` 的字号与颜色。删掉后单元格里是普通段落，粗体、斜体、下划线、删除线、字距都继承。CHANGELOG 写明：表格外包 `Styled { bold }` 时，格内文字会变粗。
2. **分隔线不自带空白。** `layout/hline.rs` 的高度从 `space_before + thickness + space_after` 变成 `thickness`，线画在框顶下 `thickness / 2`。线本体仍是不可拆的原子块。迁移：在 `HLine` 前后各放一个 `Spacer`；spacer 会吃满本页剩余高度并把余量带到下一页（§2.13），不会像段后距那样丢弃。原来总高度超过整页会报 `UnsplittableTallerThanFrame`，迁移后的 spacer 可以跨页；需要保留原来「空白与线整体不可拆」时，用 `KeepTogether` 包住三者。迁移还会把 `LimitKind::Blocks` 计数从一个 HLine 增为最多三个块。
3. **行高与角度构造时校验。** 现在公开的 `line_height_factor` 在 `lower.rs:671` 写进 `Style`，随后由 `text/measure.rs` 的 `LineMetrics::new` / `fontless` 在排版测量阶段校验。改用 `LineHeight` 后在值构造时以同一个 `GeometryKind::LineHeightFactor` 提前失败。角度原在 `layout_spec` 的 `lower_rotated` 报错，改为在 `Angle::degrees` 报错。

`Anchor` 改成 `children` 不算行为变化：只有一个孩子时与原 `child` 等价。

## 6. 实现细则

### 6.1 段落与行内（`spec/mod.rs`、`spec/lower.rs`、`spec/latin.rs`）

- `lower_paragraph`（`lower.rs` 632–675 行）：原来分别套 `color`、`font_size`、`line_height_factor`，改为把 `style.text` 当一层 `RunStyle` 补丁叠在外层 `Styled` 之上，再套 `align`、`first_line_indent`、`line_height`。迁移等价只指 `style.text.color` / `font_size`：段落字号（Em 空白、无字 run 行高用它）取叠完 `style.text` 后的字号，与现在「段落 `font_size` 覆盖外层」等价。
- 段落层 `style.text.bold` / `italic` / 装饰 / 字距是新增路径；仍按 §12.1 选面与绘制，缺面照常报 `BoldFaceRequired` 等。加选面、装饰与字距继承测试。
- `InlineRun` → `Span`；`spec/latin.rs` 的 `split_latin` 挂到 `Span`。深度预检里 `InlineRun::Link` 每层计一层的规则不变，只改名。

### 6.2 单元格（`spec/mod.rs`、`spec/lower.rs`、`spec/depth.rs`）

- 删除 `CellBlock` 枚举、`lower_cell_anchor`（`lower.rs` 约 1084–1107）及从 1109 行起的 `push_cell_block`。`lower_cell` 只调用 `lower_blocks(spec.children, ..)`，再把结果交给 `layout_table::Cell::new` / `CellBody::new`（`layout/nesting.rs:106`）；cell 专用路径不得再调用 `paint.plain()` 直接造 `TextNode`。
- `CellBody::new` 只接收 `lower_blocks` 的输出：`Styled` 已在 spec lower 摊平，`Anchor` / `Inset` / Stack 由矩阵递归。`TableCell` 禁 `KeepTogether`、`Table`、`Columns`、`PageBreak`；List 走 `ListBodies`，逐项继承单元格限制；Image / Canvas 允许。报 `InvalidCellContent { container: "table cell", .. }`，不新增错误。`nesting.rs` 不改以此前提为硬约束。
- `validate_cell_blocks`（`lower.rs` 从 2113 行起）删除，图片校验走正文同一个函数。
- `spec/depth.rs` 的 `Node::Cell` / `Owned::Cell` 分支（约 79–91、158 行）删除；表格路径把 `cell.children` 直接压成 `Node::Block` / `Owned::Block`。单元格孩子按普通 `Block` 计深度；§12.5 清单里「anchor（`Block` 与 `CellBlock` 两种）」「`Inset`（两种）」收成一种。`NestingDepth` 与 `Blocks` 的数值规则不因删除 CellBlock 改变。
- 拒绝时机从「类型上写不出」变成 lower 后建 `CellBody` 时。`Location` 与现在列表项内违规相同，只有 section，不补 `cell` 或块路径（§12.5 D19 仍另议）。

### 6.3 分栏、列表、表格、包裹、叶子

纯形状搬迁，lower 里改字段路径即可：

- 列表：`lower_list` 从 `ListKind::Ordered { numbering, start, format }` 取三个值，`Bullet` 时内部 `start` 用 1、`affix` 不读。`Numbering` 与 `MarkerFormat` 不变。
- 表格：`header: None` 与 `Some(TableHeader { rows: vec![], .. })` 降成同一个无表头的内部表（见 §9 Q2）。`repeat` 只在 `Some` 时读。
- `Anchor { children }` 不能简单 lower 成 `AnchorNode(sole_or_stack(children))`：当前 Stack 内的 `PageBreak` 会报 `NestedPageBreak`。实现须为一个逻辑 Anchor 保留「尚未落点」状态，让 `PageBreak` 按正文顶层协议分页，并在后续第一个 `fragment_is_effective` 的片段落点；首个有效片段可以是正高度 Spacer，其目标就是 spacer 顶。外层 Column / ListItem / TableCell 禁止 PageBreak 时仍由规则矩阵拒绝，Anchor 不得绕过。
- 一个公开 Anchor 在 `NestingDepth` 与 `LimitKind::Blocks` 中仍只计一次；不能为每个孩子展开成会重复计数的多个普通 `AnchorNode`。只向 `AnchorRegistry` 登记一个 expectation，重复表头仍 first-wins。孩子全空、只有零高空节点或以尾随 PageBreak 结束而没有有效片段时，照常报 `AnchorWithoutFragment`。
- Anchor 的内部 layout 表示必须向 `Flowable::nested` 暴露全部尚未处理的孩子，使 Column / ListItem / TableCell 的规则矩阵能递归看到其中的 PageBreak 和其他禁用节点；顶层分页能力不能变成绕过嵌套限制的后门。
- PR (3) 同步修改 `spec/depth.rs` 的 `push_children` / `dismantle`、`lower.rs::validate_blocks`：`Anchor` 分支遍历整个 `children`，不得保留单 `Box<child>` 路径。
- `HLine`：`layout/hline.rs` 删两个间距字段，`lower_hline`（`lower.rs:812`）跟着改；线本体仍不可拆。

### 6.4 节、页眉页脚、图层

- `BandSlot` 结构体：`link` 为 `Some` 时走原 `Link` 路径，`None` 走原 `Text` 路径；空 URI 仍按 §8 失败。
- `LayerMark { when, child }`：lower 里先按 `when` 过滤，再按 `Mark` 分派，原来每个变体里重复的 `when` 分支合并。`Mark::Rotated` 与页眉页脚的 `Rotated` 走同一个函数（现在 `lower.rs` 约 1890 行已共用 `Rotation`）。
- `SectionStyle` / `BandStyle`：字段路径变更。

### 6.5 不动

- layout 层内部类型除 `HLine` 与承载 Anchor 多孩子/PageBreak 所必需的 `AnchorNode` / 分页协议外不改形状；`ListNode`、`TableNode` 等保持不变。
- `RunStyle`、`Alignment`、`Track`、`TableBorder`、`BlankSpec`、`CanvasMark`、`LayerShape`、`PageWhen`、`BandFn`、`PageBandQuery`、`Margins` 不变。
- 规则矩阵 `nesting.rs` 不改；硬前提是 `Cell::new` / `ItemBody::new` 只接收 `lower_blocks` 输出，不能改成直接检查 spec 树。

## 7. PR 切分

顺序固定，每个 PR 自己能过 TOOL-5 门禁。合并后统一发 `0.6.0`，CHANGELOG 在每个 PR 里各写各的条目。

| PR | 内容 | 理由 |
| --- | --- | --- |
| (1) | 升 `pdfcraft-pdf` 到 `0.6.0`；同步根/包 README 版本与 CHANGELOG Breaking；登记 XxxStyle 的 API-9 例外；`LineHeight`、`ParagraphStyle`；`InlineRun` → `Span` | VER-2 要求 breaking 前先升 `y`；README identity 是 TOOL-5 门禁；段落是所有容器的叶子 |
| (2) | 删除 `CellBlock`（含 `CellBlock::Text`）；单元格孩子为 `Block` | 改 lower 与 depth 的结构，单独审 |
| (3) | 分栏、列表（`ListKind::Ordered`）、表格（`TableHeader`、`CellStyle`）、包裹（`InsetStyle`、`KeepTogether`、`Anchor.children`），以及 Anchor 多孩子/PageBreak 所需的内部 layout 改动 | 容器形状搬迁；Anchor 的分页语义在同一 PR 闭合 |
| (4) | 叶子：`ImageStyle`、`HLineStyle`（删间距）、`CanvasStyle` | 含一处行为变化，单独审 |
| (5) | 节、页眉页脚、图层：`Angle`、`Affine::rotate_cw_about(Angle)`、`SectionStyle`、`BandStyle`、`BandSlot`、`PageText`、`Mark`、`LayerMark { when, child }` | 角度类型与使用它的公开字段原子落地；文档级设置一起改 |
| (6) | 文档：`crates/pdfcraft-pdf/README.md`（版本在 PR 1 已先同步）、`docs/guide.md`、`docs/lists.md`、`examples/README.md`、`src/lib.rs` / `src/spec/mod.rs` 顶层说明、现有 `docs/migrate-*.md`、新的 0.6 集中迁移说明；提案书 §1、§2.3、§2.4、§2.6、§2.10、§2.12、§2.14、§4.1、§5、§8、§12.1–§12.6、确认/搁置总表及 AGENTS 的旧名改写 | 最后统一终态说明，避免每个 PR 改一遍长示例 |

示例与集成测试（`examples/*.rs`、`tests/*.rs`）在每个 PR 里跟着编译改，(6) 只做文字文档。

## 8. 测试

- 排版断言仍用 `RecordingCanvas`。纯形状搬迁的 PR，现有测试改构造写法后**坐标断言不得变化**；变了就是搬错了。
- 新增：
  - 段落层 `style.text.bold = Some(true)` 选粗体面；缺粗体面报 `BoldFaceRequired`。
  - 单元格里的段落继承外层 `Styled` 的粗体（行为变化 1）；格内 `Styled` 只作用于它包住的段落（§9 Q1）。
  - 单元格里放 `Columns` / `Table` / `KeepTogether` / `PageBreak`，各报 `InvalidCellContent { container: "table cell" }`（原来是类型挡掉，现在运行时）。
  - 单元格递归矩阵：`Styled → Columns`、`Anchor → Table`、`Inset → KeepTogether`；单元格列表项内 Columns / Table / KeepTogether 报 `"table cell"`，PageBreak 报 `"list item"`；Image / Canvas 与合法列表通过。
  - `HLine` 高度等于 `thickness`；同页时前后 `Spacer` 复现原坐标；跨页时验证 spacer 可拆、线仍原子；`KeepTogether` 包三者恢复原不可拆语义；迁移后的 `LimitKind::Blocks` 计数写测试（行为变化 2）。
  - `LineHeight::new` 拒绝 NaN、无穷、0、负数；`Angle::degrees` 拒绝 NaN、无穷。
  - `Anchor` 多个孩子：前导零高空孩子被跳过；前导正高度 Spacer 的目标在 spacer 顶；PageBreak 保持分页且目标落在后续首个有效片段；跨页 children 只落一次；全空、只有空节点或尾随 PageBreak 无后续有效片段时报 `AnchorWithoutFragment`；外层列表/栏/单元格仍拒绝 PageBreak。
  - 深度与析构：单元格内 Styled / Anchor / Inset、单元格列表项、`Anchor.children` 全部计入；超深输入由 `dismantle` 非递归释放。删除 CellBlock 前后等价树的 NestingDepth / Blocks 数值不变。
  - `TableHeader { repeat: false }` 不重复；`header: None` 与空 `rows` 输出相同。
  - `LayerMark { when: Last, child: Mark::Rotated { .. } }` 只画在末页。
- 删除只为 `CellBlock` 存在的测试（`validate_cell_blocks`、`CellBlock::Text` 继承规则），不留空壳。

## 9. 开工前确认项（已清零，2026-11-03）

| # | 问题 | 决定 |
| --- | --- | --- |
| Q1 | 删除 `CellBlock` 后，`Block::Styled` 能进单元格（原来不能） | **允许**。lower 把 `Styled` 摊平，规则矩阵看不到它，矩阵不改。CHANGELOG `Added` 写一条 |
| Q2 | `TableHeader { rows: vec![] }` | **等同 `None`**，rustdoc 写明，不加错误 |
| Q3 | PR 切分 | **按 §7 六个** |
| Q4 | 借鉴 Flutter / Compose 的组合模型 | **保持纯值树**；调用方用普通 Rust 函数组合，不引入组件运行时或 trait |
| Q5 | Style 与结构包装 | **typed style + 显式 wrapper**；不引入统一 Modifier 链 |
| Q6 | 孩子命名 | 通用组合用 child / children；rows / marks / shapes 等领域集合保留语义名 |
| Q7 | XxxStyle 演进 | 保持 exhaustive 公有字段 + Default + 字面量；首个实现 PR 登记 API-9 统一例外 |
| Q8 | 流内 Canvas | **正式保留**为低层叶子；不与图层形状合并 |
| Q9 | Anchor.children 中的 PageBreak | **支持**；保持顶层分页语义，锚点落在分页后首个有效片段 |

## 10. 不做

- 矢量块与图层形状合并（D19）。
- `Block::Styled` 加段落级参数。
- 栏内垂直对齐、栏间竖线、行高、行底色、每格内边距等新版式能力。
- 给 `Location` 补单元格坐标或块路径（§12.5 另议项不变）。
- builder API、`Widget` trait、serde。
- forms / orgchart / post；hcsw 工程。
- 擅自 `git commit` / `push`。
