# Knife 14 设计：行内填空下划线

基线 `feat/style-builders` @ `5deb0df`。权威是提案书 §2.2，框宽用 §2.1 的 `W` / `W'`，长度与颜色用 §2.0。本刀只扩展段落行内项。不新增 `FlowNode`，不改块级 `HLine`。

填空是与 `text` / `bold` 并列的行内占位：一段不可拆的水平前进量，加上基线下方的一条水平 `Stroke`。它不进入 skrifa，不增加 `line_height`。定宽与吃到行尾互斥。吃到行尾的 blank 是该项所在那一行的剩余宽，不在后续行重复。

## 用法

```rust
document(|doc| {
    doc.font(font_bytes)?;
    doc.paragraph(|p| {
        p.text("金额：");
        p.blank(NonNegative::pt(80.0)?);
        p.text("元");
    });
    doc.paragraph(|p| {
        p.text("大写：");
        p.blank_em(4);
    });
    doc.paragraph(|p| {
        p.first_line_indent(NonNegative::pt(24.0)?);
        p.text("签字：");
        p.blank_to_end()
            .thickness(Positive::pt(0.6)?)
            .color(Color::rgb(0x33, 0x33, 0x33));
    });
    Ok(())
})?;
```

`blank` / `blank_em` / `blank_to_end` 与 `text` 一样写在 `Flow::paragraph` 闭包里。正文、section、`columns`、`keep_together`、列表项都走这条闭包，框宽仍是容器已经传下来的 `W`。单元格、Band、Layer 本刀没有填空入口。

链式的 `thickness` / `color` / `baseline_offset` 借的是当前这条 blank，语句结束即释放。下一次 `text` 仍写在段落上。

## 公共 API

落在 `src/document.rs`，由 `src/lib.rs` 在现有 `Paragraph` 旁边再导出 `Blank`。

```rust
impl Paragraph {
    pub fn blank(&mut self, width: Length<NonNegative>) -> Blank<'_>;
    pub fn blank_em(&mut self, ems: u32) -> Blank<'_>;
    pub fn blank_to_end(&mut self) -> Blank<'_>;
}

pub struct Blank<'a> { /* paragraph + item index */ }

impl Blank<'_> {
    pub fn thickness(&mut self, thickness: Length<Positive>) -> &mut Self;
    pub fn color(&mut self, color: Color) -> &mut Self;
    pub fn baseline_offset(&mut self, offset: Length<NonNegative>) -> &mut Self;
}
```

三个构造方法对应两种宽度：`blank` 与 `blank_em` 都是定宽，`blank_to_end` 是吃到行尾。调用方组不出「又定宽又吃到行尾」。负宽、非有限宽在 `NonNegative::pt` / `mm` 就被拒绝。线粗必须是 `Length<Positive>`，0 与负数在 `Positive::pt` 就被拒绝。基线偏移是 `Length<NonNegative>`：y 向下，正值在基线之下，0 表示线压在基线上，负偏移组不出来。

`Blank` 不标 `must_use`。忽略返回值即使用默认描边。

| 属性 | 默认 | 解析时机 |
| --- | --- | --- |
| 线粗 | `1pt`（`Positive::pt(1.0)`，与 `HLine::default` 同一写法） | 组装期写入 blank |
| 基线偏移 | `1pt` 之下（`NonNegative::pt(1.0)`） | 组装期写入 blank |
| 线色 | `None`，放置时用 `style.color` | 放置期 `color.unwrap_or(style.color)` |
| 定宽 0 | 合法 | 占 0 宽，不发 stroke |
| `blank_em(0)` | 合法 | 布局期变成 0pt，同样不发 stroke |
| `blank_em(n)` | — | 布局期 `n as f32 * style.font_size` |

线上叠字没有字段。不设虚线、双线、线帽。`Stroke` 仍只有线宽。

`text("")` / `bold("")` 保持今天的行为：不产生项，也不打断相邻同 `Face` 的合并，填空之后的空串也不算「后面还有 run」。

## 类型

`TextRun` 与 `Face` 保持原结构。变的是段落保存的向量：元素从 `TextRun` 换成行内项。`TextNode::new` 与 `ParagraphNode::layout(&str)` 仍生成至多一条 Body 文本，单元格、Band、`single_line_run` 不经过 `Paragraph`。

```rust
enum InlineItem {
    Text(TextRun),
    Blank(InlineBlank),
}

struct InlineBlank {
    width: BlankWidth,
    thickness: Length<Positive>,
    color: Option<Color>,
    baseline_offset: Length<NonNegative>,
}

enum BlankWidth {
    Fixed(Length<NonNegative>),
    Em(u32),
    ToEndOfLine,
}
```

`Paragraph` 在 `runs` 之外增加 `error: Option<Error>`，只保留第一次组装错误。字段可以改名为 `items`，语义是有序行内项。

布局期、断行器内部再用解析后的形式。`Em` 在进入断行器之前变成 pt；断行器看不到字号。

```rust
enum Advance {
    Fixed(f32),
    Fill,
}

struct BlankDraw {
    thickness: Length<Positive>,
    color: Option<Color>,
    baseline_offset: Length<NonNegative>,
}
```

`LineBox` 从「只有 `Vec<GlyphRun>`」改成有序片段。坐标相对本行，不预加行号。

```rust
struct LineBox {
    pieces: Vec<LinePiece>,
}

enum LinePiece {
    Glyphs(GlyphRun),
    Stroke(InlineStroke),
}

struct InlineStroke {
    x0: f32,
    x1: f32,
    y: f32,
    color: Color,
    thickness: Length<Positive>,
}
```

## 模块

依赖方向不变：`document → layout → text`。`line_break` 仍不引用 `krilla`、`GlyphRun`、`Path`。

| 模块 | 本刀 |
| --- | --- |
| `document.rs` | `blank` / `blank_em` / `blank_to_end`；组装期拒绝填空后再追加；`Flow::paragraph` 把该错误 `push(Err)` |
| `error.rs` | 新增 `RunAfterFillBlank`。过宽继续用 `UnbreakableWiderThanFrame` |
| `layout/paragraph.rs` | `InlineItem`；按空白切开文本组再测量；放置时走笔；`fragment_from_lines` 多发 `FragmentCommand::Stroke` |
| `text/line_break.rs` | 空白原子、fill 在 `pack` 里取剩余宽、行片段；`Atom::Hard` 自带 cluster 下标 |
| `text/cluster.rs` | 不改。每个连续文本组仍调用现有 `cluster_tagged` |
| `geom.rs` `canvas.rs` `layout/flow.rs` `layout/hline.rs` `backend/` | 不改。复用 `Path::line`、`Paint::solid`、`Stroke::new`、`FragmentCommand::Stroke`、`with_offset`、`stroke_path` |

`break_lines` 保留为纯文本门面，现有断行测试的行文本、`glyph_range`、行宽保持原断言。它内部改走同一套 `break_spans`，再把「只有一个文本组、没有空白」的片段折回今天的 `LineRecord`。段落布局直接消费片段，不走这层折回。

```text
Paragraph::blank / blank_em / blank_to_end
  → Vec<InlineItem>                     document.rs
  → Flow::paragraph                     复制 Style，缩进写入 style
  → TextNode { items, style }
  → TextNode::split
       layout_runs(W)
         文本组 measure（跳过 blank）
         Em → pt
         break_spans
         每行 place → LineBox
       ParagraphNode::split             只按行竖直切
  → fragment_from_lines                 Glyphs + Stroke
  → draw_fragment / with_offset         已有
```

## 组装

`append` 仍只合并相邻的 `InlineItem::Text`，且 `Face` 相同才合并。空白先把两侧文本隔开，所以 `text("A"); blank(...); text("B")` 是三项，不会合成 `"AB"`。相邻两个定宽空白也不合并，各自保留线粗和颜色。

追加真正的项（非空文本，或任意空白）时，若上一项已是 `ToEndOfLine`，记下 `Error::RunAfterFillBlank` 并不再写入。`Flow::paragraph` 看到这个错误就 `push(Err)`，不压入 `TextNode`。`section.lower` 的 `into_nodes` 在 `paginate_sections` 之前返回，因此不会 `start_page`。

允许的顺序：定宽之后再 `blank_to_end`；文本、定宽、汉字数糖任意穿插，只要吃到行尾的那条是最后一项。`text("a\n")` 里的换行是文本内部的 BK，不是下一项，填空仍可以跟在它后面。

## 测量

`layout_runs` 先做今天已有的框校验：`W` 非有限或 `≤ 0`，以及 `W' = W - indent` 非有限或 `≤ 0`，都是 `InvalidGeometry`。这一步仍在测量之前。

然后把 `InlineItem` 切成交替的文本组与空白。连续的 `Text` 合成一组，中间的空白是切断点。

- 一组内仍用 `cluster_tagged`。相邻不同 `Face` 之间的 CRLF 折叠与今天相同。
- 空白两侧各自成组。`text("a\r"); blank(...); text("\nb")` 不会把空白两边折成一个 BK。单独的 `\r` 按现有规范化变成 `\n`。
- 没有空白的段落只有一个组，测量次数、粗体槽、缺字行为与现在一致。
- 空白不进入 `glyph_text`，不查 cmap，不产生 `InlineGlyph`。
- `fonts.bold()` 仍只在某一组里存在非空 Bold 文本时调用。只有空白、或 Body 加空白，不要求 `bold_font`。
- 每个组自己一条 glyph tape，cluster 下标与 glyph 下标都从该组的 0 起算。

`blank_em(n)` 在这里乘 `style.font_size.as_pt()`。乘积非有限或小于 0 时返回 `InvalidGeometry`。合法字号下 `u32` 乘积是有限的；仍然做这次检查。得到的 pt 作为 `Advance::Fixed`。`ToEndOfLine` 保持 `Advance::Fill`，不在这里变成 pt。

行高在断行之后读取，且只读 Body 的 `line_metrics`，与现在相同。空白和粗体都不参与这个数。

全空段落：零个 span，断行结果仍是一行宽度 0。只有空白的段落至少有一个 span，不能走「空 run 列表」的短路。

## 断行

### 空白如何进入断行器

`project_atoms` 今天一个 cluster 一个原子，原子下标等于 cluster 下标。空白没有 cluster。段落把测量结果交给新的 `break_spans`：

```rust
enum Span<T> {
    Group(usize),
    Blank { advance: Advance, payload: T },
}

fn break_spans<T: Copy>(
    groups: &[ClusteredText],
    spans: &[Span<T>],
    budget: LineBudget,
    width_of: impl FnMut(usize, Cluster) -> f32,
) -> Result<BrokenSpans<T>>
```

投影按 span 顺序走。`Group` 的每个 cluster 仍是 `Hard` 或 `Ink`。`Ink` 的宽度仍由 `width_of` 给出，也就是该组 glyph advance 乘字号。`Blank` 变成一个原子，不调用 `width_of`。

```rust
enum Atom<T> {
    Hard { group: usize, cluster: usize },
    Ink { pair: PairClass, width: f32, group: usize, cluster: usize },
    Blank { advance: Advance, width: f32, payload: T },
}
```

`Hard` 记下自己的 cluster。`eof_cursor` 读这个字段。空白插在 BK 前面时，原子下标不再等于 cluster 下标，不能再写 `text.cluster(atom_index - 1)`。

`width` 对定宽在投影时就等于 pt。对 fill 在被接纳之前保持 0，接纳时再写入。

零个 span 与今天的空字符串一样：一行，宽度 0，没有片段。

### 定宽：不可拆，行首不丢

定宽原子内部没有断点。接纳条件和 `Ink` 相同：`line_width + width <= avail` 则留下，否则向前找断点。

断点规则：

1. 紧挨在定宽空白之前，总是可以断开。整颗空白移到下一行。这一条不查 pair 表，空白也不是 `PairClass::Sp`，行首不会把它丢掉。
2. 紧挨在定宽空白之后：若这颗空白（或一串空白）已经顶到当前行的 `line_start`，可以断开，后面的文字按普通规则自己再断。若空白左侧同一行里还有 `Ink`，则用那颗 `Ink` 的 `PairClass` 与空白右侧下一颗 `Ink` 做 `allows`。空白对 pair 表是透明的。`：____。` 在同一行时，`Cl` 对 `Cl` 仍不允许把句号单独拆走；剩余宽不够时，断点落在空白之前，空白和句号一起下移。
3. 两颗 `Ink` 之间仍用现有 pair 表。
4. 行首的 `PairClass::Sp` 仍只丢弃真正的 U+0020。

因此开口括号后面的空白也可以整颗下移。括号留在上一行。这和「空白粘在前一个字上、剩余宽不够就整段报错」不同。

行首原子放不下时，现有 `nearest_break` 的搜索区间是空的，直接 `UnbreakableWiderThanFrame`。定宽空白同样不改去试下一行的 `rest`。`width` 是从当前行起点累加到这颗放不下的原子；空白自己顶在行首时，`width` 就是这颗空白的 pt。`avail` 是正在构造的那一行的预算。

预算：

| 正在构造的行 | `avail` |
| --- | --- |
| 第一条已发出的记录（`records.len() == 0`） | `W' = W - indent` |
| 其后每一行 | `W` |

首行缩进把第一条视觉行收成 `W'`，与 §2.1 以及今天 `"AA"` 在缩进后溢出的语义一致。`W' < 空白 ≤ W` 且空白落在第一条视觉行的行首时，错误里的 `avail` 是 `W'`，不裁切，也不缩到 `W` 里再试。

前面的文字已经占了一些宽、剩余宽不够这颗空白、但下一行的预算够：规则 1 在空白前断开，整颗空白到下一行。下一行行首仍用该行预算再试一次。

### fill 宽度

fill 的宽度不能在 `project_atoms` 里算。那里还不知道这颗原子会落在第几条视觉行，也不知道本行已经收下多少。

`pack` 见到 `Advance::Fill` 时单独处理，不进入溢出搜索：

```text
remain = avail - line_width          # 此时 line_width <= avail
width  = remain                      # 0 合法
把该原子的 width 写成 remain
line_width = avail                   # 直接赋值，避免 x + (a - x) 的舍入
收下该原子
```

`avail - line_width` 就是本行剩余宽。它加进 `LineRecord.width`。因为组装期保证 fill 是最后一项，这一行在它之后没有别的原子，行宽等于该行预算。

`line_width` 被设成 `avail` 而不是 `line_width += remain`，这样 Start / Center / End 的 `align_x` 都是 0。fill 行上的文字从 `frame_x` 起笔，横线铺到该行右缘。早于 fill 的那些已折行没有这颗原子，对齐仍按各自行宽计算。

`remain == 0`：原子仍留下，贡献 0 宽，不另起一行，放置时不发 stroke。前面的文字已经顶满本行时，填空就停在这一行。

fill 不是 BK。它不会像硬换行那样先提交一行再占下一行的整宽，除非它前面的文本里真有 BK，或前面的文字自己折行后，填空正好落在新行的行首。后一种情况剩余宽等于新行的整段预算。

`text("\n"); blank_to_end()`：第一条记录是 BK 制造的空行，吃掉 `W'`；填空在下一条视觉行，预算是 `W`。这是现有行预算状态机，不给填空单独回退。

只有填空、且没有前导 BK：填空在第一条视觉行，宽度是 `W'`。有首行缩进时，线从 `indent` 铺到 `W`，而不是从 0 铺到 `W`。

进入 `break_spans` 之前再查一次：存在 fill span 且它不是最后一个 span 时，返回同一个 `RunAfterFillBlank`。公开路径在组装期已经失败；这条是 `TextNode` 被直接造出来时的同一错误。

### 行片段

`emit` 时按原子区间生成片段，而不是只留一个 `glyph_range`。

```rust
enum Seg<T> {
    Glyphs { group: usize, start: ClusterCursor, end: ClusterCursor },
    Blank { width: f32, payload: T },
}
```

同一组里连续的 `Ink` 并成一个 `Glyphs`。空白把两侧切开。区间不含 `Hard`。空区间没有片段，行宽按调用方传入的值；空行传入 0。

`BrokenLine::glyph_range` 表达不了「字形、空白、字形」：空白不在 glyph tape 里，两侧字形在各自组里甚至下标都从 0 开始。只保存行宽也恢复不了空洞的位置。片段的顺序就是笔序。

纯文本门面在「恰好一个 `Glyphs` 片段」时折回原来的 start/end cursor，空行沿用原来的 `eof_cursor` 规则。现有空行、行尾空格被丢后多出的空行，断言不变。

## 放置：行中空白把后面的字形向右推

断行之后、`align_x` 之前，行宽已经含空白。`layout_runs` 对每一条已断行：

```text
frame_width = index == 0 ? W' : W
frame_x     = index == 0 ? indent : 0
line_x      = frame_x + align_x(frame_width, line.width, style.align)
x           = line_x
```

然后按片段走笔：

1. `Glyphs`：用该组的测量结果调用现有 `glyph_runs_for_line`，起点是当前 `x`。跑完后 `x` 加上这些 glyph 的 `advance_norm * font_size`。
2. `Blank`：若 `width > 0`，记一条行内 stroke，`x0 = x`，`x1 = x + width`。然后无论是否发线，`x += width`。
3. 下一个 `Glyphs` 从更新后的 `x` 起笔。

数值例子：`W = 200`，无缩进，Start。「AB」advance 共 20pt，定宽 30pt，「C」advance 10pt，三者放得下。行宽 60。`line_x = 0`。「AB」原点 x = 0；stroke 为 `(20, y)–(50, y)`；「C」原点 x = 50。

fill 例子：`W = 120`，`indent = 20`，故 `W' = 100`。行首「AB」20pt，然后 `blank_to_end`。剩余宽 80，行宽被写成 100，`align_x = 0`，`frame_x = 20`。字形在 x = 20；stroke 为 `(40, y)–(120, y)`。右端是框的右缘 `W`。

定宽空白不把行宽写成预算。Center / End 把「字 + 空白 + 字」整段挪动，空白两侧的相对距离保持走笔结果。fill 行因为行宽等于预算，三种对齐的额外偏移都是 0，横线占的是行内剩余，不是对齐之后才补的一条框内装饰。

本行没有任何字形片段时（真的空行，或只有空白），在片段最前面插入一条空的 Body `GlyphRun`，原点 `(line_x, baseline)`，字号与颜色用 `style`。这就是今天 `glyph_runs_for_line` 对空 `glyph_range` 的那条 run。列表的 `first_baseline` 只看 `Glyphs`，空 run 让标记仍对齐到基线。已有字形的行不再多插一条空 run。

`y`（行局部）= `baseline + baseline_offset`。`baseline` 是 Body 的 ascent。默认偏移 1pt，所以默认 `y = baseline + 1`。

`width == 0` 不构造 `Path`。`Path::line` 允许重合端点，`Layer::line` 会拒绝；填空在放置前省略，既不报错，也不留下退化 stroke。

## 绘制：一条 stroke，行高不变

`fragment_from_lines` 按 `LinePiece` 的顺序发命令。对片段内的第 `index` 行（尾页从 0 重新计数，与今天字形相同）：

```text
y0 = index * line_height
Glyphs：origin.y = y0 + 行局部基线
Stroke：Path::line((x0, y0 + stroke.y), (x1, y0 + stroke.y))
        Paint::solid(已解析的颜色)
        Stroke::new(thickness)
```

每颗已放置且宽度大于 0 的空白恰好一条水平 stroke，两个端点 y 相同。命令顺序与阅读顺序一致：左文、线、右文。

`Fragment.height` 仍是 `行数 * line_height`。`line_height` 仍是 `max(ascent - descent + leading, font_size * line_height_factor)`。线粗、偏移、`HLine` 的 `space_before` / `space_after` 都不加进 `line_height`、`Fragment.height`、`spacing_after`。段后距仍是 `style.paragraph_spacing`。

线可能画出本行盒子。默认 1pt 线宽、路径在基线下 1pt 时，居中描边的墨大约在 `[baseline + 0.5, baseline + 1.5]`，不切基线。调用方把偏移设得很大时，墨会进入下一行的视觉范围，布局高度仍然不变。本刀不给 fragment 加裁剪。

`LineBox` 上的 x 和 stroke 的 `x0/x1/width` 在第一次 `layout_runs` 就写死。`ParagraphNode::split` 只切 `lines`。尾节点仍是 `FlowNode::Paragraph`，不再断行，不再算剩余宽，也不把续页第一行重新缩进。空白跟着它所在的那一行走。续页 fragment 从行下标 0 重算 y，所以 stroke 的行局部 y 不能在 `layout_runs` 里预先加上「全文第几行」。

列和列表已经会对 stroke 做 `Fragment::with_offset`。列内填空的 `W` 是列宽；列表项里的 `W` 是扣掉标记列之后的正文宽，标记列下不会多出一条线。

后端不新增标记。`to_stroke` 继续把线帽、连接、虚线留在 krilla 默认值。测试锁的是 path 端点。若默认线帽不是 butt，墨可能超出端点约半个线粗；那是后端默认，本刀不给 `Stroke` 加线帽字段。

## 错误

| 情况 | 结果 | 时机 |
| --- | --- | --- |
| 负宽、NaN、±∞ | `NegativeLength` / `NonFiniteLength` | `Length` 构造，组不出 blank |
| 线粗 ≤ 0 或非有限 | `NonPositiveLength` / `NonFiniteLength` | `Positive` 构造 |
| 负的基线偏移 | `NegativeLength` | `NonNegative` 构造 |
| 定宽与吃到行尾同时指定 | 类型上不存在 | — |
| 吃到行尾之后还有非空文本或另一条空白 | `RunAfterFillBlank` | 组装期；`paint` 在 `start_page` 之前返回 |
| 吃到行尾之后只有 `text("")` / `bold("")` | 忽略空串 | 不报错 |
| `W` 或 `W'` 非法 | `InvalidGeometry` | `layout_runs` 开头，测量之前 |
| `blank_em` 乘积非有限 | `InvalidGeometry` | 测量之后、断行之前 |
| 定宽空白宽于它所在视觉行的预算 | `UnbreakableWiderThanFrame { width, avail }` | `pack`；第一条视觉行的 `avail` 是 `W'` |
| fill 的剩余宽为 0 | 不发 stroke，行宽仍等于预算 | 不报错 |
| 定宽为 0 | 不发 stroke，后方字形不右移 | 不报错 |
| 文本缺字 | `MissingGlyph` | 测量期；空白自己不会缺字 |
| 更窄的框上再次 split | `InvalidGeometry` | 已有行为；不重算 fill |

`RunAfterFillBlank` 是无字段变体。显示文案：`a fill-to-end blank must be the last run`。`InvalidGeometry` 已经表示坏框和「缩进 ≥ 框宽」，序列错误若复用它，调用方无法分辨。过宽则复用已有变体，不新增「空白太宽」。

`UnbreakableWiderThanFrame` 的文档注释今天写的是 overflowing cluster。空白不是 cluster。注释改成「从当前行起点到放不下的那一颗原子（cluster 或行内空白）」，避免注释说错。这不改变字段含义。

两条路径都在 `Canvas::start_page` 之前失败：组装错误停在 `Section::lower`；过宽停在 `plan_pages` 的第一次 `split`。`paint_pages` 还没运行，记录画布上没有 `StartPage`。

## 测试

坐标用已有的 `pt_near`。字形 advance 用夹具字体的测量结果计算，不把某次 skrifa 数字写死。stroke 用 `CanvasCommand::StrokePath` 的 `Path::verbs`：一个 `MoveTo`、一个 `LineTo`，两端 y 相等。

### 断行单元（`src/text/line_break.rs`）

- 现有 `break_lines` 断言不改期望。
- 文本、定宽、文本：行宽等于两侧 advance 加空白；片段是 Glyphs、Blank、Glyphs。
- 剩余宽不够定宽、下一行预算够：空白整颗出现在下一行，上一行不含它。行首不丢这颗原子。
- 第一条视觉行行首的定宽满足 `W' < width ≤ rest`：`UnbreakableWiderThanFrame { width: 空白 pt, avail: W' }`。
- `（`（`Op`）后接放不下的定宽：断在空白前，括号留在上一行。
- fill：接纳后该行 `width == avail`，片段宽度等于 `avail - 已接纳行宽`。
- 本行已经顶满再遇 fill：片段宽度 0，行宽仍是 `avail`，行数不增加。
- `Ink`、`Hard`、fill：fill 用的是下一条记录的 `rest`，不是 `first`。
- 原子序列 `[Ink cluster 0, Blank, Hard cluster 1]` 之后的空行，cursor 来自 Hard 上保存的 cluster，而不是 `atom_index - 1`。

### 组装（`src/document.rs`）

- `paragraph_enters_the_flow_unwrapped` 保持「一条 Body 文本」的相等性。`TextNode::new` 内部改存 `InlineItem::Text`。
- `paragraph_coalesces_…` 的期望改成 `InlineItem::Text` 包装；合并与忽略空串的行为不变。
- `text("A"); blank; text("B")` 得到三项，两侧文本不合并。
- `text("A"); text("B"); blank_to_end()` 得到合并后的文本再加 fill。
- fill 之后 `text("x")`、`bold("x")`、再一条 `blank` / `blank_em` / `blank_to_end`：`flow.error == RunAfterFillBlank`，节点列表里没有这段段落。
- fill 之后 `text("")` 不报错。
- `document(...).paint` 在上述组装错误上返回 `RunAfterFillBlank`，记录画布没有 `StartPage`。

### 放置与画布（段落布局测试，RecordingCanvas）

- 行中定宽：后一字的 origin x 等于前一字 advance 加空白 pt。`StrokePath` 两端就是这个区间。y 等于该行基线 + 1。`stroke.width` 为 1pt。paint 为 `style.color`（默认黑）。
- 同一段文字、框足够宽、因此空白不引起折行时，`line_height` 与 `Fragment.height` 等于不含空白的对照段。高度公式里没有线粗。
- 只有 `blank_em(2)`、只注册了正文脸：成功，一条 stroke，一条空 glyph run，不出现 `UnknownFont`。
- 首行缩进加 `blank_to_end`：stroke 从 `indent + 前文 advance` 到 `W`。行数等于前文折行行数；顶满的最后一行剩余为 0 时没有 stroke，也不多一行。
- 多行文字最后 `blank_to_end`：全文只有一条 stroke，在最后一行。
- `Style.align` 为 Center 或 End、且该行含 fill：字形从 `frame_x` 起，不把整段墨居中后再另画一条剩余线。定宽行则整段（含空白宽）参与 `align_x`。
- 宽度 0 的 `blank` 与 `blank_em(0)`：没有 `StrokePath`；后一字 x 等于前一字的末端。
- 自定义页宽下，定宽大于 `W'` 且小于 `W`：`UnbreakableWiderThanFrame` 的 `avail` 接近 `W'`，画布没有 `StartPage`。
- 跨页：页高只够一行，第 0 行是缩进文字加定宽或 fill。该行的 stroke 在第 1 页；第 2 页第一行的字形 x 为 0，且没有把第 0 行的 stroke 再画一遍。`ParagraphNode` 在更窄的框上 split 得到 `InvalidGeometry`。
- 列表项里的 `blank_to_end`：stroke 的 x 落在标记列右侧，右端是该项正文右缘。标记的 y 仍是正文基线。
- 列内 fill：stroke 长度等于列内剩余宽，再被列的 `with_offset` 平移。

### 后端

`backend-krilla` 只断言含一条空白的文档 `to_pdf` 以 `%PDF` 开头。不断言内容流里的数字。

### 明确不测、也不实现

单元格空白、Band 空白、Layer 文字上的下划线、`decoration: underline`、块级 `HLine` 的高度公式、虚线、双线、空白上叠字。
