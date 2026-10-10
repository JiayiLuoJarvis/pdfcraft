# PR7 candidate 3: sticky pieces, glyph-range lines

## Problem

Knife 5 要在已钉死的七类表和 6×6 决策表上，把一段文字变成若干行。断行原子是 UAX #29 扩展字素簇，簇宽是簇内各 scalar 名义 advance 之和；knife 4 的 `measure_text` 仍按 scalar 返回扁平 `MeasuredGlyph`，缺字仍是 `MissingGlyph`，本刀不改 `measure.rs`。`line_break` 不能碰 skrifa、krilla、分页或公共 DSL。

难处在数据形状。决策表回答的是「相邻两簇之间能不能断」。编号算法在放不下时向左找最近的允许断点，而允许断点的补集恰好是一段必须粘住的簇。若装箱循环里再查一次表，同一条规则会有两个读者。另外 `\r\n` 在 UAX #29 的 GB3 里是一个簇，簇首是 `\r`；`\r` 本身不是 `BK`。不先把 CR 折成 `\n`，换行会消失。`BK` 不绘制，但现有 cmap 路径会把 `\n` 当成缺字。测量输入因此必须是去掉强制断行之后的 scalar 串，行的输出必须能直接切 `measured.glyphs`。

## Usage (caller's view)

段落和测试都不分类、不查表、不自己拼原子。它们准备一份不可变的 `Pieces`，用 `scalars()` 去测量（或填等宽宽度），再 `wrap`。`wrap` 可对同一份 `Pieces` 用不同宽度和不同 `W` 重复调用。

等宽行为测试用每个可绘制 scalar 宽度 `1.0`（整数，比较稳定）。这对应设计稿替身「BMP scalar = 10 字体单位、upem 1000」再乘字号后的点值；生产路径才乘 `advance_norm * font_size`。

**调用点 A — 决策表行为（`好（世`、`（世界`、句号、超宽词）：**

```rust
use crate::error::Error;
use crate::text::line_break::Pieces;

let pieces = Pieces::prepare("好（世");
let widths = vec![1.0; pieces.scalars().chars().count()];
let lines = pieces.wrap(&widths, 2.0)?;
assert_eq!(lines.text(0), "好");
assert_eq!(lines.text(1), "（世");
assert_eq!(lines.lines()[0].glyphs, 0..1);
assert_eq!(lines.lines()[1].width, 2.0);

let pieces = Pieces::prepare("（世界");
let lines = pieces.wrap(&[1.0, 1.0, 1.0], 2.0)?;
assert_eq!(lines.text(0), "（世");
assert_eq!(lines.text(1), "界");

let pieces = Pieces::prepare("字。下");
let lines = pieces.wrap(&[1.0, 1.0, 1.0], 2.0)?;
assert_eq!(lines.text(0), "字。");
assert_eq!(lines.text(1), "下");

let err = Pieces::prepare("hello").wrap(&[1.0; 5], 4.0).unwrap_err();
assert_eq!(err, Error::UnbreakableWiderThanFrame { width: 5.0, avail: 4.0 });
```

`hello` 五个 `AL` 粘成一块。宽 4 时前四个放得下，第五个放不下，块内没有断点，`width` 是从行首到这个放不下的簇（含该簇）。

**调用点 B — 换行、行首空格、全角空、组合字符、`Other`：**

```rust
use crate::text::cluster::Normalized;
use crate::text::line_break::Pieces;

assert_eq!(Normalized::new("A\r\nB").as_str(), "A\nB");

let pieces = Pieces::prepare("A\r\nB");
assert_eq!(pieces.scalars(), "AB");
let lines = pieces.wrap(&[1.0, 1.0], 10.0)?;
assert_eq!(lines.len(), 2);
assert_eq!(lines.text(0), "A");
assert_eq!(lines.text(1), "B");

let pieces = Pieces::prepare("hello ");
let lines = pieces.wrap(&[1.0; 6], 5.0)?;
assert_eq!(lines.text(0), "hello");
assert_eq!(lines.lines()[1].width, 0.0);

let pieces = Pieces::prepare("  A");
let lines = pieces.wrap(&[1.0, 1.0, 1.0], 10.0)?;
assert_eq!(lines.text(0), "A");
assert_eq!(lines.lines()[0].glyphs, 2..3);

let pieces = Pieces::prepare("A\u{3000}B");
let lines = pieces.wrap(&[1.0, 1.0, 1.0], 1.0)?;
assert_eq!(lines.len(), 3);
assert_eq!(lines.text(1), "\u{3000}");

let pieces = Pieces::prepare("e\u{0301}X");
assert_eq!(pieces.scalars().chars().count(), 3);
let lines = pieces.wrap(&[1.0, 1.0, 1.0], 2.0)?;
assert_eq!(lines.text(0), "e\u{0301}");
assert_eq!(lines.lines()[0].glyphs, 0..2);

let pieces = Pieces::prepare("\u{FF21}\u{FF22}");
let err = pieces.wrap(&[1.0, 1.0], 1.0).unwrap_err();
assert!(matches!(err, Error::UnbreakableWiderThanFrame { width: 2.0, avail: 1.0 }));

let pieces = Pieces::prepare("字\u{FF21}");
let lines = pieces.wrap(&[1.0, 1.0], 1.0)?;
assert_eq!(lines.text(0), "字");
assert_eq!(lines.text(1), "\u{FF21}");

assert_eq!(Pieces::prepare("").wrap(&[], 1.0)?.len(), 1);
assert_eq!(Pieces::prepare("").wrap(&[], 1.0)?.lines()[0].width, 0.0);
```

**调用点 C — 以后的段落 flow（不在本刀实现）：**

```rust
let pieces = Pieces::prepare(paragraph_text);
let measured = measure_text(fonts, font, pieces.scalars())?;
let size = font_size.as_pt();
let widths: Vec<f32> = measured
    .glyphs
    .iter()
    .map(|glyph| glyph.advance_norm * size)
    .collect();
let lines = pieces.wrap(&widths, content_width_pt)?;

for (index, line) in lines.lines().iter().enumerate() {
    let glyphs = &measured.glyphs[line.glyphs.clone()];
    let _ink = lines.text(index);
    // PR8 再用 line.width 做 Start/Center/End。本刀不接收对齐。
}
```

`pieces.scalars()` 与 `measured.glyphs` 逐 scalar 对齐，因为测量的就是这串。`Line::glyphs` 同时切这两者。`\n` 和被折掉的 `\r` 不在这串里，也不会进 cmap。

同一批规则用类别单测钉住，避免只在整行结果里间接失败：

```rust
use crate::text::break_class::{allows, classify, BreakClass, PairClass};

assert!(matches!(classify('\n'), BreakClass::Hard));
assert!(matches!(classify('\r'), BreakClass::Pair(PairClass::Other)));
assert!(matches!(classify(' '), BreakClass::Pair(PairClass::Sp)));
assert!(matches!(classify('\u{3000}'), BreakClass::Pair(PairClass::Id)));
assert!(matches!(classify('\u{00D7}'), BreakClass::Pair(PairClass::Other)));
assert!(matches!(classify('\u{00F7}'), BreakClass::Pair(PairClass::Other)));
assert!(matches!(classify('\u{F900}'), BreakClass::Pair(PairClass::Other)));
assert!(allows(PairClass::Id, PairClass::Op));
assert!(!allows(PairClass::Op, PairClass::Id));
assert!(!allows(PairClass::Id, PairClass::Cl));
assert!(allows(PairClass::Cl, PairClass::Id));
assert!(!allows(PairClass::Al, PairClass::Al));
```

`allows` 的参数类型里没有 `BK`。

设计稿要求的断点都能用上面的 `prepare` / `wrap` 写出来，不需要额外入口：

| 用例 | 输入 | `W`（每 scalar 宽 1） | 结果 |
| --- | --- | --- | --- |
| ASCII 在空格处断 | `"hello world"` | `5` | `"hello"`，`"world"` |
| 超宽 `AL` | `"hello"` | `4` | `UnbreakableWiderThanFrame { width: 5, avail: 4 }` |
| `ID` 之间可断 | `"你好"` | `1` | `"你"`，`"好"` |
| `好（` 在 `（` 前断 | `"好（世"` | `2` | `"好"`，`"（世"` |
| `（世` 在括号后不断 | `"（世界"` | `2` | `"（世"`，`"界"` |
| `字。` 在句号前不断 | `"字。"` | `1` | 错误，`width` 2 |
| 句号后可断 | `"字。下"` | `2` | `"字。"`，`"下"` |
| CRLF 一个 `BK` | `"A\r\nB"` | `10` | 两行 `"A"`、`"B"`，`scalars == "AB"` |
| 行首 U+0020 丢掉 | `"  A"` | `10` | 一行 `"A"`，`glyphs == 2..3` |
| 放不下的 U+0020 丢掉 | `"hello "` | `5` | `"hello"`，再加一行宽 0 |
| U+3000 保留 | `"A\u{3000}B"` | `1` | 三行，中间是 U+3000 |
| 组合字符一个簇 | `"e\u{0301}"` | `1` | 错误，`width` 2；`glyphs` 长度 2 |
| 两个 `Other` 不断 | `"\u{FF21}\u{FF22}"` | `1` | 错误，`width` 2 |
| `ID` 与 `Other` 可断 | `"字\u{FF21}"` | `1` | 两行 |

## Shape

装箱前把「禁止断开」编译成片段。相邻两簇若 `allows` 为假，就并进当前片段；为真就切开。`ID|ID` 为真，所以每个汉字自己是一段。`AL|AL` 为假，所以一个西文词是一段。`OP` 与后一簇为假，所以 `（世` 是一段，而 `世|界` 为真，`界` 另起一段。

`wrap` 只看见片段，不再看见 `BreakClass`，也不向左扫描簇。片段边界就是最近的合法断点：更早的边界更远，片段内部没有合法断点。当前行非空而下一片段放不下时，整段回到下一行。当前行为空时，先丢掉片段开头的 U+0020 簇，再按簇前缀求和；第一个放不下的簇触发 `UnbreakableWiderThanFrame`，`width` 含该簇，不含尚未轮到的簇，也不含已丢掉的行首空格。

这与编号算法等价，因为原算法取的是离溢出簇最近的允许断点，而融合时每个允许断点都成了片段边界，最近的那一个就是当前片段的起点。空行不能在「第一个片段之前」再断，否则会空转；空行放不下就报错。

### Module map

```text
Cargo.toml
  unicode-segmentation = "=1.13.3"
src/error.rs
  Error::UnbreakableWiderThanFrame
src/text/cluster.rs
  CR/LF → Normalized；grapheme_indices(true)
src/text/break_class.rs
  BreakClass / PairClass、码位表、ALLOW
src/text/line_break.rs
  Pieces::prepare 融合；Pieces::wrap 装箱
src/text/measure.rs
  不改
src/layout/paragraph.rs
  不改
```

依赖只向下：`cluster` 与 `break_class` 互不引用；`line_break` 使用二者，不使用 `measure`、skrifa、krilla。`text/mod.rs` 继续只声明子模块，不转发 `wrap`。

| 模块 | 拥有 | 不拥有 |
| --- | --- | --- |
| `cluster` | `\r\n` / `\r` 折成 `\n`；已规范化文本上的扩展字素簇 | 类别、宽度、行 |
| `break_class` | 簇首 scalar 的类别；6×6 `ALLOW` | 字符串、片段、装箱 |
| `line_break` | 片段融合、行首 SP、BK 行、溢出错误、`Line` 的 glyph/byte 区间 | 决策表的第二份拷贝、字体、对齐 |

### Types and signatures

`src/error.rs` 增加一个变体，并在现有 `Display` 里加一支：

```rust
/// 从当前行起点到放不下的那个簇（含该簇）宽于可用宽度。
UnbreakableWiderThanFrame {
    /// 不可断前缀的宽度，与 `avail` 同一单位。
    width: f32,
    /// 调用方传入的可用宽度。
    avail: f32,
}

// Display:
// "unbreakable run width {width} exceeds {avail}"
```

`src/text/cluster.rs`：

```rust
/// 已把 `\r\n` 与单独的 `\r` 折成 `\n` 的文本。
///
/// UAX #29 GB3 会把 CR×LF 粘成一个簇，簇首是 `\r`。
/// 字素迭代只接受这个类型，避免把回车当普通字符。
pub(crate) struct Normalized<'a>(Cow<'a, str>);

impl<'a> Normalized<'a> {
    /// 第二次调用对已无 `\r` 的文本不再改写。
    pub(crate) fn new(text: &'a str) -> Self {
        todo!()
    }

    pub(crate) fn as_str(&self) -> &str {
        todo!()
    }
}

/// 规范化字符串上的一个扩展字素簇。簇非空。
pub(crate) struct Grapheme<'a> {
    pub(crate) text: &'a str,
    pub(crate) bytes: Range<usize>,
    pub(crate) first: char,
}

/// `UnicodeSegmentation::grapheme_indices(true)`。
pub(crate) fn graphemes(text: &Normalized<'_>) -> impl Iterator<Item = Grapheme<'_>> {
    todo!()
}
```

`src/text/break_class.rs`：

```rust
/// 簇首 scalar 的类别。`Hard` 不进决策表。
pub(crate) enum BreakClass {
    Hard,
    Pair(PairClass),
}

/// 决策表的行、列。判别值就是 `ALLOW` 的下标，顺序与设计稿列序一致。
#[repr(u8)]
pub(crate) enum PairClass {
    Id = 0,
    Al = 1,
    Other = 2,
    Sp = 3,
    Op = 4,
    Cl = 5,
}

/// 上一类为行、下一类为列。`true` 表示可以在 next 之前断开。
const ALLOW: [[bool; 6]; 6] = [
    [true, true, true, true, true, false],
    [true, false, false, true, true, false],
    [true, false, false, true, true, false],
    [true, true, true, true, true, false],
    [false, false, false, false, false, false],
    [true, true, true, true, true, false],
];

const _: () = assert!(std::mem::variant_count::<PairClass>() == ALLOW.len());

/// 只看一个 scalar。`\n` 为 `Hard`；`\r` 为 `Other`（折行只在 `Normalized`）。
pub(crate) fn classify(scalar: char) -> BreakClass {
    todo!()
}

/// 决策表的唯一读者。
pub(crate) fn allows(prev: PairClass, next: PairClass) -> bool {
    ALLOW[prev as usize][next as usize]
}
```

`classify` 的数据，先精确标点，再区间，其余 `Other`：

```rust
const OP: &[char] = &[
    '(', '[', '{',
    '\u{FF08}', '\u{3010}', '\u{300A}', '\u{300C}', '\u{300E}',
    '\u{3008}', '\u{3014}', '\u{FF5B}',
];
const CL: &[char] = &[
    ')', ']', '}',
    '\u{FF09}', '\u{FF0C}', '\u{3002}', '\u{3001}', '\u{FF1B}', '\u{FF1A}',
    '\u{FF01}', '\u{FF1F}', '\u{3011}', '\u{300B}', '\u{300D}', '\u{300F}',
    '\u{3009}', '\u{3015}', '\u{FF5D}', '\u{FF0E}',
];
/// 闭区间，互不重叠。U+3000 单独成段，不把 U+3001–U+303F 卷进来。
const RANGES: &[(u32, u32, PairClass)] = &[
    (0x0030, 0x0039, PairClass::Al),
    (0x0041, 0x005A, PairClass::Al),
    (0x0061, 0x007A, PairClass::Al),
    (0x00C0, 0x00D6, PairClass::Al),
    (0x00D8, 0x00F6, PairClass::Al),
    (0x00F8, 0x00FF, PairClass::Al),
    (0x3000, 0x3000, PairClass::Id),
    (0x3040, 0x309F, PairClass::Id),
    (0x30A0, 0x30FF, PairClass::Id),
    (0x3400, 0x4DBF, PairClass::Id),
    (0x4E00, 0x9FFF, PairClass::Id),
    (0xAC00, 0xD7AF, PairClass::Id),
    (0xFF66, 0xFF9D, PairClass::Id),
];
```

`src/text/line_break.rs`：

```rust
/// 融合后的段落。与宽度无关，可多次 `wrap`。
pub(crate) struct Pieces {
    scalars: String,
    clusters: Vec<Cluster>,
    pieces: Vec<Piece>,
}

struct Cluster {
    /// 在 `scalars` 与将来的 `MeasuredGlyph` 向量里的 scalar 下标。
    glyphs: Range<usize>,
    /// `scalars` 的 UTF-8 字节区间，与 `glyphs` 是同一批 scalar。
    bytes: Range<usize>,
    /// 簇首是 U+0020。行首丢弃只看这个标志。
    space: bool,
}

enum Piece {
    /// 一个 `\n`。不占 scalar。
    Hard,
    /// `clusters` 上的非空区间。区间内相邻簇的决策表结果都是禁止。
    Ink(Range<usize>),
}

/// 一次 `wrap` 的行。区间只覆盖要画的 scalar。
#[derive(Clone)]
pub(crate) struct Line {
    pub(crate) glyphs: Range<usize>,
    pub(crate) bytes: Range<usize>,
    pub(crate) width: f32,
}

pub(crate) struct Lines<'a> {
    scalars: &'a str,
    lines: Vec<Line>,
}

impl Pieces {
    /// 规范化、分簇、按簇首类别融合。不测量。
    pub(crate) fn prepare(text: &str) -> Self {
        todo!()
    }

    /// 送给 `measure_text` 的串：无 `\r`、无 `\n`，簇内 scalar 顺序不变。
    pub(crate) fn scalars(&self) -> &str {
        todo!()
    }

    /// `width_pt[i]` 是 `scalars()` 第 i 个 scalar 的点宽（或测试用的同一单位）。
    ///
    /// `avail` 非有限或 `<= 0`，或 `width_pt` 长度不符、含非有限或负数，返回 `InvalidGeometry`。
    pub(crate) fn wrap(&self, width_pt: &[f32], avail: f32) -> Result<Lines<'_>> {
        todo!()
    }
}

impl<'a> Lines<'a> {
    pub(crate) fn lines(&self) -> &[Line] {
        todo!()
    }

    pub(crate) fn len(&self) -> usize {
        todo!()
    }

    /// `lines()[index]` 的 `bytes` 切在 `scalars()` 上。
    pub(crate) fn text(&self, index: usize) -> &'a str {
        todo!()
    }
}
```

`prepare` 的融合（实现时按此写，此处不是第二套规则）：

```text
normalized = Normalized::new(text)
prev: Option<PairClass> = None
对 graphemes(normalized) 的每个簇 g:
    Hard:
        关闭正在写的 Ink
        push Piece::Hard
        prev = None
        不把 `\n` 写入 scalars
    Pair(class):
        把 g 的全部 scalar 追加进 scalars，记下 Cluster { glyphs, bytes, space: class == Sp }
        若 prev 是 None，或 allows(prev, class)：
            关闭上一个 Ink，用这个簇新开 Ink
        否则：
            把这个簇并进当前 Ink
        prev = class          // 下一对看的是簇本身，不是片段的第一个类
结束时关闭未结束的 Ink
```

因此 `"好（世"` 的片段是 `Ink(好)`、`Ink(（世)`；`"hello world"` 是词、单空格、词三段；`SP|CL` 合成「空格 + 闭标点」一段，空格仍是该段的第一个 `space` 簇。`OP|SP` 禁止，开口括号后的空格留在括号那段里，不会在行首被单独丢掉。

`wrap` 的装箱：

```text
校验 avail 与 width_pt
簇宽 = width_pt[cluster.glyphs] 之和，只在这里求和
当前行 glyph/byte 起点 = 0，宽度 = 0，空行 = true
i = 0
while i < pieces.len():
    Hard:
        输出当前行（可以是空行）
        新行从同一 glyph 游标开始
        i += 1
    Ink:
        若空行：跳过片段开头的连续 space 簇，游标移过它们，不计入宽度
        若跳完为空：i += 1，继续
        若空行：
            按簇累加，第一处 acc + c > avail 即返回
            UnbreakableWiderThanFrame { width: acc + c, avail }
            否则整段留下，i += 1
        否则若 行宽 + 段宽 <= avail：
            整段留下，i += 1
        否则：
            输出当前行
            新行空，游标设到该片段第一个簇（含其行首空格）
            不增加 i，下一轮以空行重新处理这一段
输入结束后再输出当前行
无片段的空文本因此得到一行 0..0、宽度 0
```

空行分支要么放下剩余簇，要么报错，不会再次「在片段前断开」，所以回退不会空转。已放入本行的 U+0020 留在该行的 glyph 区间里并计入 `width`。

### Invariants encoded in the types

- `allows` 无法接受 `BK`。强制断行只是 `Piece::Hard`。
- `graphemes` 只接受 `Normalized`。`classify('\r')` 不会被用来识别换行。
- `Piece::Ink` 非空，且内部相邻簇都是禁止断开。合法断点只存在于片段之间。
- `Cluster::space` 只在融合时由 `PairClass::Sp` 写成。装箱不再调用 `classify`。
- 行上的 `glyphs` 与 `bytes` 在输出那一刻从同一批留下的簇抄出，之后 `Pieces` 与 `Line` 都没有写入路径。
- 簇宽没有第二份存储。`Line::width` 是留下的簇在 `width_pt` 上的和。
- 测量串与 glyph 下标是同一条 `scalars`。段落不要测量用户原文。

校验在 `wrap` 入口：非法 `W`、宽度切片长度、非有限宽度、负宽度都是 `InvalidGeometry`。融合之后信任 `Piece` 与 `Cluster`。`MissingGlyph` 仍只由 `measure_text` 返回。

明确不做：两端对齐与 Start/Center/End、kerning、连字、bidi、竖排、UAX #14 的其余类、连字符、裁剪、分页、`Flowable`、公共 DSL、改写 `MeasuredText`。

### Interface depth

段落侧的表面是 `prepare`、`scalars`、`wrap`，加上 `Line` 的三个字段。藏在后面的是 CR 折叠、GB3、七类区间、6×6 表、片段融合、行首空格和不可断前缀的宽度。调用方不必知道 `（世` 为什么是一段。

`classify` / `allows` / `Normalized` 是策略模块自己的测试缝，不是装箱的第二套入口。`wrap` 不接收类别，也不接收回调。宽度在边界上一次性对齐到 scalar 下标，测量失败不会插进装箱循环。

按 red flags 看过：生产路径不会连着调「分簇、分类、查表、断行」；决策表不出现在 `line_break` 的装箱循环里；三个文件按各自的规则划分，而不是按加载/校验/保存划分；`text/mod.rs` 不转发同签名函数。嵌套的 `Vec<Vec<Cluster>>` 不用，簇是一块扁平数组，片段只存区间。

## Synthesis decision

N/A. This file is one arena candidate. The synthesizer fills this section.

## Tradeoffs accepted

- 接受西文词和「开口括号 + 后继簇」在 `prepare` 时合成多簇片段，换装箱时不再向左搜索、也不再读 `ALLOW`。
- 接受 `scalars()` 是去掉 `\n` 的第二份字符串，换 `measure_text` 不会因为换行缺字而失败。
- 接受行只暴露一段连续 glyph/byte 区间，换段落画行时直接切 `measured.glyphs`，不必再遍历簇。被丢掉的行首空格是行与行之间的空洞，不是行内的洞。
- 接受调用方自己把 `advance_norm * font_size` 填进切片，换 `line_break` 不依赖 `MeasuredText` 和字号。
- 接受末尾 `\n`，以及「仅因放不下而被丢掉的末尾空格」，各多出一个宽度为 0 的行。这是强制断行和输入结束时都输出当前行的同一条规则。
- 接受负的 scalar 宽度在入口变成 `InvalidGeometry`，换行宽累加保持单调。名义 advance 为负的字体进不了这刀的装箱。

## Alternatives considered

逐簇保存 `break_before`，溢出时从该簇向左找第一枚允许位。决策表在融合时已经回答过「哪里能断」，扫描器再读一遍同一答案。调用方或装箱循环仍要理解簇邻接，表面没有变小，规则却有两个居住点。

按簇回调 `FnMut(&Cluster) -> f32` 现算宽度。回调把测量政策留在装箱过程中，`measure_text` 的 `Result` 也塞不进这个类型。宽度切片在入口校验一次，cmap 错误留在测量那一侧。

装箱时对扁平簇实时查 6×6 表并回溯。`BreakClass` 留在热路径上，以后最容易在扫描器里补一条口头规则。表的读者应当只有 `allows`，扫描器只应当看见已经粘好的片段。

## Open questions and risks

- 末尾换行，以及调用点 B 里放不下的 `"hello "`，会多给出一行宽度为 0 的行。段落 flow 要不要把这行算进行盒？算法现在会输出它。
- 闭标点清单按设计稿两段字符的并集写成，多出来的只有 U+FF0E `．`。这个码位是否就是第二段想保留的那个？
- 链接若要标在用户原文上，`MeasuredGlyph::byte_range` 会落在 `scalars()` 里（已折 CR、已去掉 `\n`）。映射回原文是否完全交给 PR8？

## Next implementation step

先在 `break_class` 落 `ALLOW`、区间表和 `classify` / `allows` 的边界单测，再写 `Normalized` 与 `Pieces::prepare` 的融合。
