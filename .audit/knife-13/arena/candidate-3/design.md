# Knife 13：段内粗体 run + 段落首行缩进

`Paragraph` 闭包就是这一刀的样式建造器。调用方多两个段落方法和一个文档级粗体注册；一段仍然降低成一个块。粗体是另一个已注册 `FontId`，在测量时进入 cluster 宽度。首行缩进是该段已解析 `Style` 上的非负长度，只改第一视觉行的断行宽和 glyph 的 x。

## Usage

现有 `document` / `paragraph` / `text` 调用继续编译。`font` 仍返回 `Result<()>`，第一张注册成功的 face 仍是正文 `FontId(0)`。粗体必须在至少一次成功的 `font` 之后用 `bold_font` 注册。缩进默认 0；调用方传入 `Length<NonNegative>`，负值在 `NonNegative::pt` / `mm` 构造期就被拒掉。

```rust
use quire_pdf::{document, NonNegative, PageSize};

let pdf = document(PageSize::A4, |doc| {
    doc.font(regular_bytes)?;
    doc.bold_font(bold_bytes)?;

    doc.paragraph(|p| {
        p.text("合同编号 ");
        p.bold(&contract.id);
    });

    doc.paragraph(|p| {
        p.first_line_indent(NonNegative::pt(24.0)?);
        p.text("正文从第一行缩进，折行后顶到框的左缘。");
    });

    Ok(())
})?;
```

栏和列表项已经是 `Flow`，同一对方法在内层段落上生效。缩进相对的 `W` 是该列或该列表项的内容宽。

```rust
doc.columns(|row| {
    row.col(|c| {
        c.paragraph(|p| {
            p.first_line_indent(NonNegative::pt(24.0)?);
            p.text("甲方 ");
            p.bold(&contract.party_a);
        });
    });
});

doc.ul(|list| {
    list.item(|flow| {
        flow.paragraph(|p| {
            p.text("条款 ");
            p.bold("甲");
        });
    });
});
```

同一段里可以多次交替。相邻的同一种 face 在建造期合成一个 run。空字符串不产生 run。后一次 `first_line_indent` 覆盖前一次。

```rust
doc.paragraph(|p| {
    p.first_line_indent(NonNegative::ZERO);
    p.text("见");
    p.bold("附件");
    p.text(" A");
    p.first_line_indent(NonNegative::mm(8.0)?);
});
```

单元格、页眉页脚的 `left` / `right`、`behind` / `front` 的 `text_at` 这一刀仍只接受普通字符串。它们不长出 `bold` 或 `first_line_indent`。

失败都发生在 `Canvas::start_page` 之前：

| 情况 | 错误 |
| --- | --- |
| `bold_font` 时还没有正文 face | `Error::Font { index: 0, message: "body font is not registered" }`，从 `bold_font` 立刻返回 |
| 用了 `p.bold` 但从未成功 `bold_font` | `Error::UnknownFont { id: 1 }`，在 `plan_pages` 里该段第一次排版时 |
| 粗体 face 没有该标量 | `Error::MissingGlyph { font: FontId(1) 或实际粗体 id, scalar }` |
| `indent >= W`（含 `W' <= 0`） | `Error::InvalidGeometry` |
| 第一行不可断簇宽于 `W'` | `Error::UnbreakableWiderThanFrame { avail: W' }` |

`RecordingCanvas` 能读到每个 `GlyphRun` 的 `font` 和 `origin.x`。`backend-krilla` 冒烟只检查字节以 `%PDF` 开头。

## Module map

```text
src/document.rs          Paragraph 累积 run 与缩进；Document::bold_font
src/fonts.rs             FontStore 增加唯一粗体槽
src/layout/style.rs      Style.first_line_indent，默认 0
src/layout/paragraph.rs  拼接、按簇计价、多 GlyphRun、首行 x
src/text/line_break.rs   LineBudget { first, rest }
src/layout/paginate.rs   仅测试：lines[0].run 改为 runs[0]
tests/document.rs        RecordingCanvas 坐标 / FontId；缺字与过宽在开页前失败

不改 src/canvas.rs、src/backend/、src/geom.rs、src/text/measure.rs、src/text/cluster.rs
不改 src/lib.rs 的重导出
不改 src/layout/list.rs、src/layout/decorate.rs 的绘制路径
```

依赖方向不变：`document` → `layout` / `fonts`；`layout` 不知道 krilla。`Face`、`TextRun`、`LineBudget` 都是 `pub(crate)`。集成测试只能走 DSL 和 `RecordingCanvas`。

## Types and signatures

```rust
// src/fonts.rs — FontStore 是粗体槽的唯一写入者
impl FontStore {
    pub(crate) fn is_empty(&self) -> bool { /* faces.is_empty() */ }

    /// 记录粗体 face。调用方只传入刚刚 `register` 成功的 id。
    pub(crate) fn set_bold(&mut self, id: FontId) { /* self.bold = Some(id) */ }

    /// 槽为空时返回 `UnknownFont { id: 1 }`（约定中的第二张 face）。
    pub(crate) fn bold(&self) -> Result<FontId> { /* ... */ }
}

// src/document.rs
impl<'a> Document<'a> {
    /// 注册正文 face。第一张成功注册的 face 是 `FontId(0)`，正文用它。
    pub fn font(&mut self, bytes: Vec<u8>) -> Result<()> { /* 现有实现 */ }

    /// 注册粗体 face 并把它设为文档唯一的粗体槽。
    ///
    /// 必须已经有正文 face，否则 `FontId(0)` 会被粗体字节占掉。
    /// 再次调用会再注册一张 face，槽改指向最新的 id。
    pub fn bold_font(&mut self, bytes: Vec<u8>) -> Result<()> {
        if self.fonts.is_empty() {
            return Err(Error::Font {
                index: 0,
                message: "body font is not registered".to_owned(),
            });
        }
        let id = self.fonts.register(FontSource {
            bytes: Arc::new(bytes),
            index: 0,
        })?;
        self.fonts.set_bold(id);
        Ok(())
    }
}

/// 一段的建造器。字段私有。
pub struct Paragraph {
    runs: Vec<TextRun>,
    indent: Length<NonNegative>,
}

impl Default for Paragraph {
    fn default() -> Self {
        Self {
            runs: Vec::new(),
            indent: NonNegative::ZERO,
        }
    }
}

impl Paragraph {
    /// 追加正文 run。空字符串不产生 run；与上一个正文 run 合并。
    pub fn text(&mut self, s: impl AsRef<str>) { /* push(Face::Body, s) */ }

    /// 追加粗体 run。空字符串不产生 run；与上一个粗体 run 合并。
    ///
    /// 这里不查字体。槽要到这段真正排版时才读。
    pub fn bold(&mut self, s: impl AsRef<str>) { /* push(Face::Bold, s) */ }

    /// 设置这段的首行缩进。后一次调用覆盖前一次。`W` 此时还未知。
    pub fn first_line_indent(&mut self, indent: Length<NonNegative>) {
        self.indent = indent;
    }

    fn push(&mut self, face: Face, s: &str) { /* 见下方 */ }
}

impl Flow {
    pub fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph)) {
        let mut paragraph = Paragraph::default();
        f(&mut paragraph);
        let style = self
            .style
            .clone()
            .with_first_line_indent(paragraph.indent);
        self.push(Ok(FlowNode::Text(TextNode::from_runs(
            paragraph.runs,
            style,
        ))));
    }
}
```

`Flow::paragraph` 不改 `self.style`。缩进只进入这一段的 `TextNode`。栏、列表、页眉从 `Flow::child` 克隆到的样式仍是缩进 0。

```rust
// src/layout/style.rs
pub(crate) struct Style {
    pub(crate) font: FontId,
    pub(crate) font_size: Length<Positive>,
    pub(crate) line_height_factor: f32,
    pub(crate) color: Color,
    pub(crate) paragraph_spacing: Length<NonNegative>,
    pub(crate) align: TextAlign,
    /// 第一视觉行向右缩进的 pt。默认 `NonNegative::ZERO`。
    pub(crate) first_line_indent: Length<NonNegative>,
}

impl Style {
    /// 签名不增加参数。新字段在函数体内写成 `NonNegative::ZERO`。
    pub(crate) fn new(/* 现有六个参数 */) -> Result<Self> { /* ... */ }

    pub(crate) fn with_first_line_indent(mut self, indent: Length<NonNegative>) -> Self {
        self.first_line_indent = indent;
        self
    }
}
```

`default_style()` 的结构体字面量补上 `first_line_indent: NonNegative::ZERO`。`Style { align, ..style.clone() }` 这种更新语法自动带上新字段。

```rust
// src/layout/paragraph.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Face {
    Body,
    Bold,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextRun {
    pub(crate) text: String,
    pub(crate) face: Face,
}

pub(crate) struct TextNode {
    runs: Vec<TextRun>,
    style: Style,
}

impl TextNode {
    /// 单元格、页眉字符串的适配。空字符串得到空 run 列表。
    pub(crate) fn new(text: impl Into<String>, style: Style) -> Self { /* 一个 Body run 或空 */ }

    pub(crate) fn from_runs(runs: Vec<TextRun>, style: Style) -> Self {
        Self { runs, style }
    }
}

/// 一行里按字体切开的若干同质 run，坐标相对段落 fragment 原点。
pub(crate) struct LineBox {
    pub(crate) runs: Vec<GlyphRun>,
}

impl ParagraphNode {
    /// `width` 是完整框宽 `W`，存进 `ParagraphNode.width`。
    pub(crate) fn layout(
        fonts: &mut FontStore,
        runs: &[TextRun],
        style: &Style,
        width: f32,
    ) -> Result<Self> { /* 见数据流 */ }
}

/// 列表标记和 layer `text_at`。只测量 `style.font`，原点 x 仍强制为 0。
/// 不读 `first_line_indent`。
pub(crate) fn single_line_run(/* 现有签名 */) -> Result<GlyphRun> { /* 保持单 run */ }
```

```rust
// src/text/line_break.rs
/// 第一视觉行用 `first`，产出第一条 `LineRecord` 之后用 `rest`。
#[derive(Debug, Clone, Copy)]
pub(crate) struct LineBudget {
    first: f32,
    rest: f32,
}

impl LineBudget {
    pub(crate) fn uniform(width: f32) -> Self {
        Self { first: width, rest: width }
    }

    pub(crate) fn first(self) -> f32 { self.first }
    pub(crate) fn rest(self) -> f32 { self.rest }
}

pub(crate) fn break_lines<'text>(
    text: &'text ClusteredText,
    budget: LineBudget,
    width_of: impl FnMut(Cluster<'text>) -> f32,
) -> Result<BrokenLines<'text>> { /* 两个宽度都须有限且 > 0 */ }
```

`measure_text` 继续只接受一个 `FontId`。段落在外面按簇调用它，不改它的签名。`GlyphRun` 不增加粗体标志。

## Data flow

建造期只追加。读路径在排版时一次走完，中间不建 map。

```text
Paragraph.runs + indent
  → TextNode { runs, Style { font: body, first_line_indent } }
  → 知道 W 之后 ParagraphNode::layout
       1. 若存在 Face::Bold，fonts.bold() 一次
       2. 原始拼接 → ClusteredText::new（唯一的 CRLF / 字素簇规范化）
       3. 每个非 BK 簇用「簇内第一个标量所属 face」的字体 measure_text
       4. LineBudget { first: W - I, rest: W }
       5. 一行按 FontId 切成多个 GlyphRun，x 从对齐原点累加 advance
  → ParagraphNode { lines, width: W }     // x 已写入
  → 分页只切 LineBox，不重算缩进、不重断行
  → FragmentCommand::Glyphs × N
  → 全部 plan 成功之后才 start_page
```

`push`：

```rust
fn push(&mut self, face: Face, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(last) = self.runs.last_mut() {
        if last.face == face {
            last.text.push_str(s);
            return;
        }
    }
    self.runs.push(TextRun { text: s.to_owned(), face });
}
```

拼接与簇的字体。`ClusteredText::new` 仍是唯一规范化入口。角色跟着原始字节走，CRLF 合成一个 LF 时，这个 LF 算 `\n` 所在的那个 run。run 的边界一定落在标量边界上，因为每个 `text` / `bold` 参数都是完整的 `&str`。

```rust
// TODO: 原始字节角色
// raw = runs 按顺序拼接
// role_at(raw_byte) = 覆盖该字节的 TextRun.face
// clustered = ClusteredText::new(&raw)
// 规范化游标：遇到原始 "\r\n" 时两个原始字节对应规范化里的一个 '\n'，其余 1:1
// 簇的 face = role_at(该簇第一个规范化字节映射回的原始字节)
// Hard 簇不测量
```

簇被 run 边界切开时（底字在 `text`，组合附加记号在 `bold`），整簇用第一个标量的 face 测量并绘制。断行仍把该字素簇当作一个原子。

计价与预算：

```rust
struct PricedGlyph {
    glyph: MeasuredGlyph, // byte_range 已重基到整段 glyph_text
    font: FontId,
}

// 每个非 BK 簇：
//   font = body → style.font；Bold → 已经取到的 bold id
//   piece = measure_text(fonts, font, cluster 的 glyph 子串)?
//   把 piece.glyphs 的 byte_range 加上该簇在 glyph_text 中的起点
// 簇宽 = Σ glyph.advance_norm * style.font_size

let indent = style.first_line_indent.as_pt();
let first = width - indent;
if !first.is_finite() || first <= 0.0 {
    return Err(Error::InvalidGeometry);
}
let budget = LineBudget { first, rest: width };
```

`width` 在这之前已经按现有逻辑拒绝非有限和非正值。`indent` 由类型保证 ≥ 0。`pack_lines` 每次比较和每次 `UnbreakableWiderThanFrame` 都读「还没有 `LineRecord` 就用 `first`，否则用 `rest`」。行首被丢掉的空格还没产出记录，所以第一行有墨水的内容仍用 `W'`。`\n` 产出一条记录之后，下一行用 `W`，包括 `\n` 后面的空行。

行高只问 `style.font` 的 `line_metrics`。粗体 face 的 ascent / descent 不参与行盒。同一段每一行的 `line_height` 仍是一个数，分页公式不变。

一行内的 x：

```rust
fn line_origin_x(line_index: usize, indent: f32, frame_w: f32, line_w: f32, align: TextAlign) -> f32 {
    if line_index == 0 {
        indent + align_x(frame_w - indent, line_w, align)
    } else {
        align_x(frame_w, line_w, align)
    }
}

// Start  首行 x = I，其后 x = 0
// End    首行 x = W - line_w（右缘仍是框的右缘）
// Center 首行在 [I, W] 内居中
// 同一行的下一个 GlyphRun.origin.x = 上一个 origin.x + 上一个 advance 之和
// 每个 run 的 font_size、color 来自段落 Style；font 来自 PricedGlyph
// GlyphPlacement.byte_range 索引该 GlyphRun 自己的 text
```

空段（没有任何 run）仍是一条空行：一个空的正文 `GlyphRun`，`origin.x` 按上面的首行公式。`ParagraphNode.width` 保存完整 `W`。跨页 tail 复制已经写好 x 的 `LineBox`。

`fragment_from_lines` 对 `LineBox.runs` 里的每一个 run 各发一条 `FragmentCommand::Glyphs`，y 仍是 `行号 * line_height + baseline`。

## Tests

`src/text/line_break.rs` 现有用例改为 `LineBudget::uniform(avail)`，断言保持不变。另加：第一行 avail 更窄时，溢出错误里的 `avail` 是 `first`；硬断之后的下一行用 `rest`。

`tests/document.rs` 用同一份 ExtraLight fixture 注册两次，钉住管线而不是视觉粗细：

- `text("A"); bold("B")` 在一行内放下时，记录里是两个 `GlyphRun`，字体分别为 `FontId(0)` 与 `bold_font` 返回前内部的第二张 face（常规顺序下是 `FontId(1)`）。第二个 run 的 `origin.x` 等于第一个 run 的 advance。
- 首行缩进后，第一视觉行 `origin.x == I`（Start），第二视觉行 `origin.x == 0`。段内 `\n` 之后的行 `origin.x == 0`。
- 第一簇宽于 `W'`、窄于 `W` 时，错误是 `UnbreakableWiderThanFrame { avail: W' }`，记录里没有 `StartPage`。
- `indent >= W`、粗体缺字、只有 `p.bold` 没有 `bold_font`，同样没有 `StartPage`。
- 段落跨页后，下一页 fragment 的第一行沿用烘焙好的 x，不再加一次 `I`。
- 列表标记和 `text_at` 的 x 与这一刀之前相同。
- `to_pdf` 只断言 `%PDF`。

`src/layout/paginate.rs` 里 `start_center_end_x_and_baseline_y` 从 `lines[0].run` 改为对 `lines[0].runs` 求和。缩进为 0 时期望的 x 不变。
