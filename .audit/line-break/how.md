### Overview

当前 `quire-pdf` 的文字子系统只完成了 knife 4 的“字体注册、按 Unicode scalar 测量名义 advance、把同一批 glyph 交给后端绘制”。字素簇、断行类别和断行算法尚未实现：`cluster.rs`、`break_class.rs`、`line_break.rs` 都只有模块注释，没有类型、函数或测试。`layout/paragraph.rs` 也仍是占位文件，因此今天还没有从文本到段落换行的可运行链路。

knife 5 / 设计 PR7 要补的是一个独立于 PDF 后端、分页和 DSL 的纯文本层：

```text
CR/LF 规范化
  → UAX #29 扩展字素簇
  → 簇内逐 scalar 沿名义 cmap/advance 管线测量
  → 按簇首 scalar 分为 BK/SP/ID/AL/OP/CL/Other
  → 用固定的“上一类 × 下一类”表和可用宽度 W 产出行
```

关键边界是：**断行原子从 scalar 提升为扩展字素簇，但字体映射和宽度计算仍然逐 scalar 进行。** knife 5 不能引入 shaping，也不能让断行宽度与 knife 4 已经交给 `draw_glyphs` 的 advance 分叉。

### Key Concepts

#### knife 4 已经提供的测量模型

`measure_text` 当前返回扁平的 `MeasuredText`：

- 保存原始 `text: String`；
- `glyphs` 与输入的 Unicode scalar 一一对应；
- 每个 `MeasuredGlyph` 保存 scalar、glyph id、字体单位 advance、归一化 advance，以及相对整段字符串的 UTF-8 byte range；
- `MeasuredText::advance_norm` 是所有 scalar 的归一化 advance 之和。

它目前**没有** `MeasuredCluster`、簇边界或 `BreakClass`，也不接收字号。点值宽度应在使用处按：

```text
width_pt = advance_font_units × font_size / units_per_em
         = advance_norm × font_size
```

计算。

`FontStore::register` 已经负责解析字体、保存 `units_per_em/ascent/descent/leading`，并为每张 face 建立 `glyph_id → advance_font_units` 缓存。`measure_text` 每次从同一份字体字节建立 skrifa `FontRef`、`Charmap` 和 unscaled `GlyphMetrics`，然后对 `text.char_indices()` 逐项执行：

1. `Charmap::map(scalar)`；`None` 返回 `MissingGlyph`；
2. 从缓存读取或用 `advance_width` 取得 unscaled advance；
3. 计算 `advance_norm = advance_font_units / units_per_em`；
4. 保留 glyph id 和 UTF-8 byte range。

`line_metrics` 也已经存在。它要求正字号和有限且大于零的 factor，并使用唯一公式：

```text
natural = ascent - descent + leading
line_height = max(natural, font_size × factor)
baseline = ascent
```

夹具锁定了 descent 为负，不允许在后续段落代码中另造一套符号分支。

#### 字素簇与 scalar 的职责不同

knife 5 必须用精确钉死的：

```toml
unicode-segmentation = "=1.13.3"
```

按 UAX #29 生成扩展字素簇。一个簇不可在内部断开；例如基字加组合附加符、emoji ZWJ 序列都必须作为一个原子。但簇内仍逐 scalar 走现有 cmap 和 advance，簇宽是这些 advance 之和，不做 GPOS 定位。

在分簇前，`\r\n` 和单独的 `\r` 都要规范成一个 `\n`。这使换行统一表现为单个 `BK`。

#### 固定的断行类别

类别只看簇的第一个 scalar：

| 类别 | 范围 |
| --- | --- |
| `BK` | `\n` |
| `SP` | 仅 U+0020 |
| `ID` | U+3400–U+4DBF、U+4E00–U+9FFF、U+3040–U+309F、U+30A0–U+30FF、U+FF66–U+FF9D、U+AC00–U+D7AF，以及 U+3000 |
| `AL` | ASCII 数字和字母；U+00C0–U+00D6、U+00D8–U+00F6、U+00F8–U+00FF |
| `OP` | ASCII `(`、`[`、`{`，以及 `（【《「『〈〔｛` |
| `CL` | ASCII `)`、`]`、`}`，以及设计表列出的中日韩句末标点和闭括号 |
| `Other` | 其余所有 scalar |

几个容易误分类的边界：

- U+00D7 `×`、U+00F7 `÷` 不是 `AL`；
- U+0020 才是可丢弃的普通空格，U+3000 是有自身宽度的 `ID`；
- tab、NBSP、弯引号、emoji 都是 `Other`；
- CJK 扩展 B、兼容表意文字 U+F900–U+FAFF、全角字母数字也是 `Other`，不能顺手扩成 `ID`。

断点定义在“next 之前”。固定决策表如下，`BK` 不参与此表：

| 上一类 \ 下一类 | ID | AL | Other | SP | OP | CL |
| --- | --- | --- | --- | --- | --- | --- |
| ID | 允许 | 允许 | 允许 | 允许 | 允许 | 禁止 |
| AL | 允许 | 禁止 | 禁止 | 允许 | 允许 | 禁止 |
| Other | 允许 | 禁止 | 禁止 | 允许 | 允许 | 禁止 |
| SP | 允许 | 允许 | 允许 | 允许 | 允许 | 禁止 |
| OP | 禁止 | 禁止 | 禁止 | 禁止 | 禁止 | 禁止 |
| CL | 允许 | 允许 | 允许 | 允许 | 允许 | 禁止 |

因此 `OP` 前可以断、`OP` 后不能断；`CL` 前不能断、`CL` 后可以断。`好（世` 可以在 `（` 前断，但不能在 `（` 与 `世` 之间断。`字。` 不能让句号落到下一行。

### How It Works

#### 今天实际可运行的路径

今天的实现是：

```text
FontSource
  → FontStore::register
  → measure_text 按 scalar 得到扁平 MeasuredGlyph
  → GlyphPlacement 保留同一个 glyph_id/advance_norm/byte_range
  → Canvas::draw_glyph_run
  → krilla KrillaGlyph + draw_glyphs(outlined: false)
```

后端不会重新测量。knife 4 的测试已经锁定夹具字体的 `A = 707 fu`、`第 = 1000 fu`、`upem = 1000`，并通过故意修改 advance 的 `TJ -250` 实验确认归一化 advance 没有被后端重复乘字号或重复除以 upem。

#### knife 5 应实现的纯断行过程

给定已经测量并分类的簇序列和可用宽度 `W`：

1. `W` 必须有限且大于零，否则返回现有的 `Error::InvalidGeometry`。
2. 从左到右处理簇。
3. 遇到 `BK`，立即输出当前行；`BK` 本身不绘制，然后清空当前行。
4. 当前行为空且下一个簇是 U+0020 `SP` 时丢弃它；不能用这条规则丢 U+3000 或 `OP`。
5. 当前行加该簇后宽度 `<= W`，就把簇放入当前行。
6. 放不下时，从“当前行末与该簇之间”开始向左查找最近的允许断点，候选包括当前行内部相邻簇之间。
7. 找到后，断点左侧成为本行；右侧簇连同本次放不下的簇按原顺序重新作为下一行输入。下一行再次执行行首 `SP` 丢弃规则。
8. 找不到任何断点，返回 `UnbreakableWiderThanFrame { width, avail: W }`。`width` 是当前不可断运行从本行起点到本次簇、包含本次簇的总宽。不能裁剪、溢出绘制或自动加连字符。
9. 空段落仍产生一个宽度为零的空行，供后续段落 flow 获得稳定行盒。

这解释了 `"hello "` 的边界行为：如果空格本身放不下，`AL|SP` 允许在空格前断；`hello` 留在本行，空格作为下一行输入后因位于行首而被丢弃。已经成功放入一行的尾随空格则保留并计入行宽。

knife 5 必须保持以下不变量：

- 簇不可拆；簇内 glyph 的顺序、glyph id、advance 和 byte range 语义不能变化；
- 簇宽严格等于 knife 4 同一批 scalar advance 之和；
- 测量和绘制继续共用 `advance_norm = font_units / upem`，后端不二次缩放；
- 缺字仍使整份处理失败，不产生 glyph 0、`.notdef` 或字体回退；
- 分类只看簇首 scalar，不能根据字体 glyph、语言或上下文改变；
- 断点只由固定类别表决定；不能暗中补 UAX #14 规则；
- `BK` 强制断行且不绘制，只有 U+0020 可按行首规则丢弃；
- 算法是纯函数，不依赖 Canvas、krilla、页面高度、分页状态或公共文档 DSL。

#### 错误面的现状

与 knife 5 直接相关、已经存在的错误包括：

- `InvalidGeometry`：设计指定用于非法 `W`；
- `MissingGlyph { font, scalar }`：cmap 无映射；
- `Font { index, message }`：字体解析或 advance 读取失败；
- `UnknownFont { id }`：使用了当前 `FontStore` 中不存在的 id。

当前 `Error` 中还**没有**断行必需的：

```text
UnbreakableWiderThanFrame { width: f32, avail: f32 }
```

knife 5 需要加入它及对应的 `Display` 分支。`UnsplittableTallerThanFrame` 属于之后的竖直 flow/分页，不是本次断行所需错误。现有 `NonFiniteLength` / `NonPositiveLength` 是长度构造边界错误；PR7 的决策仍明确要求断行入口遇到非法 `W` 时返回 `InvalidGeometry`。

#### 明确不属于 knife 5

- 不做两端对齐；knife 5 只决定行，`Start/Center/End` 也应由后续段落组装施加；
- 不做 kerning、连字或任何 shaping，不读 GPOS/kern；
- 不做 bidi、阿文/印度系 shaping、竖排或 ruby；
- 不实现完整 UAX #14，也不加入 WJ、GL、NS、HY、RI、emoji modifier、数字百分号等额外规则；
- 不做分页、段后距、`PageBreak`、`KeepTogether`；
- 不实现段落 flow、Canvas 绘制或公共 document DSL。

### Where Things Live

| 路径 | 当前职责 / 状态 |
| --- | --- |
| `src/text/mod.rs` | 挂载 `cluster`、`measure`、`break_class`、`line_break` 四个模块 |
| `src/text/measure.rs` | knife 4 已实现：逐 scalar cmap、advance 缓存、归一化宽度、行度量 |
| `src/text/cluster.rs` | 一行模块注释；PR7 要实现 CR/LF 规范化和 UAX #29 扩展字素簇 |
| `src/text/break_class.rs` | 一行模块注释；PR7 要实现七类枚举、码位分类和固定 pair table |
| `src/text/line_break.rs` | 一行模块注释；PR7 要实现对已测量簇的纯断行 |
| `src/fonts.rs` | `FontStore`、字体注册、face metrics、glyph advance 缓存 |
| `src/error.rs` | 已有字体/几何错误；尚缺 `UnbreakableWiderThanFrame` |
| `src/canvas.rs` | `GlyphRun` / `GlyphPlacement`；接收已经定位且已经测量的 glyph |
| `src/backend/emit.rs` | 将相同 glyph id 和 `advance_norm` 交给 krilla `draw_glyphs` |
| `src/layout/paragraph.rs` | 仍是占位文件；knife 5 不应填它 |
| `Cargo.toml` | 目前只有 unicode-segmentation 的计划注释，依赖尚未加入 |
| `tests/fixtures/quire-measure.ttf` | knife 4 的真实 skrifa 小字体夹具 |

### Gotchas

#### 逻辑测试与真实字体测试要分层

PR7 的分类和断行测试应使用等宽替身，而不是依赖真实字体：设计要求每个 BMP scalar 固定 10 字体单位、upem 1000。这样可精确构造宽度边界，并覆盖：

- ASCII 只在空格处断，超宽 `AL` 运行报错；
- `ID|ID` 可断；
- `好（世` 可在 `（` 前断，`OP` 后不可断；
- `CL` 前不可断、后可断；
- CRLF 只形成一个 `BK`；
- 行首 U+0020 丢弃，放不下的 U+0020 转到下一行后丢弃；
- U+3000 保留并占宽；
- 基字加组合字符是一个簇，宽度仍为簇内 scalar advance 之和；
- 连续 `Other` 不断，`ID|Other` 可断；
- 单簇或不可断运行超宽时精确匹配 `width` 与 `avail`。

真实 skrifa 夹具保留给测量管线测试。它目前只覆盖 printable ASCII、数字、U+2022 和 `第/页/共`，不覆盖完整的括号、句号、组合字符和分类边界，因此不适合承担 PR7 决策表测试。现有真实测试继续锁定：

- `A` 和 `第` 的已知 advance；
- `descent < 0`；
- 未映射 scalar 返回 `MissingGlyph`；
- measured glyph 到 `draw_glyphs` 的 `advance_norm` 不被重复缩放。

如果 knife 5 为测量结果增加 cluster 视图，必须保留后端可无损取得原始扁平 glyph 序列和相对绘制字符串的 byte range；不能为了测试方便重新映射 glyph 或重新计算 advance。

#### 分类表是产品契约，不是近似提示

最常见的错误是“顺手改进”分类：把扩展 B、兼容汉字或全角字母归入 `ID`，把 NBSP 当 `SP`，或者把弯引号当开闭标点。这都会改变已确认的失败模式和断点，必须拒绝。v1 有意让某些 `Other` 运行不可断；过宽时返回错误，而不是声称兼容完整 UAX #14。

#### 空格与标点规则是非对称的

`OP` 前允许、后禁止；`CL` 前禁止、后允许。普通 U+0020 只在成为行首输入时丢弃，已经落入上一行的空格不做尾部 trim。实现若先统一 trim 再断行，会改变行宽、对齐和测试结果。
