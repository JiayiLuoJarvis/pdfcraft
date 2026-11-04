# 段落与行内用法指南（调用方）

本文只讲正文流里的 `Block::Paragraph` / `ParagraphSpec`、`Span`，以及常一起用的
`Block::Styled`。字段对照见 [migrate-0.6.md](migrate-0.6.md)。完整字段与错误面见
[guide.md](guide.md) 与 [docs.rs](https://docs.rs/pdfcraft-pdf)。

目标：用一份可复用的 `ParagraphStyle` 定段落层外观，用 `Span` 拼字与链接，
而不是在每个 span 上重复写字号和颜色。

下文代码里字体键用 `()`，和 guide 一样。正式项目里换成你的 `K`。

版式在 `ParagraphStyle` 上。行内片段在 `children: Vec<Span<K>>`。
快捷构造 `Block::paragraph(font, text)` 会生成单 span 段落，样式走默认。

---

## 强烈建议

| 做 | 不要 |
| --- | --- |
| 整段共用的粗体 / 字号 / 颜色写在 `ParagraphStyle.text` 或外层 `Styled` | 每个 `Span::Text` 重复抄同一份 `RunStyle` |
| 行高用 `LineHeight::new(factor)?` | 直接塞非有限 / ≤0 的 `f32`（构造就会失败） |
| 混排字体用多个 `Span::Text` 或 `Span::split_latin` | 指望一个 span 自动跨字体回退 |
| 段内链接用 `Span::Link` | 用 `Block::Anchor` 当「可点的词」（那是目标锚点，不是行内链） |
| 整块区域统一加粗用外层 `Block::Styled` | 给表格每个单元格段落手写 `bold: Some(true)` |
| 字面量写 `..ParagraphStyle::default()` | 漏字段导致无法编译 |
| 缺字 / 缺粗体面当成整份失败处理 | 指望静默换成别的面 |

---

## 心智模型

```text
文档种子（约 12 pt、黑、正体、无下划线/删除线、无字距）
    │
    ▼
外层 Block::Styled（可多层叠）
    │
    ▼
ParagraphStyle.text（段落层 RunStyle，覆盖 Styled）
    │
    ▼
每个 Span 自己的 style（最内层覆盖）
```

```text
ParagraphSpec
├─ style: ParagraphStyle
│     text / align / first_line_indent / line_height
└─ children: Vec<Span>
      Text { text, font, style }
      Link { target, children }
      Blank(...)
```

- `None` 表示继承上一层；`Some(false)` 表示关掉继承来的开关（例如粗体）。
- 列表标记只继承字号和颜色，且永远用正体面。详见 [lists.md](lists.md)。
- 页眉页脚与图层文字用 `PageText`，从种子起步，**不**吃正文里的 `Styled`。

---

## 最小段落

```rust
use pdfcraft_pdf::spec::{Block, ParagraphSpec, ParagraphStyle, RunStyle, Span};
use pdfcraft_pdf::Positive;

fn hello() -> Block<()> {
    Block::Paragraph(ParagraphSpec {
        style: ParagraphStyle {
            text: RunStyle {
                font_size: Some(Positive::pt(12.0).unwrap()),
                ..RunStyle::default()
            },
            ..ParagraphStyle::default()
        },
        children: vec![Span::Text {
            text: "你好，pdfcraft-pdf。".into(),
            font: (),
            style: RunStyle::default(),
        }],
    })
}

// 等价快捷写法（样式全默认）：
fn hello_short() -> Block<()> {
    Block::paragraph((), "你好，pdfcraft-pdf。")
}
```

---

## ParagraphStyle 字段

| 字段 | 含义 | 默认 |
| --- | --- | --- |
| `text` | 段落层 `RunStyle`（字号、颜色、粗斜体、下划线、删除线、字距） | `RunStyle::default()`（全 `None`，吃上层） |
| `align` | 行对齐 | `Alignment::Start` |
| `first_line_indent` | 首行缩进 | `0` |
| `line_height` | 行高因子；`None` 用文档种子 `1.2` | `None` |

```rust
use pdfcraft_pdf::spec::{Alignment, LineHeight, ParagraphStyle, RunStyle};
use pdfcraft_pdf::{Color, NonNegative, Positive};

let style = ParagraphStyle {
    text: RunStyle {
        font_size: Some(Positive::pt(11.0).unwrap()),
        color: Some(Color::rgb(0, 0, 0)),
        bold: Some(true),
        ..RunStyle::default()
    },
    align: Alignment::Justify,
    first_line_indent: NonNegative::pt(21.0).unwrap(),
    line_height: Some(LineHeight::new(1.5).unwrap()),
};
```

`LineHeight::new` 拒绝 NaN、无穷、0、负数。错误是
`GeometryKind::LineHeightFactor { factor }`。合法因子下的排版高度与改名前一致。

---

## 多 span：强调、填空、链接

```rust
use pdfcraft_pdf::spec::{
    Alignment, BlankSpec, BlankWidth, ParagraphSpec, ParagraphStyle, RunStyle, Span,
};
use pdfcraft_pdf::{Color, LinkTarget, NonNegative, Positive};

let paragraph = ParagraphSpec {
    style: ParagraphStyle {
        align: Alignment::Start,
        ..ParagraphStyle::default()
    },
    children: vec![
        Span::Text {
            text: "应付金额：".into(),
            font: (),
            style: RunStyle {
                bold: Some(true),
                ..RunStyle::default()
            },
        },
        Span::Blank(BlankSpec {
            width: BlankWidth::Em(8),
            thickness: Some(Positive::pt(0.75).unwrap()),
            color: Some(Color::rgb(0, 0, 0)),
            baseline_offset: Some(NonNegative::ZERO),
        }),
        Span::Text {
            text: "。详见".into(),
            font: (),
            style: RunStyle::default(),
        },
        Span::Link {
            target: LinkTarget::uri("https://example.test/pay").unwrap(),
            children: vec![Span::Text {
                text: "付款条款".into(),
                font: (),
                style: RunStyle {
                    underline: Some(true),
                    ..RunStyle::default()
                },
            }],
        },
        Span::Text {
            text: "。".into(),
            font: (),
            style: RunStyle::default(),
        },
    ],
};
```

| `Span` | 要点 |
| --- | --- |
| `Text` | `{ text, font, style }`。空字符串跳过，不读字体。 |
| `Link` | `target` 是 `Uri` 或 `Anchor`。空 URI → `EmptyLink`；空锚点 id → `EmptyAnchorId`。 |
| `Blank` | `Fixed` / `Em` / `ToEndOfLine`。`Em` 相对**段落**解析后的字号。 |

`tracking` 加在簇与簇之间，计入断行宽度。过负会报 `ErrorKind::NegativeTracking`。

---

## 样式叠放与 `Block::Styled`

外层 `Styled` 适合包住一节、一张表或一组段落，让里面的段落一起变粗或变色。

```rust
use pdfcraft_pdf::spec::{Block, RunStyle};

Block::Styled {
    style: RunStyle {
        bold: Some(true),
        ..RunStyle::default()
    },
    children: vec![
        Block::paragraph((), "本节标题与正文都走粗体面。"),
        Block::paragraph((), "除非某个 Span 写 bold: Some(false)。"),
    ],
}
```

顺序再次强调：种子 → 每一层 `Styled` → `ParagraphStyle.text` → span。
表格单元格里的段落会继承**包围该表的** `Styled` 的全部字段（0.6 行为变化）。
列表标记仍然只要字号和颜色。

粗体 / 斜体需要 `FontFamily` 对应槽位。缺面报 `BoldFaceRequired` 或同类错误，
整份失败，不静默回退。

---

## 中英混排与 `split_latin`

每个 `Span::Text` 只在自己的 `font` 键上查字。缺字是 `MissingGlyph`，不会去另一个键找。

调用方可以用显式多个 span，也可以调用 `Span::split_latin`，按字符表切成拉丁与其它段，
并为两类各指定一个键。不调用该函数时，库不会按字符自动拆字体。

行盒高度：同一行各字体按**各自字号**取最大上伸、最深下伸、最大行距，再与
「最大字号 × 行高因子」取较大值。纯空白行用第一个文字 span 解析后面在段落字号下的面。

---

## 对齐

| `Alignment` | 行为摘要 |
| --- | --- |
| `Start` / `Center` / `End` | 常规行对齐 |
| `Justify` | 只影响放置，不断行。先拉宽 U+0020（≤0.5em），再均分到可拉伸的簇间隙。末行、硬换行行、`ToEndOfLine` 空白行等保持 Start 形 |

图片、`HLine`、canvas 把 `Justify` 当成 `Start`。

---

## 常见翻车

| 现象 | 原因 | 做法 |
| --- | --- | --- |
| 编译不过，缺字段 | `ParagraphStyle` / `RunStyle` 字面量未写全 | `..Default::default()` |
| `LineHeightFactor` | `LineHeight::new` 收到非法因子 | 用有限且 >0 的值 |
| `BoldFaceRequired` | 开了粗体但没注册粗体面 | 填 `FontFamily.bold`，或不要 `bold: Some(true)` |
| `MissingGlyph` | 当前键的字体没有该字 | 换字体键，或拆 span |
| 链接点了没跳到段内词 | 用了 `Block::Anchor` 当行内链 | 行内用 `Span::Link`；`Anchor` 是目标 |
| 页眉文字跟着正文 `Styled` 变粗 | 带文字不吃正文 `Styled` | 在 `BandSlot.style` 里单独设 |

---

## 明确不做（请别设计成依赖）

- 自动字体回退链（一个 span 跨多个家族找字）
- kerning、连字、bidi、阿文 / 印度系 shaping、竖排
- 行内嵌套 `Block`（段落孩子只能是 `Span`）
- 把 `PageText` 当正文 span 用

需要条款编号、悬挂缩进时用 [lists.md](lists.md)。需要表格单元格里的段落规则见
[tables.md](tables.md)。

---

## 相关文档

| 文档 | 内容 |
| --- | --- |
| [guide.md](guide.md) | 全库用法。Paragraphs、Links、Limits |
| [tables.md](tables.md) | 表格与单元格（格内段落继承） |
| [lists.md](lists.md) | 列表与 Inset |
| [migrate-0.6.md](migrate-0.6.md) | `0.5` 到 `0.6` 字段对照 |
| [migrate-inline.md](migrate-inline.md) | 更早的行内样式迁移史 |
| [crate CHANGELOG](../CHANGELOG.md) | `0.6.0` Breaking / Added / Changed |
| 提案书 §12.1、§12.2、§12.3、§12.7 | 已确认产品口径（仓库 `docs/提案书.md`） |
| `examples/hello.rs` | 段落、链接、列表混排样例 |
