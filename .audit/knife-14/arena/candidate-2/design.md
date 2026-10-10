# Knife 14 设计：段落内填空下划线

## 目标与边界

填空下划线是段落内的有序行内项，与 `text` / `bold` 同级。它参与断行和水平占宽，但没有 Unicode scalar、glyph id、font 或 advance，不进入 skrifa 测量。每个有正宽度的 blank 最终产生一条 `FragmentCommand::Stroke`；它不是 `FlowNode::HLine`，也不改变行高。

本刀只覆盖正文 `Paragraph`。表格 `Cell`、页眉页脚 `Band`、页面层 `Layer`、普通文字 underline、线上叠字、虚线/双线均不扩展。

采用 arena 默认：

| 项 | 决定 |
| --- | --- |
| 默认线粗 | 1 pt，`Length<Positive>` |
| 默认基线偏移 | y 向下 1 pt |
| 默认线色 | 段落 `style.color` |
| 定宽 span | `Length<NonNegative>`，允许 0 |
| 首行预算 | `W' = W - first_line_indent` |
| 定宽 blank 换行 | blank 整体不可拆；当前行放不下时允许在它之前断行，不使用 `PairClass::Sp` |
| 0 宽 | 占宽 0，不发退化 stroke，不报错 |
| 汉字数糖 | 布局时按 `n * style.font_size` 解析 |
| fill API | 与定宽 API 分开 |

## 调用示意

API 延续现有闭包式 DSL 和 `HLine` 的属性 builder 习惯：

```rust
let signature = NonNegative::mm(35.0)?;
let thin = Positive::pt(0.8)?;
let lower = Finite::pt(1.25)?;

doc.paragraph(|p| {
    p.text("签字：");
    p.blank(signature, |blank| {
        blank.thickness(thin);
        blank.baseline_offset(lower);
        blank.color(Color::rgb(40, 40, 40));
    });
    p.text(" 日期：");
    p.blank_em(6, |_| {});
});

doc.paragraph(|p| {
    p.text("备注：");
    p.blank_to_end(|_| {});
});
```

`blank_em(6)` 表示约六个当前字号的方块汉字宽，不读取字体的“汉字 advance”。`blank_to_end` 必须是该段最后一个非空行内项。

## 公共 API

建议只新增一个小型属性 builder 和三个 `Paragraph` 方法：

```rust
pub struct Blank { /* 字段私有 */ }

impl Blank {
    pub fn thickness(&mut self, thickness: Length<Positive>);
    pub fn color(&mut self, color: Color);
    pub fn baseline_offset(&mut self, offset: Length<Finite>);
}

impl Paragraph {
    pub fn blank(
        &mut self,
        width: Length<NonNegative>,
        f: impl FnOnce(&mut Blank),
    );

    pub fn blank_em(
        &mut self,
        n: u16,
        f: impl FnOnce(&mut Blank),
    );

    pub fn blank_to_end(
        &mut self,
        f: impl FnOnce(&mut Blank),
    );
}
```

- 宽度由方法决定，`Blank` 没有 `width` / `to_end` setter，因此“Fixed 与 ToEnd 同时设置”不可表示。
- `u16` 让汉字数非负、无 NaN/∞ 且平台无关；`0` 与零 pt 定宽同义。乘法在布局期检查结果有限，否则返回现有 `Error::InvalidGeometry`。
- `baseline_offset` 用 `Length<Finite>`，正数在 y-down 坐标中位于基线下方；默认是 `+1 pt`。线粗仍必须严格为正。
- `Blank.color` 内部是 `Option<Color>`；未覆盖时在 `TextNode` 所持的 `Style` 上解析为 `style.color`，不是固定黑色。
- 线上叠字没有 API。

### 组装错误

`Paragraph` 增加“首个错误”状态。`blank_to_end` 成功追加后将段落标成 terminal。此后：

- 非空 `text`、非空 `bold`、任意一种 blank 都记录新的精确错误 `Error::InlineAfterFillBlank`；
- `text("")` / `bold("")` 仍按现行为 no-op，不算 later run；
- `first_line_indent` 是段落属性而不是 run，可继续设置；
- 第二个 `blank_to_end` 也属于 later run。

现有错误枚举没有能准确表达该 DSL 关系错误的变体；不把它伪装成 `InvalidGeometry` 或 `InvalidCellContent`。`Flow::paragraph` 在闭包结束后将 `Paragraph.error` 交给现有 `Flow::push(Err(...))`。因此错误在 section lower 时返回，不会进入分页，更不会调用 `Canvas::start_page`。首个 builder 错误继续获胜。

## 内部类型

### 组装态

`TextNode` 的内容从 `Vec<TextRun>` 改为有序联合：

```text
InlineItem
  = Text(TextRun { text, face })
  | Blank(InlineBlank {
      width: BlankWidth,
      thickness: Length<Positive>,
      color: Option<Color>,
      baseline_offset: Length<Finite>,
    })

BlankWidth
  = Fixed(FixedBlankWidth)
  | ToEndOfLine

FixedBlankWidth
  = Points(Length<NonNegative>)
  | Em(u16)
```

相邻同 `Face` 的文本仍合并；blank 会打断两侧合并。`TextNode::new` 继续生成零或一个 Text item，保证普通字符串、表格字符串和装饰文本不需要新 API。

### 测量/断行边界

`line_break` 不依赖 `InlineBlank` 的画笔属性，只接收排版所需的原子：

```text
BreakAtom
  = Ink {
      pair: PairClass,
      width: f32,
      span: TextSpan,       // 显式 glyph/byte 起止
    }
  | Hard {
      cursor: TextCursor,   // 不再假设 atom index == cluster index
    }
  | Blank {
      blank_id: usize,
      advance: Fixed(f32) | ToEndOfLine,
    }

BrokenLine {
  width: f32,
  parts: [LinePart],
}

LinePart
  = Text(TextSpan)
  | Blank { blank_id, width: f32 }
```

`TextSpan` 明确带 glyph range 与 glyph-byte range；相邻 `Ink` 可在输出时合并。blank 只用稳定的 `blank_id` 回查 thickness、color、offset。这样移除了现有 `Atom` 下标等于 cluster 下标、整行只有一个连续 `glyph_range` 的隐含前提。

文本按“被 blank 分开的最大连续 Text 组”调用 `cluster_tagged`。不能先删除 blank 再把两侧字符串拼起来，否则组合字符或字素簇可能错误地跨过 blank。每组测量后把 span 重定位到段落级 glyph/byte tape。只有这些真实文本组调用 cmap/skrifa；blank-only 段落不会制造空格、下划线或虚拟 glyph。

### 布局态

```text
LineBox {
  runs: Vec<GlyphRun>,
  blanks: Vec<BlankStroke>,
}

BlankStroke {
  x0: f32,
  x1: f32,
  y: f32,                  // baseline + baseline_offset
  thickness: Length<Positive>,
  color: Color,
}
```

`BlankStroke` 是行局部坐标。零宽 blank 不加入 `blanks`。`LineBox` 与 glyph 一起保存在 `ParagraphNode.lines` 中，所以分页切整行时两者绝不会分家。

## 模块接入图

| 文件 | 改动职责 |
| --- | --- |
| `src/document.rs` | `Blank` 公共属性 builder；三个 `Paragraph` 方法；terminal/首错收集；把有序 `InlineItem` 交给 `TextNode` |
| `src/error.rs` | 增加 `InlineAfterFillBlank` 及明确文案 |
| `src/layout/paragraph.rs` | 定义内部 inline item；只测量 Text；解析 em；构造 breaker atoms；按 `LinePart` 回放 x；`LineBox` 保存 strokes；fragment 发 stroke |
| `src/text/line_break.rs` | 支持显式 text span、固定 blank、动态 fill；输出可回放 `LinePart`；去掉 atom/cluster 下标等同假设 |
| `src/layout/flow.rs` | 无新节点、无新命令；复用 `FragmentCommand::Stroke` 与 path 平移 |
| `src/geom.rs` | 无新几何类型；复用 `Length`、`Color`、`Path`、`Paint`、`Stroke`、`Point` |
| `src/canvas.rs`、`src/backend/` | 无协议变化；已有 `stroke_path` 翻译直接工作 |
| `src/layout/hline.rs` | 不接入；只可参考构造 `Path::line` 的方式 |

`lib.rs` 只需随实现公开 `Blank`（如果公共项采用显式重导出）；不得公开任何 breaker 或 krilla 类型。

## 布局算法

### 1. Assemble

1. `Paragraph::text` / `bold` 追加 `InlineItem::Text`；空字符串忽略；只在最后一项也是同 face Text 时合并。
2. `blank` / `blank_em` 创建 `InlineItem::Blank(Fixed(...))`。
3. `blank_to_end` 创建 `InlineItem::Blank(ToEndOfLine)` 并设置 terminal。
4. terminal 后出现实际行内项时记录 `InlineAfterFillBlank`。
5. `Flow::paragraph` 捕获当前 `Style` 和首行缩进，若有组装错误则推入现有错误通道，否则生成一个 `FlowNode::Text`。blank 永远不成为 `FlowNode::HLine`。

### 2. Measure and project

`TextNode::split` 第一次得到实际框宽 `W`：

1. 验证 `W > 0`，计算首行 `W' = W - indent` 并验证 `W' > 0`。
2. 从 Body 字体取得 line metrics。即使段落只有 blank，也仍用相同 baseline/line height；blank 的线粗和偏移不参与 metrics。
3. 对每个连续 Text 组做 cluster、字体选择和 skrifa 测量，投影为 `Ink` / `Hard`。Hard 原子携带显式 cursor。
4. 定宽 pt 直接成为 `Fixed(width)`；`blank_em(n)` 在这里解析 `n * style.font_size.as_pt()`，验证结果有限且非负。
5. blank 不触发 cmap、缺字检查、bold font 查询或 glyph advance。

### 3. Break / pack

breaker 使用 `LineBudget { first: W', rest: W }` 贪心装原子：

- `Ink` 沿用现有 pair table。
- `Fixed(w)` 是单个不可拆原子，不会作为 `Sp` 在行首被删除。若当前行余量不足，可在 blank 之前提交已有内容，再把整个 blank 放到下一视觉行。
- break-before-blank 是显式边界规则，不借用 Unicode 字符类别。fixed blank 后通常也允许断；若后继 text atom 是 `PairClass::Cl`，则禁止让闭口标点独占行首。这样既不把 blank 粘在前文或后续单词上，也不修改现有 6×6 文本矩阵。
- blank 位于空视觉行开头且 `w > avail`，或移行后仍有 `w > avail`，返回现有 `UnbreakableWiderThanFrame { width: w, avail }`。第一视觉行的 `avail` 是 `W'`，不会因为后续 `W` 较宽而暗中跳过首行。
- `ToEndOfLine` 只可能是段落末原子。接纳它时才计算 `resolved = avail - line_width`；此时 `line_width <= avail`。记录 `LinePart::Blank { width: resolved }`，并把最终 `BrokenLine.width` 设为 `avail`。
- `resolved == 0` 时仍有一个 0 advance part，但后续不发 stroke。它不是错误。
- fill 前若有 Hard，Hard 已提交上一行，fill 使用下一视觉行的 `rest = W`。

定宽 blank 的宽度从一开始就在 pack stream 中，因此会影响断点。fill 也在 pack 内解析，而不是绘制时补齐；这保证 End/Center 对齐看到真实的整行宽度。

### 4. BrokenLine → LineBox

每行先计算：

```text
frame_width = line 0 ? W' : W
frame_x     = line 0 ? indent : 0
line_x      = frame_x + align_x(frame_width, broken.width, style.align)
cursor_x    = line_x
```

然后按 `LinePart` 顺序回放：

1. `Text(span)`：从 `cursor_x` 生成一个或多个按字体切分的 `GlyphRun`，随后按该 span 的 measured advance 推进 `cursor_x`。
2. `Blank { width }`：记录 `[cursor_x, cursor_x + width]`，其 `y = metrics.baseline + baseline_offset`；再把 `cursor_x` 推进 width。
3. 因为 cursor 真正经过 blank，位于段落中间的 fixed blank 会把后面的 glyph origin 精确右移 blank 宽度。
4. fill 行的 `broken.width == frame_width`，所以 Start/Center/End 的额外对齐量都为 0；首行端点是段落框的 `W` 右边缘，而不是缩进后的局部原点加错一次。

可用 debug assertion 检查最终 `cursor_x - line_x` 与 `broken.width` 近似相等，防止 breaker 和行回放漂移。

### 5. LineBox → fragment

`fragment_from_lines` 对每行加 `line_index * line_height`：

- GlyphRun 按现有方式调整 origin.y，发 `FragmentCommand::Glyphs`。
- 每个 `x1 > x0` 的 blank 用 `Path::line((x0, y), (x1, y))`，发恰好一个 `FragmentCommand::Stroke { Paint::solid(color), Stroke::new(thickness) }`。
- `x1 == x0` 不构造 path、不发命令。

`Fragment.height` 仍是 `line_count * line_height`，`spacing_after` 不变。thickness、offset 既不增高 `LineBox`，也不增高 Fragment；较大的自定义偏移可以视觉上进入邻行，但不会推动布局。

### 6. Pagination and paint

首次 `TextNode::split` 已将 glyph x 和 blank path 几何烘焙进每个 `LineBox`。`ParagraphNode::split` 只按整行切 `Vec<LineBox>`；stroke 与所在行一起进入 head 或 tail。续页不重新断行、不重新计算 fill，也不重复首行缩进。

`paginate_sections` 先完成所有 `plan_pages` 和 decoration 测量，之后才 `paint_pages/start_page`。因此组装错误、非法 em 乘积、固定 blank 过宽、缺字体等都在任何页面开始前返回。

## 错误与边界情况

| 情况 | 结果 |
| --- | --- |
| 负、NaN、∞ 定宽 | 由 `Length<NonNegative>` 构造器的现有错误拒绝 |
| 0 定宽 / `blank_em(0)` | 合法，advance 0，省略 stroke |
| 非正线粗 | 由 `Length<Positive>` 构造器拒绝 |
| 非有限 offset | 由 `Length<Finite>` 构造器拒绝 |
| `n * font_size` 溢出非有限 | 布局时报 `InvalidGeometry`，且画布为空 |
| fill 后有非空 run | 组装错误 `InlineAfterFillBlank` |
| fill 后只有空字符串 append | 保持 no-op，合法 |
| 定宽 blank 超过所在视觉行预算 | `UnbreakableWiderThanFrame`；首行预算为 `W'` |
| fill 恰好无剩余 | 合法，0 advance stroke 被省略 |
| blank-only 段落 | 仍有 Body 字体 baseline 和一行正常行高；只画非零 blank stroke |
| 大线粗/大 offset | 不裁、不增行高；坐标仍须有限 |
| fixed blank 位于文本中间 | stroke 占宽，后续 glyph x 右移 |
| hard break 后 fill | fill 位于下一视觉行，使用 `W` |
| 段落跨页 | blank 随它的整行移动 |

## 测试计划

### `document.rs` 组装单测

1. `blank` 打断相邻相同 face 的文本合并：`Text("A"), Blank, Text("B")` 保持三项。
2. fixed、em、to-end 三个入口生成互斥的内部 width 形态，默认属性为 1 pt、1 pt offset、继承色。
3. `blank_to_end` 后的非空 `text`、`bold`、fixed blank、第二个 fill 都得到 `InlineAfterFillBlank`；空字符串不报错。
4. 错误进入 Flow 的“首错获胜”通道。

### `line_break.rs` 纯单测

1. fixed blank 是一个不可拆 atom；行尾余量不足时在 blank 前断，blank 不被当作 leading space 丢弃。
2. fixed blank 自身大于当前 `LineBudget` 时精确返回 `UnbreakableWiderThanFrame`。
3. 第一行使用 `first = W'`，后续行使用 `rest = W`。
4. fill 解析为 `avail - line_width`，最终 line width 等于 avail。
5. fill 在 Hard 后使用 rest budget。
6. 0 剩余 fill 和 0 fixed 都成功。
7. 显式 Hard cursor 测试证明插入 blank 后不再依赖 atom index == cluster index。
8. 输出 parts 顺序为 `Text, Blank, Text`，两段 text span 不被错误合并。

### `paragraph.rs` 布局单测

1. `A + fixed(20) + B`：B 的 glyph x 等于 A 末端加 20；stroke 两端正好填入该间隔。
2. 自定义 offset 的 stroke y 精确等于 `baseline + offset`；自定义 thickness/color 原样进入 FragmentCommand。
3. blank-only 不调用文字测量路径，但仍使用 Body baseline/line height。
4. em 宽度等于 `n * style.font_size`。
5. fill 在有首行缩进时终点仍是段落框右边缘，宽度按 `W'` 解析；Center/End 不产生额外漂移。
6. fixed blank 与 fill 都不会改变 `line_height`、fragment height 或下一段 y，即使 thickness/offset 很大。
7. 每个正宽 blank 恰好一个 Stroke；零宽不发 Stroke。

### `tests/document.rs` RecordingCanvas 集成测试

1. 锁定 `CanvasCommand::StrokePath` 的两个 `PathVerb`、颜色、stroke width。
2. 锁定中间 blank 后 glyph 的页面 x，含 margin、column 或 list item 的外层 offset。
3. 细页面上让含 blank 的行落到第二页，断言 glyph 与 stroke 同页且相对 y 不变。
4. fixed 过宽返回 `UnbreakableWiderThanFrame`，RecordingCanvas 无任何命令。
5. fill 后 run 的组装错误也保证无 `StartPage`/页面命令。
6. 对照有/无 blank 的下一段 y，证明不增行高。
7. fill 在首行缩进、hard break 后、列宽内分别锁定右端点，防止误用纸张宽或全框 `W`。

### backend-krilla

只加一个带 fixed 和 fill blank 的 `to_pdf()` 冒烟测试，断言 `%PDF`。坐标、路径、颜色和行高全部由 RecordingCanvas 测试负责，不断言压缩 PDF 内容流。
