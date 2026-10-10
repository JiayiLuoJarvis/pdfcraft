# candidate-2：两阶段排版（`LineBox` + 高度游标）

Knife 6 的内部形状是两段，中间不经过 `FlowNode::split`，也没有 `Fragment` / `FragmentCommand`。

1. **阶段 1** 在当前框宽 `W` 上把段落（以及 spacer、pageBreak、keepTogether、stack）排成自有的高度单位。段落的每一行是一个 `LineBox`，行盒就是片段。`GlyphRun` 的原点是片段内的绝对坐标：左上为原点，y 向下；x 已经含 `Start` / `Center` / `End`，y 已经是 `line_metrics.baseline`。出页只加行盒左上角的页面位置，不再累加 advance，也不重算对齐。
2. **阶段 2** 用一个只看高度和 `PageBreak` 的游标分页。出页时把 `LineBox` 平移到页面坐标，调用 `RecordingCanvas::draw_glyph_run`。Spacer、段后距、换页标记没有绘制命令。

`lay_out` 只认 `W`。`paginate` 只认正文框的原点与高 `H`。同一批 `LineBox` 可以放进更高的框；`W` 变了必须重新 `lay_out`。

本刀不写 `src/`。下面的签名是实现时要落进去的 crate 内部表面，函数体保持 `todo!()`，契约以本文为准。

---

## 1. 用法草图

测试在 `src/layout/` 的 `#[cfg(test)]` 里。不新增 `tests/` 集成测，不从 `document` 构造。夹具字体走已有的 `fonts::fixture_bytes`（`tests/fixtures/quire-measure.ttf`）。坐标用 `pt_near`（0.01 pt）。

三个库入口，测试直接调用，不包一层 `render`：

```rust
let laid = lay_out(&source, &mut fonts, width)?;          // 阶段 1，只认 W
let pages = paginate(&laid, body)?;                       // 阶段 2，只认原点与 H
paint(&pages, page_size, &mut canvas)?;                   // 出页重放，不再测量
```

`paginate` 失败时不调用 `paint`，`RecordingCanvas` 保持空日志。

### 1.1 段落：基线与三种对齐

`W` 是正文框宽，不是页宽。正文框可以离开页面原点，用来锁住「平移发生在出页」。

```rust
use crate::canvas::{pt_near, Canvas, CanvasCommand, RecordingCanvas};
use crate::fonts::{fixture_bytes, FontId, FontSource, FontStore};
use crate::geom::{Color, NonNegative, Point, Positive, Size};
use crate::text::measure::line_metrics;
// lay_out / paginate / paint / Source / ParagraphInput / TextAlign / BodyFrame
// 都是 layout 内的 pub(crate) 项，测试模块与它们同 crate。

fn store() -> (FontStore, FontId) {
    let mut fonts = FontStore::new();
    let id = fonts
        .register(FontSource { bytes: fixture_bytes(), index: 0 })
        .unwrap();
    (fonts, id)
}

#[test]
fn center_baseline_uses_frame_width_and_body_origin() {
    let (mut fonts, id) = store();
    let size = Positive::pt(12.0).unwrap();
    let metrics = line_metrics(fonts.metrics(id).unwrap(), size, 1.2).unwrap();
    // 夹具里「第」的 advance_norm 是 1。1.5 倍字号放得下一字、放不下两字。
    let width = Positive::pt(size.as_pt() * 1.5).unwrap();
    let source = Source::Paragraph(ParagraphInput {
        text: "第第".to_owned(),
        font: id,
        font_size: size,
        line_height_factor: 1.2,
        color: Color::rgb(0, 0, 0),
        align: TextAlign::Center,
        spacing_after: NonNegative::pt(0.0).unwrap(),
    });
    let laid = lay_out(&source, &mut fonts, width).unwrap();
    let page = Size::try_pt(400.0, 400.0).unwrap();
    let body = BodyFrame {
        origin: Point::try_pt(12.0, 16.0).unwrap(),
        size: Size::new(width, Positive::pt(200.0).unwrap()),
    };
    let pages = paginate(&laid, body).unwrap();
    let mut canvas = RecordingCanvas::new();
    paint(&pages, page, &mut canvas).unwrap();

    let recorded = canvas.pages();
    assert_eq!(recorded.len(), 1);
    let runs: Vec<_> = recorded[0]
        .ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::DrawGlyphRun { run } => Some(run),
            _ => None,
        })
        .collect();
    assert_eq!(runs.len(), 2);
    let line_w = runs[0].glyphs[0].advance_norm * size.as_pt();
    assert!(pt_near(
        runs[0].origin.x().as_pt(),
        12.0 + (width.as_pt() - line_w) / 2.0
    ));
    assert!(pt_near(
        runs[0].origin.y().as_pt(),
        16.0 + metrics.baseline
    ));
    assert!(pt_near(
        runs[1].origin.y().as_pt(),
        16.0 + metrics.line_height + metrics.baseline
    ));
    // 字形 id 与 measure_text(glyph_text) 同一条管线，不在绘制时重测。
}
```

`Start` 的 x 是 `body.origin.x()`。`End` 的 x 是 `body.origin.x() + (W - line_width)`。行宽用 `BrokenLine::width()`：不含被丢掉的行首 U+0020，含仍留在行尾、当时放得下的 U+0020。每个 `DrawGlyphRun` 的 advance 之和等于该行宽。

阶段 1 单测不经过分页，直接看行盒局部原点：`run.origin.y() == metrics.baseline`，`Center` 的局部 x 是 `(W - line_width) / 2`。出页测试只证明再加上正文原点与行顶游标。

空段落 `""` 以及 `"A\n\nB"` 的空行仍占 `line_height`。空行 `glyphs` 为空，`paint` 不调用 `draw_glyph_run`。`"A\n\nB"` 里第二个有字行的基线比第一行低 `2 * line_height`。

`W` 非正由 `Length<Positive>` 挡在构造期。`line_height_factor` 非有限或 `<= 0` 时 `line_metrics` 返回已有的 `InvalidGeometry`。缺字是 `MissingGlyph`。簇宽于 `W` 是 `break_lines` 已有的 `UnbreakableWiderThanFrame`。本刀不另写断行。

### 1.2 分页：四条 PageBreak 轨迹

游标语义与提案书 §2.9 一致。本刀不画页眉页脚；空白页仍是一次 `start_page` + `finish_page`，页数用 `RecordingCanvas::pages().len()` 断言，空白页的 `ops()` 里没有 `DrawGlyphRun`。

| 测试名 | 输入 | 页 |
| --- | --- | --- |
| `leading_page_break_is_one_empty_page` | `[PageBreak]` | 1，无字形 |
| `two_leading_page_breaks_stay_one_page` | `[PageBreak, PageBreak]` | 1，无字形 |
| `trailing_page_break_is_dropped` | 一段文字，然后 `[PageBreak]` | 1，有字形 |
| `two_trailing_page_breaks_emit_one_blank` | 一段文字，然后 `[PageBreak, PageBreak]` | 2；第 2 页无字形 |
| `break_between_content_is_two_pages` | 文字、`PageBreak`、文字 | 2，两页都有字形，中间无空白页 |
| `double_break_between_content_inserts_blank` | 文字、`PageBreak`、`PageBreak`、文字 | 3；中间页无字形 |

空 `Source::Stack(vec![])` 走 `Fresh` 结束，也是 1 页空白。这与「只有开头的 PageBreak」的页数相同；区分它们的是上表第 2、4 行，而不是空文档本身。

高度 0 的 spacer 是内容（`Fit`），会把 `Fresh` 推进到 `HasBody`。因此 `[Spacer(0), PageBreak, PageBreak]` 是 2 页：第 1 页是这个零高 spacer 所占的正文页，第 2 页是第二个 break 提交的空白页。`[PageBreak, PageBreak]` 仍是 1 页。

### 1.3 段后距丢掉、spacer 拆开、keep 整组

框高由测试用 `line_metrics.line_height` 拼出来，不写死夹具 ascent。

段后距放不下就整段丢掉，下一页第一行的基线是 `body.y + baseline`：

```rust
// H = line_height + 5，spacing_after = 6，两段各一行。
// 第 1 页只有第一行。剩余 5 < 6，Gap 丢弃，不为间距换页。
// 第 2 页第一笔字形的 y = body.origin.y + baseline。
```

Spacer 按高度拆，余量可以是下一页的唯一内容：

```rust
// 只有 Spacer，高度 = 正文框高 + 1，没有段落。
// 两页，两页都没有 DrawGlyphRun。
// 后面若再跟一段一行的段落，且余量为 1、行高 > 1：
// 余量占满第 2 页顶部后，该段落到第 3 页，基线仍是 body.y + baseline。
```

`KeepTogether` 是原子。组高含孩子之间的段后距，不含最后一个孩子的段后距：

```rust
// 组内：一行、spacing 4、一行；组后 spacing 6。
// lay_out 得到 Keep(pieces) 然后单独的 Gap(6)。
// pieces 的高度 = 2 * line_height + 4。Gap 不在这个和里面。
// 框高恰好等于这个和：整组在第 1 页，Gap 放不下则丢。
// 组前另有一行、框高只够「前一行 + 整组」中的前一行：
// 两行都在第 2 页，第 2 页第二行的 y = body.y + line_height + baseline。
// 组高 > 框高：paginate 返回 UnsplittableTallerThanFrame，canvas 无命令。
```

单行的 `line_height > H` 同样是 `UnsplittableTallerThanFrame`（行不可横切）。`lay_out` 仍然成功：它不知道 `H`。

---

## 2. 类型草图

### 2.1 阶段 1 的输入

没有公共 `Block`，也没有 `lower`。测试和将来的 DSL 都停在这个 crate 内部树上。树没有 `split` 方法。

```rust
// src/layout/flow.rs
pub(crate) enum Source {
    Paragraph(ParagraphInput),
    Spacer(Length<NonNegative>),
    PageBreak,
    KeepTogether(Vec<Source>),
    Stack(Vec<Source>),
}

// src/layout/paragraph.rs
pub(crate) struct ParagraphInput {
    pub text: String,
    pub font: FontId,
    pub font_size: Length<Positive>,
    pub line_height_factor: f32,
    pub color: Color,
    pub align: TextAlign,
    pub spacing_after: Length<NonNegative>,
}

pub(crate) enum TextAlign {
    Start,
    Center,
    End,
}
```

`TextAlign` 没有 `Justify`。字号、spacer 高、段后距的非负/正数由 `geom` 的 newtype 在构造期完成。`lay_out` 不再检查 NaN 字号或负间距。

`layout/lower.rs`、`document.rs` 本刀不动。具名样式表、`StyleOverride`、`resolve_style` 留到有公共 DSL 的刀；测试直接填 `ParagraphInput`。

### 2.2 行盒

一行一个 `GlyphRun`。本刀一段一种字体、字号、颜色。将来行内多样式把字段改成 `Vec<GlyphRun>`，高度仍是这一行。

```rust
// src/layout/paragraph.rs
pub(crate) struct LineBox {
    /// `line_metrics.line_height`。空行也是这个高度。
    pub height: f32,
    /// 原点在行盒局部：x 已对齐，y = `baseline`。
    pub run: GlyphRun,
}

pub(crate) struct ParagraphLayout {
    pub lines: Vec<LineBox>,
    pub spacing_after: f32,
}

pub(crate) fn layout_paragraph(
    input: &ParagraphInput,
    fonts: &mut FontStore,
    width: Length<Positive>,
) -> Result<ParagraphLayout> {
    todo!("knife 6")
}
```

`layout_paragraph` 的步骤：

1. `ClusteredText::new(&input.text)`。
2. `measure_text(fonts, input.font, clustered.glyph_text())`。只测这一次。BK 不在 `glyph_text` 里。
3. `line_metrics(fonts.metrics(font)?, font_size, factor)`。行高和基线只读返回值，段落里不重写公式。
4. `break_lines(&clustered, width.as_pt(), width_of)`。`width_of` 对簇的 `glyph_range()` 求和：`advance_norm * font_size.as_pt()`。与第 2 步同一批 `advance_norm`。
5. 每个 `BrokenLine` 复制成自有 `GlyphRun` 后，`ClusteredText` 即可丢弃。`BrokenLines` 借用源文本，不能放进分页列表。
   - `text` = `line.text().to_owned()`。
   - `glyphs` = `measured.glyphs[line.glyph_range()]`，`byte_range` 减去 `line.glyph_byte_range().start`，因为范围是进 `GlyphRun::text` 的。
   - `glyph_id` / `advance_norm` 原样拷贝。
   - 局部 x：`Start → 0`，`Center → (W - line.width()) / 2`，`End → W - line.width()`。
   - 局部 y：`metrics.baseline`。
   - `height`：`metrics.line_height`。
6. 空行（空段、连续 BK、被丢掉的行首空格留下的空行）`glyphs` 为空，高度仍是一行。`spacing_after` 取 `input.spacing_after.as_pt()`，0 也原样返回，由 `lay_out` 决定是否发出 `Gap`。

对齐不改变断点。U+3000 与普通簇一样占用 advance，不按 U+0020 丢弃；丢弃规则留在 `break_lines` 里。

### 2.3 扁平列表

```rust
// src/layout/flow.rs
pub(crate) struct LaidFlow {
    /// 阶段 1 使用的 W。paginate 要求正文框宽与它相同。
    pub width: Length<Positive>,
    pub items: Vec<FlowItem>,
}

pub(crate) enum FlowItem {
    Line(LineBox),
    /// 段后距。放不下则整段丢弃，没有余数字段。
    Gap(f32),
    /// 用户 spacer 的剩余高度。分页器用游标旁的 carry 拆它，不把 item 改写成尾节点。
    Spacer(f32),
    PageBreak,
    Keep(Vec<KeptPiece>),
}

/// 组内零件。没有 PageBreak，没有可丢弃的 Gap，没有可拆的 Spacer。
pub(crate) enum KeptPiece {
    Line(LineBox),
    /// 孩子之间的段后距，或组内 spacer。组一旦放下，这些前进全部吃掉。
    Advance(f32),
}

pub(crate) fn lay_out(
    source: &Source,
    fonts: &mut FontStore,
    width: Length<Positive>,
) -> Result<LaidFlow> {
    todo!("knife 6")
}

pub(crate) fn kept_height(pieces: &[KeptPiece]) -> f32 {
    todo!("knife 6")
}
```

`lay_out` 借用 `Source`，不消耗树。分页之后源树还在；换 `W` 时重新调用即可。

展开规则：

| `Source` | 发出的 `FlowItem` |
| --- | --- |
| `Paragraph` | 每行一个 `Line`。`spacing_after > 0` 时再跟一个 `Gap`。0 距不发 `Gap` |
| `Spacer(h)` | 一个 `Spacer(h.as_pt())`，含 0 |
| `PageBreak` | 一个 `PageBreak` |
| `Stack` | 按顺序拼接孩子。stack 本身不是 item |
| `KeepTogether` | 恰好一个 `Keep`，最后一个孩子的段后距若 `> 0` 再跟一个组外 `Gap` |

组内折叠（最后一个段后距留在组外）：

```text
pending = 0
pieces = []
for child in keep.children {
    if pending > 0 { pieces.push(Advance(pending)); pending = 0 }
    match child {
        Paragraph => pieces 追加各行；pending = spacing_after
        Spacer(h) => if h > 0 { pieces.push(Advance(h)) }
                     // h == 0 不产生 Advance，但 Keep 这个 item 仍然存在
        PageBreak => return Err(InvalidGeometry)
        Stack    => 孩子就地进入这个循环。
                    不要走外层 lay_out，否则段后距会变成可丢弃的 Gap
        KeepTogether => 递归得到内层 pieces 与内层 trailing；
                        pieces 追加内层 pieces；pending = 内层 trailing
    }
}
发出 Keep(pieces)；若 pending > 0，再发出 Gap(pending)
```

组高 = 各 `Line.height` 与 `Advance` 之和，不含组外那个 `Gap`。`KeepTogether(vec![])` 仍发出 `Keep(vec![])`，高度 0，算内容。组内的 spacer 变成 `Advance`，分页器不能把它拆到下一页；组高因此超过 `H` 时整组报错。

`PageBreak` 出现在 `KeepTogether` 里（含嵌在内层 stack / 内层 keep）是 `InvalidGeometry`。提案书允许的组合是组**后面**再跟 break，那是两个并列的 `Source`，会变成 `Keep` 然后再一个 `PageBreak`。

`kept_height` 是组高的唯一计算。不在 `Keep` 上另存一份高度。

### 2.4 阶段 2：游标

```rust
// src/layout/paginate.rs
pub(crate) enum PageCursor {
    Fresh,
    HasBody,
    PendingBlank,
}

pub(crate) struct BodyFrame {
    pub origin: Point,
    pub size: Size,
}

pub(crate) struct PlacedLine {
    /// 行盒左上角的页面坐标。
    pub origin: Point,
    pub line: LineBox,
}

pub(crate) struct PageBody {
    pub lines: Vec<PlacedLine>,
}

pub(crate) fn paginate(laid: &LaidFlow, body: BodyFrame) -> Result<Vec<PageBody>> {
    todo!("knife 6")
}

pub(crate) fn paint(
    pages: &[PageBody],
    page_size: Size,
    canvas: &mut dyn Canvas,
) -> Result<()> {
    todo!("knife 6")
}
```

`body.size.width() != laid.width` 时 `paginate` 返回 `InvalidGeometry`（关系约束，挡「用 A 宽断行、用 B 宽放置」）。`paginate` 不调用 `FontStore`、`measure_text`、`break_lines`。

`PageCursor` 只有三态。Spacer 的余量是旁边的 `carry: Option<f32>`，不是第四个游标态，也不是 `FlowItem` 尾节点。

正文框高 `H = body.size.height().as_pt()`，`Size` 保证 `H > 0`。`y` 是当前页已占用的高度，从 0 起。`remain = H - y`。

放置一行时写入 `PlacedLine { origin: (body.x, body.y + y), line }`。组内 `Advance` 只增加 `y`，不写入 `lines`。

**换页标记**（先匹配，与高度无关）：

| 游标 | `PageBreak` |
| --- | --- |
| `Fresh` | 忽略，保持 `Fresh` |
| `HasBody` | 提交当前缓冲。游标改为 `PendingBlank`，`y = 0`，缓冲清空。不立刻再输出一页 |
| `PendingBlank` | 再提交一页空 `PageBody`。游标保持 `PendingBlank` |

**流结束**：

| 游标 | 结果 |
| --- | --- |
| `Fresh` | 提交一页空正文 |
| `HasBody` | 提交当前缓冲 |
| `PendingBlank` | 丢掉挂起页，不提交 |

**`Gap(g)`**：只有 `HasBody` 且 `y + g <= H` 时 `y += g`。其余情况整段丢掉，不改游标，不换页，不留余数。`remain == 0` 时也是丢掉，禁止先换页再在新页顶部吃掉这段距。

**`Spacer`**：

- 高度 0：若游标是 `Fresh` 或 `PendingBlank`，打开正文（见下），然后消耗该 item。`y` 不变。
- 高度 > 0：设 `carry = height`，消耗该 item。之后每圈循环先处理 `carry`，再读下一个 item。
- 处理 `carry` 时若游标尚未 `HasBody`，先打开正文。若 `remain == 0`，按「溢出换页」提交当前页并在新页继续（新页 `y = 0`，游标保持 `HasBody`）。`take = min(carry, remain)`，`y += take`。`carry` 减去 `take`。减到 0 则清空 `carry`。未减完则溢出换页，下一圈继续。
- 每页至少减去 `H > 0`，不会空转。余量可以独占一整页，该页没有字形。
- Spacer 永不产生 `UnsplittableTallerThanFrame`。

**打开正文**（放入行、组、零高 spacer，或开始消耗 `carry` 时）：

- `Fresh` → `HasBody`，`y = 0`，缓冲为空。此时还不 `push` 页面。
- `PendingBlank` → 同一动作。这张挂起页变成当前缓冲，不额外提交空白页。
- 已经是 `HasBody` 则不动。

**`Line` 与 `Keep`**：

1. `h` 分别是 `LineBox.height` 与 `kept_height`。
2. `h > H` → `Err(UnsplittableTallerThanFrame)`。先于「能不能塞进剩余高度」判断。空页上放不下同样报错，禁止先提交一页空白再重试。
3. 打开正文。
4. `y + h <= H`：按顺序放下（组内含全部 `Advance`），游标保持 `HasBody`。
5. 否则当前页已经有占用：溢出换页（提交缓冲，`y = 0`，游标保持 `HasBody`），再放下。第 2 步保证此时 `h <= H`。

比较用 `f32` 的 `<=`，与 `break_lines` 相同。测试断言用 `pt_near`。

溢出换页与 `PageBreak` 是两个函数。溢出换页之后游标是 `HasBody`，因为新页马上要放内容。`PageBreak` 之后才是 `PendingBlank`。

### 2.5 出页

```text
for page in pages {
    canvas.start_page(page_size)?
    for placed in page.lines {
        if placed.line.run.glyphs.is_empty() { continue }
        let mut run = placed.line.run.clone();
        let x = placed.origin.x().as_pt() + run.origin.x().as_pt();
        let y = placed.origin.y().as_pt() + run.origin.y().as_pt();
        run.origin = Point::try_pt(x, y)?;
        canvas.draw_glyph_run(&run)?;
    }
    canvas.finish_page()?;
}
```

`Point::try_pt` 遇到非有限坐标时返回已有的 `NonFiniteLength`。本刀不引入 `LimitExceeded`。

`paint` 不调用 `set_document_info`，不画页眉页脚，不调用 `fill_path` / `stroke_path` / `draw_image` / `annotate_link`。空白页只有 `StartPage` 与 `FinishPage`。

`PageBody` 不预留 header 槽。装饰刀在分页完成之后另画，不得改 `PlacedLine` 的原点，也不得把装饰塞进 `FlowItem`。

### 2.6 错误变体

只在 `src/error.rs` 增加无字段变体，放在 `UnbreakableWiderThanFrame` 旁边：

```rust
/// A line box or keep-together group is taller than the full body frame.
UnsplittableTallerThanFrame,
```

`Display`：`content is taller than the frame`。

不增加 `LimitExceeded`、`InvalidCellContent`、`DecorationTallerThanMargin`。组内 `PageBreak` 复用已有的 `InvalidGeometry`。

---

## 3. 模块地图

依赖只向下。`layout` 不 `use krilla`。

```text
geom, error, canvas::GlyphRun, fonts::FontStore, text::{cluster, measure, line_break}
        │
        ▼
layout::paragraph     layout_paragraph → ParagraphLayout / LineBox
        │
        ▼
layout::flow          Source, lay_out → LaidFlow / FlowItem / KeptPiece
        │
        ▼
layout::paginate      PageCursor, paginate → Vec<PageBody>, paint → dyn Canvas
```

| 文件 | 本刀 |
| --- | --- |
| `src/layout/paragraph.rs` | 实现阶段 1 的段落。模块注释改为「在 `W` 上排出自有行盒」 |
| `src/layout/flow.rs` | 替换现在的「consume-self `split`」模块注释。实现 `Source` 与 `lay_out`。没有 `FlowNode::split` |
| `src/layout/paginate.rs` | 游标、`paginate`、`paint` |
| `src/layout/mod.rs` | 模块注释里的 “split/draw flowables” 改成两阶段。不新增子模块 |
| `src/error.rs` | 增加 `UnsplittableTallerThanFrame` |
| `src/layout/lower.rs` | 保持空占位 |
| `src/layout/table.rs`、`decorate.rs`、`dump.rs` | 不动 |
| `src/document.rs` | 不增加公共 builder |
| `src/text/*`、`src/fonts.rs`、`src/canvas.rs` | 只调用，不改断行、测量、画布语义 |
| `src/lib.rs` | 不新加重导出 |

`LineBox` 定义在 `paragraph.rs`。`flow` 调用 `layout_paragraph` 再包进 `FlowItem`。`paginate` 只依赖 `flow` 的 item，不依赖 `ClusteredText`。

测试分布：

- `paragraph.rs`：局部原点、字形 id / `byte_range` 与 `measure_text` 一致、空行高度、`UnbreakableWiderThanFrame` 原样上浮。
- `paginate.rs`：`RecordingCanvas` 上的页面坐标、页数、四条 PageBreak 轨迹、段后距、spacer、keep。游标的纯高度用例可以用空 `glyphs` 的合成 `LineBox`（`FontId::from_raw(0)`，高度由测试指定），不经过 skrifa。基线与对齐的用例走夹具字体。

---

## 4. 理由

### 4.1 本刀的垂直语法用高度就够

Knife 6 要排的东西是：按行切开的段落、可丢弃的段后距、可按高度切开的 spacer、相对整框原子的 keepTogether、三态 pageBreak。水平方向的 `W`、簇、advance、断点、对齐在进入某一页之前已经确定，而且换页不改变 `W`（提案书 §2.1）。

阶段 1 把这些决定写进行盒。阶段 2 的匹配臂只做高度算术和三态转移。出页只做平移。测量函数不会在分页循环里被调用，测试可以把「夹具字形坐标」和「合成高度的页数」拆开。

段落跨页是行盒序列上的下标，不是一个重新持有原文的尾节点。`break_lines` 的结果借用 `ClusteredText`，所以阶段 1 必须在源文本还活着的时候把 `GlyphRun` 拷出来。拷贝之后分页器与字体存储无关。

`Keep` 的高度在阶段 1 就算完。阶段 2 拿它和 `H`、`remain` 比。同一组行盒放进更高的框不必重测；这只在 `W` 不变时成立，所以 `LaidFlow.width` 与正文框宽不一致要报错。

### 4.2 为什么这比消耗自身的 `split` 更适合本刀

设计稿 PR8/PR9 的 `split(self) -> Fit | Split { head, tail } | MoveToNextPage` 是为了服务「尾节点的类型要看这一页放进了什么」：表的剩余行、栏的 head/tail、列表标记只在首页。那些节点不在本刀。

若本刀就采用那套枚举，段落的 `tail` 里最终仍是已经断好的行，`FragmentCommand` 在本刀只会有 `Glyphs` 一臂活着，`Fill` / `Stroke` / `Image` / `Link` 没有构造点。Spacer 与段后距没有命令。换页标记按提案书 §2.9 本来就不走 `split`。`natural_height` 再对整框 `split` 一次，只是为了在消耗语义下重算 keep 的高度；两阶段里这个高度就是 `kept_height`。

因此本刀的分页器匹配 `FlowItem`，绘制匹配 `GlyphRun`。不引入命令枚举，等表或分隔线真的要描边时再给那个 item 自己的绘制数据。

行为仍按提案书和设计稿里已经被 §2.4 点名的规则，变的是承载方式：

| 行为 | 落点 |
| --- | --- |
| 段后距放不下则丢，不留余数，不为它开页 | 组外的 `Gap`。组内距是 `Advance`，不经过 `Gap` 臂 |
| 用户 spacer 可拆，余量可以独占下一页；高度 0 也是内容 | `Spacer` + `carry` |
| keep 的自然高含孩子之间的距、不含最后一个孩子的距；高于整框则报错；高于剩余则整组到下一页 | `Keep` 与它后面的那个 `Gap` |
| PageBreak 三态、四条轨迹 | `PageCursor` |
| 空文档一页 | `Fresh` 结束 |
| 基线 = 行顶 + `line_metrics.baseline` | 局部 y 加出页平移 |
| `Start` / `Center` / `End`，无 `Justify` | 阶段 1 写入局部 x |

`Stack` 在展开时消失。本刀的 stack 不改变 `W`，也没有自己的分页规则，分页器不需要看见它。

### 4.3 不采用的做法

- **`FlowNode::split` + `FragmentCommand`。** 见 4.2。本刀若先落这套，表和栏还没出现，枚举与消耗语义就已经定死。
- **公共 `Flowable` 或 `document` builder。** 提案书 §2 的对外签名仍不应在本刀出现。`document.rs` 继续空着。
- **分页循环里调用 `break_lines`。** 会把 `W` 藏进游标，换页与断行耦在一起，栏宽变化时更难看出要重排。
- **把 `BrokenLines` 存在 item 里。** 它借用 `ClusteredText`，活不过 `layout_paragraph` 的栈帧。
- **以整段为一个片段、段内 y 预先烘焙，分页再切窗口。** 也是两阶段，但段后距又回到段落对象上，游标得同时理解「段内窗口」和「距」。行盒使行、距、spacer、组高走同一种加法。
- **拆 spacer 时改写出一个更矮的尾 `Source`。** 那是消耗自身的 `Split { tail }`。余量用 `carry: Option<f32>` 即可。
- **用布尔表示 PageBreak。** 提案书 §2.9 已否决。开头 break、尾随一个 break、连续两个 break 在布尔上会叠成同一种读法。
- **在 `lay_out` 里根据 `H` 判 `UnsplittableTallerThanFrame`。** `H` 属于分页。行盒可以放进另一个高度的框。
- **现在就加命令枚举「以便以后画表」。** 表的描边、重复表头还没有 item。提前加枚举只会让段落路径多一层匹配。

### 4.4 以后做 table / columns 的风险

提案书优先于设计稿。§2.5 的栏是**可拆**的：各列在本页剩余高上切分，head 留在本页，tail 成为下一页的 columns。设计稿 PR15「整组移到下一页」与此冲突，本包按提案书估计风险，不按那个简化。

**栏。** 本刀的 `lay_out` 对整棵树使用一个 `W`。栏内段落的 `W` 是列宽，而且同一页上有多列、每列自己的剩余高度。扁平游标不能同时推进列 A 和列 B。

以后的接法：`layout_paragraph` 保持「给定一个 `W` 排出行盒」。进入一栏时用该列宽重新 `lay_out` 该列的 `Source`，再跑一个子游标。文档级那一条 `FlowItem` 列表不是最终 IR。若实现时把行盒缓存在 `Source` 上、默认 `W` 永不再变，栏会画出按正文宽断好的行。`LaidFlow.width` 那道检查只挡住「同一次分页用错框宽」，挡不住「忘了按列重排」。

栏内再套 stack 只是该列序列的拼接，现有展开还能用。栏的 tail 是「每列还没放下的行盒」，不是一个 `KeptPiece`。不要为了栏把 `FragmentCommand` 加回来；缺的是框栈，不是命令枚举。

**keep 包住将来的栏。** 组高是各列高度的最大值，不是孩子高度之和。`kept_height` 的求和只对垂直零件成立。栏规划器必须自己给出组高，再把这个数交给同一个「高于 `H` 报错、高于剩余则整组下移」的判断。不能把栏 children 丢进现在的 `Keep(Vec<KeptPiece>)` 后复用求和。

**表。** 行高依赖单元格在「列宽 − padding − 边框」下的阶段 1。分页单位是行，不是行盒。重复表头要把同一批行盒再画一次：自有 `LineBox` 可以克隆重放，这一点比消耗掉的 `split` 尾节点好用。风险在绘制负载。单元格边框是 `stroke_path`，不是 `GlyphRun`。应当新增表自己的 item（行高 + 若干已经定位的行盒与描边），由出页时多调用一种画布方法。禁止把描边塞进 `GlyphRun`，也禁止为此把本刀的段落路径改成 `FragmentCommand`。

行的原子性与现在的 `Keep` 类似，但 tail 是剩余行，续页还可能先放表头。那是表规划器在调用高度游标之前要发出的序列。开启重复表头时，表头高加一行正文高大于 `H` 仍是 `UnsplittableTallerThanFrame`，否则续页会空转。这个错误变体本刀已经加上，表刀可以复用，不必改段落。

单元格内禁止 `PageBreak`（以及提案书里的 columns、keep_together）是 `InvalidCellContent`，本刀不增加该变体。现在组内 `PageBreak` 用的 `InvalidGeometry` 不要直接当成格内错误。

**列表（本刀不做，只记一条以免行盒定死）。** 标记只在该项首页，续页不重复标记，正文左缘不变。标记应是列表阶段 1 写在第一行旁边的绘制数据，续行的 x 在更窄的 `W` 上已经对齐。不要指望分页游标知道「这是第几页的列表项」。

这些限制都落在阶段 1 的调用边界上：谁知道 `W`，谁调用 `lay_out`。高度游标保持不懂簇、不懂列、不懂单元格。
