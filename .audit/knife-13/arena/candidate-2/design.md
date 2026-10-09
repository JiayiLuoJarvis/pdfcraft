# Candidate 2：角色化字体 + typed runs + 一次性富文本排版

## Usage first

调用方显式注册正文与粗体两个 face；`bold` 只选择另一个 face，不合成字重。段落样式只作用于当前段落。

```rust
use quire_pdf::{
    document, Color, LineHeight, Margins, NonNegative, PageSize, Positive,
};

let body_size = Positive::pt(12.0)?;
let body_leading = LineHeight::factor(1.5)?;
let indent = NonNegative::mm(7.4)?;
let paragraph_gap = NonNegative::pt(6.0)?;

let rendered = document(PageSize::A4, |doc| {
    doc.font(REGULAR.to_vec())?;
    doc.bold_font(BOLD.to_vec())?; // 正常注册顺序下是 FontId(1)
    doc.margins(Margins::uniform(NonNegative::mm(25.4)?));

    doc.paragraph(|p| {
        p.font_size(body_size);
        p.line_height(body_leading);
        p.color(Color::rgb(20, 20, 20));
        p.first_line_indent(indent);
        p.space_after(paragraph_gap);
        p.text("合同编号 ");
        p.bold(&contract.id);
    });

    Ok(())
})?;
```

同一段内的 regular/bold 边界参与同一次断行；它们不是相邻块：

```rust
doc.paragraph(|p| {
    p.first_line_indent(indent);
    p.text("甲方确认编号 ");
    p.bold("A-2026-18");
    p.text(" 已核验。");
});
```

对齐仍是段落属性。首行在缩进后的 `[indent, W]` 框中对齐，后续行在 `[0, W]` 中对齐：

```rust
doc.columns(|row| {
    row.col(|col| {
        col.paragraph(|p| {
            p.center();
            p.first_line_indent(indent);
            p.text("栏内段落仍按栏宽 W 断行");
        });
    });
    row.col(|col| col.paragraph(|p| p.text("另一栏")));
});
```

未注册 bold face 时，只有实际含非空 `p.bold(...)` 的段落失败，返回 `UnknownFont { id: 1 }`；regular face 有该字符也不回退：

```rust
let err = document(PageSize::A4, |doc| {
    doc.font(REGULAR.to_vec())?;
    doc.paragraph(|p| p.bold("必须是真粗体 face"));
    Ok(())
})
.unwrap_err();
```

## Public surface

不公开 `Style`、run 类型、字体角色或断行阶段。新增的公共面只有一个字体槽方法、一个受控 factor 类型和段落 builder 方法。

```rust
// document.rs
impl Document<'_> {
    /// 注册 p.bold 使用的 face。常规用法要求它紧随 regular `font` 注册。
    pub fn bold_font(&mut self, bytes: Vec<u8>) -> Result<()>;
}

// document.rs
pub struct Paragraph {
    style: Style,              // 已解析快照，字段私有
    runs: Vec<SourceRun>,      // 类型私有
}

impl Paragraph {
    pub fn text(&mut self, s: impl AsRef<str>);
    pub fn bold(&mut self, s: impl AsRef<str>);

    pub fn font_size(&mut self, size: Length<Positive>);
    pub fn line_height(&mut self, line_height: LineHeight);
    pub fn color(&mut self, color: Color);
    pub fn space_after(&mut self, space: Length<NonNegative>);
    pub fn first_line_indent(&mut self, indent: Length<NonNegative>);

    pub fn start(&mut self);
    pub fn center(&mut self);
    pub fn end(&mut self);
}

// geom.rs；factor 不是长度，但同样在边界排除 <= 0、NaN、±∞。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineHeight(f32);

impl LineHeight {
    pub const DEFAULT: Self; // 1.2
    pub fn factor(value: f32) -> Result<Self>;
    pub const fn as_factor(self) -> f32;
}
```

`Flow::paragraph` 从 `Flow.style` 克隆已解析快照给 builder。setter 直接改这份快照；闭包结束只 lower 一次、只 push 一个 `FlowNode::Text`。因此不引入公共 `StyleOverride`，也没有第二套 merge/cascade 规则：

```rust
impl Flow {
    pub fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph)) {
        let mut paragraph = Paragraph::new(self.style.clone());
        f(&mut paragraph);
        self.push(Ok(paragraph.lower()));
    }
}
```

相邻且角色相同的非空 run 在 `Paragraph::append` 时合并；空 append 不制造 run。整段为空时仍按现状排出一个 regular 空行。`p.bold("")` 不要求 bold face。

## Core types and signatures

### 字体槽与 run

字体角色只有一个私有枚举，注册、测量和绘制都由它驱动，避免在多个模块手写「0 是 regular、1 是 bold」。

```rust
// document.rs / layout::flow 共用，crate-private
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FontRole {
    Regular,
    Bold,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FontSlots {
    regular: Option<FontId>,
    bold: Option<FontId>,
}

impl FontSlots {
    pub(crate) fn resolve(self, role: FontRole) -> Result<FontId>;
}

// document.rs
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceRun {
    role: FontRole,
    text: String,
}
```

正常调用顺序 `font` → `bold_font` 分别得到 `FontId(0)`、`FontId(1)`。`Document` 保存显式槽，而不是让 layout 猜「第二个 face」。`FontSlots::resolve` 对空 regular/bold 槽分别报告现有 `UnknownFont { id: 0/1 }`。`bold_font` 仍走 `FontStore::register`，所以解析和限额错误在组装期返回。重复或反序配置不另造可观察语义：实现拒绝不是“regular 后紧接的第二次注册”的 `bold_font`，使用一个窄的 `Error::InvalidFontConfiguration`，仍早于绘制。

```rust
// error.rs
Error::InvalidFontConfiguration
```

`Style` 不承载 bold；bold 是 run 的字体角色。段落样式只保留所有 run 共享的值：

```rust
// layout/style.rs
pub(crate) struct Style {
    pub(crate) font_size: Length<Positive>,
    pub(crate) line_height: LineHeight,
    pub(crate) color: Color,
    pub(crate) paragraph_spacing: Length<NonNegative>,
    pub(crate) first_line_indent: Length<NonNegative>,
    pub(crate) align: TextAlign,
}
```

默认 indent 明确是 `NonNegative::ZERO`。`Style::new` 不再重复校验 line-height factor；`LineHeight` 是唯一校验入口。

### Cluster tape and measurement

run 边界可能切进同一个扩展字素簇，CRLF 也可能横跨两个 run。不能逐 run 建 `ClusteredText` 再拼。`text::cluster` 增加一个 crate-private、与样式无关的 tagged 构造器：它在同一次「拼接 → CRLF 归一化 → grapheme segmentation → 去 BK glyph tape」过程中，给每个非 BK scalar 保留来源 tag。

```rust
// text/cluster.rs
pub(crate) struct TaggedClusteredText<T> {
    clustered: ClusteredText,
    glyph_tags: Box<[T]>, // 与 ClusterCursor.glyph 一一对应
}

pub(crate) fn cluster_tagged<T: Copy, S: AsRef<str>>(
    runs: impl IntoIterator<Item = (T, S)>,
) -> TaggedClusteredText<T>;
```

这不是公共富文本框架；它只防止 paragraph 和 cluster 两边各自实现一套 CRLF/byte remap。一个 grapheme 可以含多个 tag，但 breaker 仍把整个 grapheme 当一个 atom。

paragraph 层把连续同角色 glyph 批量交给现有 `measure_text`，再扁平化成唯一的 measured tape：

```rust
// layout/paragraph.rs
#[derive(Debug, Clone, PartialEq)]
struct MeasuredInline {
    text: TaggedClusteredText<FontRole>,
    glyphs: Vec<InlineGlyph>,
}

#[derive(Debug, Clone, PartialEq)]
struct InlineGlyph {
    font: FontId,
    glyph_id: u32,
    advance_norm: f32,
    glyph_byte_range: Range<usize>, // 统一 glyph_text 坐标
}

fn measure_inline(
    fonts: &mut FontStore,
    slots: FontSlots,
    runs: &[SourceRun],
) -> Result<MeasuredInline>;
```

cluster 宽度直接求其 glyph range 内 `advance_norm * style.font_size` 之和。绘制也消费同一 `InlineGlyph` tape，不重新查宽，保证量画同源。`MissingGlyph` 携带真实 resolved `FontId`；bold 缺字不会尝试 regular。

### First-line width API

断行器显式接收首行和后续行两个宽度，不用 `Option` 或闭包让调用者理解内部行号：

```rust
// text/line_break.rs
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineWidths {
    first: f32,
    rest: f32,
}

impl LineWidths {
    pub(crate) fn uniform(width: f32) -> Result<Self>;
    pub(crate) fn first_then(first: f32, rest: f32) -> Result<Self>;
    fn for_line(self, line_index: usize) -> f32;
}

pub(crate) fn break_lines<'text>(
    text: &'text ClusteredText,
    widths: LineWidths,
    width_of: impl FnMut(Cluster<'text>) -> f32,
) -> Result<BrokenLines<'text>>;
```

`pack_lines` 每次 fit、overflow 和 `UnbreakableWiderThanFrame.avail` 都读取 `widths.for_line(records.len())`。hard break 一旦 emit 第一条记录，后续也用 `rest`；分页不会重置这个 index。

`ParagraphNode::layout` 是唯一把 indent 翻译成几何的地方：

```rust
let w = frame_width;
let i = style.first_line_indent.as_pt();
let first = w - i;
if first <= 0.0 {
    return Err(Error::InvalidGeometry);
}
let lines = break_lines(clustered, LineWidths::first_then(first, w)?, width_of)?;

// line 0
x = i + align_x(first, line_width, style.align);
// line n > 0
x = align_x(w, line_width, style.align);
```

因此 Start 的首行 x 是 `I`；End 仍贴原框右边；Center 在 `[I, W]` 内居中。`ParagraphNode.width` 继续保存完整 `W`。缩进已烘焙进第 0 个 `LineBox`，跨页 tail 只切片，不会把续页第一行再次缩进。

### Line boxes and metrics

一条视觉行含若干 homogeneous `GlyphRun`；每个 run 的 x 是行原点加前面 glyph 的实际 advance 总和。

```rust
// layout/paragraph.rs
pub(crate) struct LineBox {
    pub(crate) runs: Vec<GlyphRun>,
}

fn glyph_runs_for_line(
    measured: &MeasuredInline,
    line: BrokenLine<'_>,
    style: &Style,
    line_origin_x: f32,
    baseline: f32,
) -> Result<Vec<GlyphRun>>;
```

切 run 的唯一条件是 resolved `FontId` 改变；字号和颜色在本 knife 是段落级。每个 `GlyphPlacement.byte_range` 重基到该 `GlyphRun.text`，后一个 run 的 origin 使用前一个 run 同一批 placements 的 pt advance。

混合 face 的段落采用一个稳定行盒：对该段实际使用的 face 求 metrics envelope，`baseline = max(ascent)`，下伸取 `min(descent)`，leading 取最大值，再与 `font_size * line_height.factor` 取 max。所有行使用这一 baseline/line-height。这样不会因 bold face 较高而让相邻行相撞，也不把 `ParagraphNode::split` 扩成可变行高算法；空段落只看 regular face。

`fragment_from_lines` 对每条线顺序发出多个 `FragmentCommand::Glyphs`，给它们加相同的 y。Canvas、RecordingCanvas、krilla adapter 都无需新增 bold 指令。

## Module map

| 文件 | 责任变化 |
| --- | --- |
| `src/document.rs` | `bold_font`、`FontSlots`、带 resolved `Style` 和 typed runs 的 `Paragraph`；一个 paragraph 仍 lower 成一个节点 |
| `src/geom.rs` | 新增受控 `LineHeight` factor |
| `src/error.rs` | 新增窄的 `InvalidFontConfiguration`；其余继续用 `UnknownFont`、`MissingGlyph`、`InvalidGeometry` |
| `src/layout/style.rs` | 去掉字体 id，加入 `first_line_indent`，持有已校验 `LineHeight` |
| `src/layout/flow.rs` | `LayoutCtx` 只读携带 `FontSlots`；无 public `Flowable` 变化 |
| `src/layout/paragraph.rs` | tagged runs → resolved font → measured tape → 一次断行 → 多 GlyphRun；metrics envelope；首行 x |
| `src/text/cluster.rs` | 在唯一归一化/分簇过程中保留每 glyph tag |
| `src/text/line_break.rs` | `LineWidths` 取代单一 `avail` |
| `src/text/measure.rs` | 保留单 face `measure_text`；paragraph 批量调用，不在这里引入“bold”概念 |
| `src/layout/paginate.rs` | 不改；继续先 plan/measure，成功后才 `start_page` |
| `src/canvas.rs`、`src/backend/*` | 不改协议；bold 是另一条 `GlyphRun.font` |
| `src/lib.rs` | 只重导出 `LineHeight`；run/font-role 内部不可导入 |
| `tests/document.rs` | RecordingCanvas 集成断言；backend 仍只做 `%PDF` smoke |

Cell、Band、Layer 明确不扩 API：`Cell::new` 仍是 regular plain text；Band slot 仍是 regular plain paragraph 且 indent 为 0；Layer `text_at` 仍走不换行 `single_line_run`，完全不读 first-line indent。列表项和栏内若调用 `paragraph`，自然得到它们局部 `W` 下的 indent；列表 marker 不受影响。

## Dominant access paths

```text
Document::font / bold_font
  → FontStore + FontSlots

Flow::paragraph
  → Paragraph { resolved Style, SourceRun[] }
  → one TextNode
  → TextNode::split(Frame W, LayoutCtx slots)
  → cluster_tagged(all runs once)
  → resolve role + measure_text(per homogeneous role)
  → flat InlineGlyph tape
  → break_lines(LineWidths { W-I, W })
  → per line: align once, split homogeneous GlyphRuns
  → baked ParagraphNode
  → paginate slices only
  → RecordingCanvas / krilla uses each run's FontId
```

任何 `UnknownFont`、`MissingGlyph`、`I >= W`、首行不可断超宽都在 `TextNode::split` 的规划阶段返回。现有 `paginate_sections` 在所有正文、band、layer 测量成功前不调用 `start_page`，所以失败画布为空。

## Verification contract

1. `Paragraph` unit test：`text/bold/text` 保留三个角色边界，相邻同角色合并，空 append 不要求 bold。
2. `cluster_tagged`：run 边界切在 `e` 与 combining acute 之间仍是一个 cluster、两个 glyph tag；跨 run 的 `\r` + `\n` 仍是一个 BK。
3. `line_break`：首行只用 `first`，wrap/BK 后都用 `rest`；首行错误里的 `avail` 是 `W-I`；uniform 保持现有测试。
4. RecordingCanvas：同一行 regular/bold 分别记录 `FontId(0/1)`，baseline 相同，第二 run x 精确等于第一 run x 加其 glyph advances。
5. RecordingCanvas：Start/Center/End 首行分别使用 `I`、`I + (W-I-ink)/2`、`W-ink`；第二行回到完整 `W`。
6. 小页分页：段落第 0 行在第一页缩进，tail 首行到第二页后不重新缩进。
7. 缺 bold slot、bold 缺 glyph、`I >= W` 都断言 canvas commands 为空；bold 缺字的 error font 是 1。
8. style builder：字号、颜色、line-height、段后距和对齐反映在 recorded runs/坐标，且不污染下一段、Cell、Band 或 Layer。
9. 新增一份可复现、OFL 兼容的小型 bold fixture，至少让一个字符 advance 与 regular 不同，钉住“用 bold face 宽度断行”而不只验证 id。
10. `backend-krilla` 只断言含 mixed runs 的结果以 `%PDF` 开头，不断言压缩内容流。

## Autonomy decisions and exclusions

- 采用 frame 默认：bold 是显式注册的第二 face（正常路径 `FontId(1)`），不是 synthetic bold。反转词：`bold-is-same-face`。
- 采用 frame 默认：首行缩进默认 0，不自动二字。反转词：`default-two-em`。
- 采用 frame 默认：不做 named style/defaultStyle map。反转词：`named-styles-in`。
- 采用缩进后对齐框 `[I, W]`；End 仍贴右，Center 在剩余框居中。
- 不做 italics、decoration、§2.2 blank、flow canvas、justify、任意 `FontId` span、Cell/Band/Layer rich text。

## Red-flag screen

- **Shallow module：通过。** 调用方只注册两个 face、写段落；cluster remap、多 face 测量、行盒和失败时序都藏在内部。
- **Information leakage：通过。** 公共 DSL 不出现 `FontId(1)`、glyph range、cluster cursor 或 break stage。
- **Temporal decomposition：通过。** typed source runs 和 measured tape 各自拥有一个阶段的不变量，不暴露“先 measure 再 break”给调用方。
- **Pass-through method：通过。** `bold_font` 增加角色绑定策略；Paragraph setters 修改 resolved snapshot，不是同签名转发层。
- **Split ownership：通过。** Document 独占字体槽，Paragraph 独占 run/style，ClusteredText 独占归一化。
- **Two ways：通过。** regular 只有 `text`，粗体只有 `bold`；不同时开放 `p.font(FontId)` 或 named styles。
- **Importable internals：通过。** `FontRole`、`SourceRun`、`TaggedClusteredText`、`LineWidths` 均为 crate-private。
- **Hand-synced list：通过。** 字体角色由同一枚举驱动 resolve/measure；没有另一份 bold bool 或 Canvas flag。
