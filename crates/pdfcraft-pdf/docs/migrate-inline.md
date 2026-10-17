# 迁到行内样式

这份说明给还在用当前已提交 API 的调用方。对照的是仓库 `HEAD`（`FontFamily.normal`、`InlineRun::Bold`、`TextStyle`）和 `sec12` 工作树上的未发布改动。版本号仍是 `0.1.0`。`pdfcraft-forms` 和 `pdfcraft-orgchart` 的类型不变。

按下面的顺序改。每一步都对应一类编译错误。改完再看文末的排版变化。那些变化能编译通过，但 PDF 上的线宽和行高会不一样。

## 1. 改 `FontFamily`

`normal` 改名为 `regular`。结构体新增 `italic` 和 `bold_italic`。不用斜体时写成 `None`。

以前：

```rust
FontFamily {
    normal: FontFace::new(body, 0),
    bold: Some(FontFace::new(bold, 0)),
}
```

现在：

```rust
FontFamily {
    regular: FontFace::new(body, 0),
    bold: Some(FontFace::new(bold, 0)),
    italic: None,
    bold_italic: None,
}
```

`FontRegistry::single` 和 `Resources::with_fonts` 的用法不变。

## 2. 删掉 `InlineRun::Bold`

粗体不再是单独的 run。把它写成 `InlineRun::Text`，并把 `style.bold` 设为 `Some(true)`。

以前：

```rust
InlineRun::Bold { text: "合计".into(), font: DocFont::Body }
```

现在：

```rust
InlineRun::Text {
    text: "合计".into(),
    font: DocFont::Body,
    style: RunStyle {
        bold: Some(true),
        ..RunStyle::default()
    },
}
```

这个 run 只在 `DocFont::Body` 这一套 `FontFamily` 里找 `bold`。另一把键的粗体文件不会顶上。没有 `bold` 时，错误仍是 `ErrorKind::BoldFaceRequired`。

## 3. 给每个 `InlineRun::Text` 加上 `style`

以前的 `Text { text, font }` 少一个字段。不改外观时用 `RunStyle::default()`。`ParagraphSpec::text(font, text)` 已经带上默认样式，调用它的代码不用改。

```rust
InlineRun::Text {
    text: "正文".into(),
    font: DocFont::Body,
    style: RunStyle::default(),
}
```

`InlineRun::Link` 和 `InlineRun::Blank` 的字段不变。单元格短文本 `CellBlock::Text { text, font }` 也不变。单元格里要粗体、下划线或字号时，改放 `CellBlock::Paragraph`。

## 4. 把 `TextStyle` 换成 `RunStyle`

`Block::Styled`、`CanvasMark::Text`、`LayerMark::Text` 的 `style` 类型从 `TextStyle` 换成 `RunStyle`。`BandSlot::Text` 和 `BandSlot::Link` 新增必填字段 `style`。

`RunStyle` 的字段都是 `Option`。`None` 继承外层。`Some(false)` 关掉外层打开的粗体、斜体、下划线或删除线。

以前：

```rust
Block::Styled {
    style: TextStyle {
        font_size: Some(Length::pt(14.0)?),
        color: None,
    },
    children,
}
```

现在：

```rust
Block::Styled {
    style: RunStyle {
        font_size: Some(Length::pt(14.0)?),
        ..RunStyle::default()
    },
    children,
}
```

页眉文字补上 `style`：

```rust
BandSlot::Text {
    runs: vec![TextRun::PageNumber],
    font: DocFont::Body,
    style: RunStyle::default(),
}
```

`BandSlot::Link` 同样加上 `style: RunStyle::default()`。

继承范围：

| 位置 | 继承什么 |
| --- | --- |
| 段落里的 `InlineRun::Text` | 外层 `Block::Styled` 的全部字段，再被段落的 `color`、`font_size` 盖住，最后被 run 自己的 `style` 盖住 |
| 单元格短文本、列表编号和圆点 | 只继承字号和颜色，不用粗体、斜体、下划线和删除线 |
| 页眉、页脚、图层文字、矢量块文字 | 不看外层 `Block::Styled`。从 12 pt、黑色、非粗、非斜开始，只叠自己的 `style` |

## 5. 需要新字面时再注册

只在 run 上把 `italic` 设为 `Some(true)` 时，同一把键的 `FontFamily.italic` 必须有文件。粗体和斜体同时打开时，要的是 `bold_italic`，不是把 `bold` 和 `italic` 两个文件叠在一起。缺文件时整份失败，错误分别是 `ItalicFaceRequired` 和 `BoldItalicFaceRequired`。库不做伪斜体，也不改用常规面冒充成功。

字距写在 `RunStyle.tracking`，单位是 pt，可以为负。它加在本行相邻字素簇之间，行末不加。加上之后某个字形的前进宽度小于 0 时，错误是 `NegativeTracking { advance, tracking }`。

## 6. 中英要分成两套字体时调用 `split_latin`

库不会按字符自动换字体。要拆时自己调用，再把返回值放进 `ParagraphSpec.runs`。

```rust
let mut runs = InlineRun::split_latin("合同金额 USD 100", DocFont::Cjk, DocFont::Latin);
if let InlineRun::Text { style, .. } = &mut runs[1] {
    style.underline = Some(true);
}
```

约束是 `K: Clone + PartialEq`。已经用于 `FontRegistry` 的键都满足。返回值全是 `InlineRun::Text`，`style` 全是 `None`，所以仍会继承外层 `Block::Styled`。某一段不要继承粗体时，把该项写成 `bold: Some(false)`。

不调用这个函数时，整段继续用你写在 run 上的那一把键。页眉、页脚、单元格短文本、图层、矢量块和列表标记没有对应的拆分函数。

## 改完后会变的版式

这些不用改调用代码，但生成的 PDF 和以前不同。

下划线和删除线的粗细固定为 1pt，不随字号变化，也不再读字体表里的粗细。位置仍来自该 run 那一套家族的常规面。下划线读 `post`，删除线读 `OS/2`，再按该 run 的字号缩放。偏移缺失或不是有限数时，错误是 `MissingDecorationMetrics`。表里粗细为 0 但偏移有限时，仍画 1pt。同一行里字号和颜色相同的相邻装饰段画成一条线，位置取第一段。

一行里有多个字体或多种字号时，每个字体按它自己在这一行上的字号缩放。行高取这些字体里最大的上伸部、最深的下伸部、最大的行距，再和「该行最大字号 × 段落行高因子」比，取较大的那个。基线放在最大上伸部。

整段只有 `Blank`、没有任何文字 run 时，库不读字体。行高等于段落字号乘以行高因子。基线在行盒顶部向下一个段落字号处。这段的上下位置用 `Block::Spacer` 调。
