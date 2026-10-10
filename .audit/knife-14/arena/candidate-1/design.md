# Knife 14 — 填空下划线（§2.2）设计

**基线：** `feat/style-builders` @ `5deb0df`  
**范围：** 段落行内定宽 / 吃到行尾 blank；不含 Cell·Band·Layer、块级 `HLine` 改版、text `decoration: underline`、具名样式。

---

## 1. 用法 sketch

与 `text` / `bold` 同级，从左到右混排在同一段落闭包里。组装期只追加行内项；实际框宽 `W` 在首次 `TextNode::split` 时才已知。

```rust
doc.paragraph(|p| {
    p.text("合同编号 ");
    p.bold("C-18");
});

doc.paragraph(|p| {
    p.text("金额（大写）：");
    p.blank_em(8); // 8 × 当前 style.font_size（布局期解析）
});

doc.paragraph(|p| {
    p.text("签字：");
    p.blank(Length::pt(120.0)?);
    p.text("    日期：");
    p.blank_em(4);
});

// 吃到行尾：必须是本段最后一个「行内项」（空 text 不算）
doc.paragraph(|p| {
    p.text("附：");
    p.blank_to_end();
});

// 可选：线粗 / 线色 / 基线偏移（默认见 §3.3）
doc.paragraph(|p| {
    p.text("备注 ");
    p.blank_styled(
        NonNegative::pt(80.0)?,
        BlankStroke::from_style(&style).thickness(Positive::pt(0.75)?),
    );
});
```

`blank_to_end()` 之后若再调用 `text` / `bold` / `blank` / `blank_em` / `blank_styled` / `blank_to_end`，组装期失败（§2.2）。`text("")` 仍被忽略，不视为「后面的 run」。

---

## 2. 公共 API（`document.rs`）

原则：定宽与吃到行尾 **互斥**；宽度用 §2.0 长度类型；非法宽度在构造期或组装期拒绝；表面保持小。

| 方法 | 含义 |
| --- | --- |
| `Paragraph::blank(width: Length<NonNegative>)` | 定宽 blank |
| `Paragraph::blank_em(n: u32)` | `n * style.font_size` → 内部存为布局期才固定的「em 个数 + 标记」，或组装时仅记 `n`，layout 乘字号（见 autonomy：`blank_em` 在 layout 解析） |
| `Paragraph::blank_to_end()` | 吃到行尾；设置「终止符」状态 |
| `Paragraph::blank_styled(width, stroke: BlankStroke)` | 定宽 + 显式 stroke 属性（`blank` / `blank_em` / `blank_to_end` 用默认 stroke） |
| `Paragraph::blank_to_end_styled(stroke: BlankStroke)` | 吃到行尾 + 显式 stroke |

`BlankStroke`（公共，可放在 `document` 或 `geom` 旁的小模块，随实现 PR 定名）：

```rust
pub struct BlankStroke {
    pub thickness: Length<Positive>,       // 默认 1 pt
    pub color: Color,                      // 默认 style.color（autonomy：非 always-black）
    pub baseline_offset: Length<NonNegative>, // 默认 1 pt，y 向下，线在 baseline 下方
}

impl BlankStroke {
    pub fn from_style(style: &Style) -> Self;
    // 可选链式 setter：thickness / color / baseline_offset
}
```

**不暴露：** `FlowNode` 新块、`InlineItem`、断行 atom、`PairClass` 扩展。

**组装态 `Paragraph` 字段（内部）：**

```text
runs: Vec<InlineItem>          // 取代纯 TextRun 向量
fill_sealed: bool              // blank_to_end* 已调用
assemble_error: Option<Error> // 首个组装错误
first_line_indent: ...         // 不变
```

`Flow::paragraph`：闭包返回后，若 `assemble_error` 为 `Some`，`push(Err(...))`；否则 `TextNode::from_inline_items(runs, style)`。

---

## 3. 类型与数据形状

### 3.1 行内联合（layout 侧 `pub(crate)`，与 `data-shape.md` 一致）

```rust
pub(crate) enum InlineItem {
    Text { text: String, face: Face },
    Blank {
        width: BlankWidth,
        stroke: BlankStroke,
    },
}

pub(crate) enum BlankWidth {
    Fixed(Length<NonNegative>),
    EmUnits(u32),           // blank_em；layout 时 × style.font_size
    ToEndOfLine,
}
```

- 相邻同 `Face` 的 `Text` 仍在组装期合并；**任意 `Blank` 打断合并**。
- `Fixed(0)` / `EmUnits(0)` 合法：占宽 0，**不发射** stroke（autonomy）。
- `ToEndOfLine` 与 `Fixed` 互斥由枚举保证。

### 3.2 断行 atom（`text/line_break.rs` 扩展，仍 `pub(crate)`）

在现有 `Hard` / `Ink` 之外增加 blank，并 **解除「atom 下标 == cluster 下标」**：

```rust
enum Atom {
    Hard { after_cluster: usize }, // 显式 BK 所在 cluster；修正 eof_cursor
    Ink { pair: PairClass, width: f32, cluster: usize },
    BlankFixed { width: f32, item_index: usize },
    BlankFill { item_index: usize },
}
```

- `width` 在 `BlankFixed` 于 **投影期** 已换算为 pt（含 `EmUnits × font_size`）。
- `BlankFill` 在 **pack 接纳时** 解析：`width = avail - line_width`（当前视觉行预算，首行 `W'`，见 §2.1）。
- `item_index` 指向 `InlineItem` 下标，用于回放 stroke 属性与测试。

**PairClass：** 新增 `PairClass::InlineBlank`（名称可微调），语义：

- **不是** `Sp` → 行首不会被丢弃。
- 与左侧 `Ink` 之间：允许断行（整颗 blank 挪到下一行；autonomy **glue-to-prev 的反面**）。
- blank 自身不可拆分；overflow 时优先在 blank **前** 断，否则若整颗 blank 仍大于 `avail` → `UnbreakableWiderThanFrame`。

`Hard` 的 `after_cluster` 供 `emit_line` / `eof_cursor` 在 atom 流含 blank 时仍正确映射 `ClusterCursor`。

### 3.3 默认值（honor `frame.md` autonomy）

| 属性 | 默认 |
| --- | --- |
| `thickness` | `1 pt`（`Length<Positive>`） |
| `baseline_offset` | `1 pt` 低于 baseline（y 向下） |
| `color` | `style.color` |
| 定宽类型 | `Length<NonNegative>`（0 允许，0 宽不画线） |
| 首行 overflow 比较 | 视觉行预算 `W'`（非「full-frame-W」） |
| `W_remain == 0` 的 fill | 占宽 0，**省略** stroke |
| fill 与定宽 API | 独立方法，非单一 enum builder |

---

## 4. 模块地图

| 模块 | 变更 |
| --- | --- |
| `src/document.rs` | `Paragraph` blank builders、`BlankStroke`、fill 终止校验、`Flow::paragraph` 错误通道 |
| `src/layout/paragraph.rs` | `InlineItem`、`TextNode` 持有项列表；`measure_inline` 跳过 blank；`layout_runs` 驱动新断行；`LineBox` 有序片段；`fragment_from_lines` 发 `Stroke` |
| `src/text/line_break.rs` | `Atom` 扩展、`project_inline_atoms`（或 `break_inline` 入口）、`pack_lines` 处理 `BlankFixed`/`BlankFill`；`BrokenLine` 暴露 **atom 范围** |
| `src/text/break_class.rs` | `PairClass::InlineBlank` + `allows` 表项 |
| `src/geom.rs` | 无新几何类型；复用 `Path::line`、`Stroke`、`Paint` |
| `src/layout/flow.rs` | 无变更（已有 `FragmentCommand::Stroke` 平移） |
| `src/layout/hline.rs` | 仅测试参考 stroke 构造 |
| `src/error.rs` | 组装违规：优先 **`Error::InvalidGeometry`**（跨字段 DSL 约束）；若需更清晰诊断，可增 **`Error::BlankAfterFill`**（实现 PR 二选一，不在本设计强制） |

依赖方向不变：`document → layout → text`；`line_break` 不依赖 `document`。

---

## 5. 布局算法

### 5.1 总览

```mermaid
flowchart LR
  A[Paragraph 组装 InlineItem] --> B[TextNode + Style]
  B --> C[layout_runs 得 W 与 W']
  C --> D[measure_inline 仅 Text]
  D --> E[project_inline_atoms]
  E --> F[pack_lines + fill 解析]
  F --> G[LineBox 片段 + x]
  G --> H[fragment_from_lines]
  H --> I[Glyphs + Stroke]
```

### 5.2 Assemble

1. `text` / `bold` → `InlineItem::Text`（空串丢弃；同 face 合并）。
2. `blank*` → 若 `fill_sealed` 则记录组装错误并 no-op 后续 mutating 调用。
3. `blank_to_end*` → `BlankWidth::ToEndOfLine`，`fill_sealed = true`。
4. `blank_em(n)` → `BlankWidth::EmUnits(n)`，不在组装期乘字号（autonomy）。

**blank-only 段落：** 允许（仅 blank 或 blank+text）。行高仍由 **Body** 字体 `line_metrics` 决定；与 today 空 `runs` 产生一行空 `GlyphRun` 的行为对齐：至少一行、`line_height` 不变。

### 5.3 Measure

对 `InlineItem` 顺序扫描：

1. 将所有 `Text` 片段按序送入现有 `cluster_tagged` → `TaggedClusteredText<Face>` + `InlineGlyph` tape（blank 不进串）。
2. 记录每个 `Text` 片段对应的 **cluster 子范围**（`ClusterSpan`），供投影 ink atom。
3. 不加载 bold 字体 unless 存在非空 bold **文本**（与 today 一致）；blank 不触发 cmap / `MissingGlyph`。

输出：`MeasuredInline { text, glyphs, items: &[InlineItem], cluster_spans: ... }`。

### 5.4 Project atoms（blank 如何进入 breaker）

按 `InlineItem` 顺序交错发射 atom：

| Item | Atoms |
| --- | --- |
| `Text` | 对该 span 内每个 cluster：`Hard` 或 `Ink{width=cluster_width_pt(...)}` |
| `Blank Fixed/Em` | 单个 `BlankFixed{width_pt}` |
| `Blank ToEndOfLine` | 单个 `BlankFill` |

BK 仍来自文本 cluster，不来自 blank。

入口函数建议：

```rust
pub(crate) fn break_inline(
    measured: &MeasuredInline,
    budget: LineBudget,
    font_size: f32,
) -> Result<BrokenInlineLines>;
```

内部：`atoms = project_inline_atoms(...)?` → `pack_lines_inline(&measured.text.clustered(), &atoms, budget)`。

### 5.5 Pack（定宽 / fill / overflow）

复用现有贪心框架，扩展 `match atoms[i]`：

**`BlankFixed { width }`**

- 若 `line_width + width <= avail`：累加宽度，继续。
- 否则：在 `nearest_break` 中允许在 blank **前** 断（`InlineBlank` pair 规则）；若断后新行仍 `width > avail` → `UnbreakableWiderThanFrame { width, avail }`（`avail` 为 **该行** 预算，首行 `W'`）。

**`BlankFill`**

- 在接纳瞬间：`w = avail - line_width`（≥ 0）。
- `line_width += w`；记录解析宽度到 side table（`ResolvedFillWidth` per line+item）。
- **本行结束**：push line record（组装期保证后面无同段 item；若硬换行后单独一行 fill，则用该行 `W` 或 `W'`）。

**`Ink` / `Hard`**

- 与 today 相同；`emit_line` 改为使用 **atom 下标范围** `[atom_start, atom_end)`，并并行维护 `glyph` cursor 区间（供 `line.text()` 与测试）。

**定宽 vs 提案「宽于当前行 W」：** 与现有文本 atom 一致，比较对象是 **该 visual line 的 avail**（首行 `W'`，后续 `W`），不是未扣 indent 的原始 `W`。

### 5.6 LineBox 与 middle blank 后移字形

`BrokenLine` 新增：

```rust
atom_range: Range<usize>,
resolved_widths: ... // fill 解析结果，或 inline 查表
```

**`layout_line_segments`**（`paragraph.rs`）对每条 broken line：

```text
x ← frame_x + align_x(frame_width, line.width(), align)
for atom in line.atom_range:
  Ink → 从 measured.glyphs 切 GlyphRun，origin.x = x，x += advance
  BlankFixed/Fill →
      w ← fixed 或 resolved fill
      if w > 0:
          y ← baseline + stroke.baseline_offset
          segment ← horizontal Path::line((x,y), (x+w,y))
          push LineSegment::Stroke { path, paint, stroke }
      x += w
```

`LineBox` 由「仅 `Vec<GlyphRun>`」改为 **`Vec<LineSegment>`**（或 `segments` + 有序 enum），保证 **blank 后的 glyph x 已含 blank 占宽**。

`align_x` 仍基于 **`line.width()`**（必须含 fill 解析宽），这样 Center/End 与文字+线整体一致。

### 5.7 Fragment 与行高

`fragment_from_lines`：

- 对每个 `LineSegment::Glyphs`：与 today 相同，y += `index * line_height`。
- 对每个 `LineSegment::Stroke`：同样 y 偏移；发 **`FragmentCommand::Stroke`**（与 `hline.rs` 相同构造，`Paint::solid(color)`）。

**不**把 `thickness` / `baseline_offset` 加入 `line_height`、`Fragment.height`、`spacing_after`。stroke 可能 visually 侵入下一行，但布局高度不变（提案 §2.2）。

### 5.8 分页

首次 `layout_runs` 已烘焙 fragment 内 x/y。`ParagraphNode::split` 仍按行切片 `LineBox`；blank stroke 随所在行跨页，**不**重新断行、不重新算 fill。

---

## 6. 错误 cases

| 阶段 | 条件 | 错误 |
| --- | --- | --- |
| 组装 | `blank_to_end` 后仍有行内项 | `InvalidGeometry` 或专用 `BlankAfterFill` |
| 组装 | 长度构造非法 | 现有 `NonNegative` / `Positive` 构造错误 |
| layout | `W` / `W'` 非法 | `InvalidGeometry`（today） |
| layout | 定宽 blank 宽于当前行 avail | `UnbreakableWiderThanFrame` |
| layout | fill 且 `W_remain == 0` | **成功**，0 宽、无 stroke |
| layout | 缺字（仅 Text） | `MissingGlyph`（blank 不触发） |
| layout | bold 无注册 | `UnknownFont`（blank 不触发） |

目标：**组装与 layout 校验在 `document` / `layout_runs` 完成**，尽量在 `start_page` 之前通过 `document(...)?` 失败。

---

## 7. 测试计划

### 7.1 单元 / 模块

**`text/line_break.rs`（inline atoms）**

- 定宽 blank 在行末 fit / 行前断行挪到下一行。
- 定宽 blank 宽于 `W'`（首行）→ `UnbreakableWiderThanFrame`。
- fill 解析：`W_remain` 等于 `avail - prior_width`；首行用 `W'`。
- fill 行 `width` 等于 frame_width（Start 对齐时 line_x 仅 indent）。
- `W_remain == 0`：行宽不变、无 stroke segment。
- atom 流含 blank 时 `Hard` / eof cursor 仍正确。

**`layout/paragraph.rs`**

- `RecordingCanvas`：`text + blank + text` 中断行后，**后续 glyph 的 x** 比无 blank 时大 `blank_width`。
- `StrokePath` 端点：`x0`、`x1 = x0 + width`，`y = baseline + offset`（多行时加 `line_index * line_height`）。
- **行高**：含 blank 段与纯文段相同 `line_height` / `Fragment.height`。
- `blank_only` / `text + blank_to_end`：至少一行、baseline 合理。
- Center/End：含 fill 行整体居中/右对齐，stroke 与文字一起移动。
- 硬换行后 `blank_to_end`：fill 用第二行预算 `W`。

### 7.2 集成（`document` + flow）

- `blank_to_end` 后再 `text` → 组装错误，空 PDF / 现有 `assert_fails_with_empty_canvas` 模式。
- 列内窄 `W`：blank 随列宽断行/ fill。
- 跨页：段落 `Partial` 后 tail 页仍带 stroke（RecordingCanvas 计数）。

### 7.3 Backend

- `cargo test --features backend-krilla`：含 blank 段落仍 `%PDF` 冒烟；不断言压缩流细节。

### 7.4 非目标（本 knife 不测）

- Cell 内 blank、Band/Layer、underline decoration、underscore 字符占位、双线/虚线。

---

## 8. 实现 PR 建议顺序

1. `InlineItem` + `Paragraph` builders + fill 组装错误 + `TextNode` 接线（可先用 layout 单测直接喂 items）。
2. `project_inline_atoms` + `pack_lines` blank + `PairClass::InlineBlank`。
3. `LineSegment` / `layout_line_segments` / `fragment_from_lines` stroke。
4. `document` 集成测试与 RecordingCanvas 断言。
5. backend 冒烟。

每步保持 `fmt` / `clippy -D warnings` / `test` 绿。
