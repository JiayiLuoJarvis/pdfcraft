# pdfcraft-pdf §12.2 中英拆分规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-16 |
| 状态 | 已写入 [`提案书.md`](提案书.md) §12.2。与提案书冲突时以提案书为准 |
| 范围 | 只描述产品行为。实现在 `pdfcraft-pdf` |
| 相关 | 提案书 §12.1（行内样式）。本文把下划线、删除线的**粗细**改为固定 1pt，位置仍读字体。§12.3、§12.4 本次不做 |

## 1. 产品一句话

调用方显式调用 `InlineRun::split_latin` 时，库按本文的字符表把一段字符串切成多个普通 `InlineRun::Text`。不调用这个函数，库不拆字体。

粗体、常规体、斜体不在这个函数里配对。库没有单独的细体槽；常规体就是 `FontFamily.regular`。

## 2. 公共函数

```rust
impl<K: Clone> InlineRun<K> {
    pub fn split_latin(text: impl AsRef<str>, cjk: K, latin: K) -> Vec<Self>;
}
```

- `cjk` 是字符表里不归拉丁类的那些字所用的字体键。名字沿用调用方的说法；这些字不限于汉字。
- `latin` 是拉丁类字符所用的字体键。
- 返回值里每一项都是 `InlineRun::Text`。`style` 为 `RunStyle::default()`，七个字段都是 `None`。
- 空字符串返回空 `Vec`。
- 连续分到同一个键的字符合并成一个 run。两个键是同一个值时，整段是一个 run。
- 函数不查 `FontRegistry`，不读字体文件，不报缺字。缺字仍在 `layout_spec` 里按该 run 的键报告。

调用方把返回的 `Vec` 放进 `ParagraphSpec.runs`，或再改其中某一项的 `style`、`font`。函数不生成 `InlineRun::Link` 或 `Blank`。

页眉、页脚、单元格短文本、图层文字、矢量块文字、列表编号和圆点没有这个函数的调用约定，库不替它们拆。

## 3. 字符表

切分单位是扩展字素簇（与排版用的同一套 UAX #29 切分）。簇的归类看它的第一个标量，簇内其余标量跟着走，不拆开。

第一个标量属于拉丁类时，整簇用 `latin`。拉丁类是：

- U+0020 空格
- U+0021 至 U+007E（ASCII 可见字符，含数字、ASCII 引号、连字符）
- Latin-1 Supplement（U+00C0–U+00FF）里 `char::is_alphabetic` 为真的字符。即 À–Ö、Ø–ö、ø–ÿ
- 下列块里 `char::is_alphabetic` 为真的字符：
  - Latin Extended-A（U+0100–U+017F）
  - Latin Extended-B（U+0180–U+024F）
  - Latin Extended Additional（U+1E00–U+1EFF）
  - Latin Extended-C（U+2C60–U+2C7F）
  - Latin Extended-D（U+A720–U+A7FF）
  - Latin Extended-E（U+AB30–U+AB6F）

其余标量整簇用 `cjk`。包括汉字、假名、谚文、全角数字、全角拉丁字母、弯引号（U+2018、U+2019、U+201C、U+201D）、破折号（U+2013、U+2014）、省略号（U+2026）、Latin-1 里的非字母（°、×、÷、¥、·、NBSP）、IPA 扩展（U+0250–U+02AF）、修饰字母（U+02B0–U+02FF）、换行（U+000A、U+000D）。

换行留在它分到的那个 run 的字符串里，由现有断行处理。`"A\nB"` 得到三个 run：`"A"`（`latin`）、`"\n"`（`cjk`）、`"B"`（`latin`）。

例子：

| 输入 | 结果 |
| --- | --- |
| `合同金额 USD 100` | `"合同金额"` 用 `cjk`，`" USD 100"` 用 `latin` |
| `A中B` | `"A"` 用 `latin`，`"中"` 用 `cjk`，`"B"` 用 `latin` |
| `e` + U+0301 | 一个 run，用 `latin` |
| `“ABC”` | `"“"` 用 `cjk`，`"ABC"` 用 `latin`，`"”"` 用 `cjk` |

每个 run 只在自己的键上查字。该键的 cmap 没有这个字时，错误是现有的 `MissingGlyph`，不去另一个键再找。

## 4. 粗体与斜体

`split_latin` 不读取、不设置粗体或斜体。

§12.1 保持不变：手写 run 上的 `style.bold`、`style.italic` 只在**这一个** `font` 键的 `FontFamily` 里选择 `bold`、`italic` 或 `bold_italic`。没注册对应文件时，错误仍是不带字段的 `BoldFaceRequired`、`ItalicFaceRequired`、`BoldItalicFaceRequired`。中文键的粗体文件和拉丁键的粗体文件没有配对。

返回的 run 是 `bold: None`、`italic: None`。外层 `Block::Styled` 若把粗体打开，这些 run 按 §12.1 的继承规则继承粗体，然后各自用自己的键选文件。某一段不要这份继承时，调用方在返回的 vec 里把该项写成 `bold: Some(false)` 或 `italic: Some(false)`。

## 5. 装饰线

下划线和删除线的粗细固定为 1pt，不读字体，不随字号缩放。这和填空线、横线的默认粗细相同。调用方不能单独改这个 1pt。

线的位置仍读该 run 自己家族常规面：下划线读 `post` 的偏移，删除线读 `OS/2` 的偏移，按该 run 的字号缩放。字体没给出有限的偏移时，错误仍是 `MissingDecorationMetrics`。字体里的粗细字段不再使用；粗细为 0 但偏移有限时，仍按 1pt 来画。

同一行内同字号、同颜色的装饰段画成一条线。粗细都是 1pt。线的位置取这一条的第一段。

## 6. 行盒

同一行画到多个字体文件时，每个文件按它在该行上的字号缩放。行盒取这些文件里最大的上伸部、最深的下伸部、最大的行距。三者合成自然行高：`上伸部 − 下伸部 + 行距`（下伸部在字体里为负，相减后是向下的深度）。自然行高再与「该行最大字号 × 段落行高因子」比较，取较大值。基线放在最大上伸部处。字号不按另一个文件的 em 框缩放。

这一行没有字形、只有 `Blank`，但本段还有文字 run 时：用第一个文字 run 解析之后的那个字体文件（含该 run 自己的粗体或斜体选择），字号用段落字号。

整段没有任何文字 run、只有 `Blank` 时：不读字体。行高等于段落字号乘以行高因子。基线在行盒顶部向下一个段落字号处。填空线相对基线的位置仍用 `BlankSpec.baseline_offset`（默认 1pt）。整段在上下方向的位置由调用方用 `Block::Spacer` 调节。列间距 `ColumnsSpec.gap` 不参与。

列表编号和圆点仍只用注册顺序里的第一个字体文件，不调用 `split_latin`。

## 7. 本次不做

- 不改 `FontFamily` 的四槽形状，不加拉丁组字段。
- 不在页眉、页脚、单元格、图层、矢量块、列表标记里自动拆分。
- 不做 §12.3 两端对齐，不做 §12.4 锚点、文内跳转、书签。
- 不改 §12.3、§12.4。本文采纳后，再改提案书：替换 §12.2，以及 §4.1、文末总表里「族内拉丁字面组」的句子；§12.1 里「下划线与删除线的粗细取字体」改为固定 1pt。
