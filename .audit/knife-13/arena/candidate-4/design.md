# Knife 13 — Candidate 4: Hierarchical Style Builders, Span Stream & LineWidthBudget

## Usage（调用方视角）

### 1. 提案书 §1 模板与正文混排

```rust
use quire::{document, Length, NonNegative, PageSize, Positive, TextAlign};

let pdf = document(PageSize::A4, |doc| {
    // 注册基准字体与粗体 face
    doc.font(regular_ttf)?;      // FontId(0)，段落正文基准
    doc.bold_font(bold_ttf)?;    // 绑定粗体为 FontId(1)；未调用则 p.bold 在测量期返回 UnknownFont

    // 提案书 §1 原生闭包模板：正文混排同一段
    doc.paragraph(|p| {
        p.text("合同编号 ");
        p.bold(&contract.id);
    });

    doc.paragraph(|p| p.text("按乙方案执行。"));
})?;
```

- 调用方无需管理底层 `FontId`，通过 `p.text` 与 `p.bold` 表达语义 run。
- 同一段内的 `text` 与 `bold` 在同一个块中联合测量、断行、分页，共享同一个段后距。

---

### 2. 流级样式构建（Flow-level Style Builder）与公文首行缩进

在公文、合同等长文本场景中，调用方无需在每个段落重复声明缩进或字号。可在 `doc` / `flow` 级别直接配置流级样式快照，后续段落与嵌套流自动继承：

```rust
let pdf = document(PageSize::A4, |doc| {
    doc.font(simsun_regular)?;
    doc.bold_font(simsun_bold)?;

    // 文档/正文流统一配置公文默认样式（字号五号 10.5 pt、行距 1.4、首行缩进约 21 pt）
    doc.style(|s| {
        s.font_size(Positive::pt(10.5)?);
        s.line_height_factor(1.4)?;
        s.first_line_indent(NonNegative::pt(21.0)?);
    });

    // 自动继承流级首行缩进与字号
    doc.paragraph(|p| {
        p.text("兹证明下列银行询证项经本机构核对无误，特此函复。");
    });

    doc.paragraph(|p| {
        p.text("本函复仅对截止指定日期之业务往来有效，详见下表：");
    });

    // 局部段落覆盖：标题居中且不缩进
    doc.paragraph(|p| {
        p.first_line_indent(NonNegative::ZERO);
        p.align(TextAlign::Center);
        p.bold("询证函回复明细");
    });
})?;
```

- `doc.style(|s| ...)` 作用于隐式 section 流；`Section` 与 `Flow` 亦支持 `.style(|s| ...)`。
- `p.first_line_indent(...)` 与 `p.align(...)` 提供段落局部覆盖能力。
- 首行缩进类型严格使用 `Length<NonNegative>`，从源头杜绝负数与 NaN。

---

### 3. 多栏与局部嵌套内的样式继承

```rust
doc.columns(|row| {
    // 继承外层 flow 的字体与字号，但可在栏内定制对齐
    row.col(|c| {
        c.paragraph(|p| {
            p.text("甲方签字：");
            p.bold("张三");
        });
    });
    row.col(|c| {
        c.style(|s| s.align(TextAlign::End));
        c.paragraph(|p| {
            p.text("签署日期：2026年10月9日");
        });
    });
});
```

---

### 本刀刻意拒绝（Non-Goals）

- **拒绝具名样式字典**：不实现 `"heading"` 等字符串具名表与 `HashMap<String, Style>`（提案书未确认，消除运行时 `UnknownStyle`）。
- **拒绝合成粗体（synthetic bold）**：粗体必须是独立注册的 `FontId(1)`。
- **拒绝 italics / decoration（下划线、删除线）/ §2.2 填空横线**：保持本刀专注。
- **拒绝两端对齐（Justify）**：仅支持 `Start` / `Center` / `End`（提案书 §3）。

---

## Shape

### 架构取舍与三处关键结构创新

不同于 Candidate 1 的“仅在 Paragraph 上加单个 setter + 并行字节范围 RunTape + 改写 break_lines 签名”，Candidate 4 采取如下深层架构切分：

1. **分层样式构建器（Hierarchical Style Builders）**：
   - 样式是值语义实体（`Style`），在 `Document`、`Flow`、`Paragraph` 三层自然流转。
   - `Flow.style` 是当前流的激活样式，派生嵌套块时直接按值 clone。
   - `Paragraph` 既可直接读取当前流样式，又支持局部覆盖（如 `p.first_line_indent`、`p.align`）。公文全篇仅需设置一次缩进，不需要 50 个段落各自写一遍。
2. **Span 枚举流（TextSpan Stream）消除字节偏移漂移**：
   - 不采用 `full_text: String` 搭配 `Vec<Range<usize>>` 的并行切片结构。`ClusteredText::new` 会进行 CRLF 规约（`\r\n` 变为 `\n`），硬换行 `\n` 还会被移出 `glyph_text`，平行字节索引极易在测量和切片时发生漂移甚至越界。
   - `Paragraph` 内部收集自包含的 `enum TextSpan { Regular(String), Bold(String) }`。
   - Lower 时合并相邻同类 Span，保留清晰的语义边界。测量时每个 Span 独立调用 `measure_text`，随后直接产出带 `FontId` 标注的 `MeasuredGlyph` 序列。
3. **`LineWidthBudget` 抽象解耦文本断行引擎**：
   - `src/text/line_break.rs` 是纯文本断行引擎（Unicode UAX #14 / UAX #29），**不应知道段落样式、缩进或 Style 的概念**。
   - 抽象轻量 `trait LineWidthBudget`：
     - `impl LineWidthBudget for f32` 保持单行宽场景 100% 向后兼容。
     - `DualWidth { first: f32, rest: f32 }` 为首行缩进提供两段式行宽预算。
   - 第一行使用 `W' = W - indent`，若发生溢出，`UnbreakableWiderThanFrame` 的 `avail` 精准暴露 `W'`。

---

### 模块职责与数据流向

```text
[Document Builder]
  ├── doc.font(bytes)      ──> FontStore::register (id=0, regular)
  ├── doc.bold_font(bytes) ──> FontStore::register (id=1, bold)
  └── doc.style(|s| ...)   ──> 更新 Section.flow.style
           │
           ▼
[Flow::paragraph(|p| ...)]
  ├── p.text(...) / p.bold(...)   ──> Paragraph.spans: Vec<TextSpan>
  └── p.first_line_indent(...)    ──> Paragraph.style_patch
           │
      lower 阶段
           │ (merge style + coalesce adjacent spans)
           ▼
[FlowNode::Text]
  └── TextNode { spans: Vec<TextSpan>, style: Style }
           │
      TextNode::split(Frame { width: W, ... })
           │
           ▼
[ParagraphNode::layout]
  ├── 1. 几何校验: W - indent <= 0 ──> Err(InvalidGeometry)
  ├── 2. 测量: 对每个 Span 依其 FontId 调用 measure_text
  │      - 缺字 ──> Err(MissingGlyph) (提案书 §4)
  │      - 未注粗体 ──> Err(UnknownFont { id: 1 })
  ├── 3. 断行: break_lines(clustered, DualWidth { first: W - indent, rest: W }, width_of)
  │      - 首行不可断内容溢出 ──> Err(UnbreakableWiderThanFrame { avail: W - indent })
  ├── 4. 组装行盒 LineBox:
  │      - 首行对齐原点: x = indent + align_x(W - indent, line_width, align)
  │      - 续行对齐原点: x = align_x(W, line_width, align)
  │      - 一行内部按连续相同 FontId 分割为多个 GlyphRun (共享同一基线)
           │
           ▼
[ParagraphNode { lines: Vec<LineBox>, line_height, spacing_after, width: W }]
```

---

### 核心类型与公共 API 签名

#### 1. 公共接口定义（`src/document.rs` & `src/lib.rs`）

```rust
// --- src/document.rs ---

impl<'a> Document<'a> {
    /// 注册主正文字体（首个注册为 FontId(0)）。
    pub fn font(&mut self, bytes: Vec<u8>) -> Result<()>;

    /// 注册粗体字体，并绑定为文档 Bold 角色（FontId(1)）。
    /// 若正文出现 `p.bold` 但未调用此方法，将在排版阶段报错。
    pub fn bold_font(&mut self, bytes: Vec<u8>) -> Result<()>;

    /// 配置隐式正文段落流的默认样式。
    pub fn style(&mut self, f: impl FnOnce(&mut StyleBuilder) -> Result<()>) -> Result<()>;
}

impl Flow {
    /// 配置当前流后续段落与子流的默认样式。
    pub fn style(&mut self, f: impl FnOnce(&mut StyleBuilder) -> Result<()>) -> Result<()>;

    /// 添加段落。段落自动继承当前流样式，闭包内可进行内容填充与局部样式覆盖。
    pub fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph));
}

/// 段落构建器：承载段落内容（文本/粗体）与段落级样式覆盖。
#[derive(Default)]
pub struct Paragraph {
    spans: Vec<PendingSpan>,
    style_patch: StylePatch,
}

impl Paragraph {
    /// 追加普通正文文本。
    pub fn text(&mut self, s: impl AsRef<str>) {
        self.spans.push(PendingSpan::Regular(s.as_ref().to_owned()));
    }

    /// 追加粗体文本。
    pub fn bold(&mut self, s: impl AsRef<str>) {
        self.spans.push(PendingSpan::Bold(s.as_ref().to_owned()));
    }

    /// 覆盖此段落的首行缩进（默认继承流样式；流默认值为 0）。
    pub fn first_line_indent(&mut self, indent: Length<NonNegative>) {
        self.style_patch.first_line_indent = Some(indent);
    }

    /// 覆盖此段落的对齐方式（默认继承流样式；流默认值为 Start）。
    pub fn align(&mut self, align: TextAlign) {
        self.style_patch.align = Some(align);
    }

    /// 覆盖此段落的行距倍数（如 1.4）。
    pub fn line_height_factor(&mut self, factor: f32) -> Result<()> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err(Error::InvalidGeometry);
        }
        self.style_patch.line_height_factor = Some(factor);
        Ok(())
    }

    /// 覆盖此段落的段后间距。
    pub fn paragraph_spacing(&mut self, spacing: Length<NonNegative>) {
        self.style_patch.paragraph_spacing = Some(spacing);
    }
}

/// 样式构建器，用于在 Document 或 Flow 级别配置默认样式。
pub struct StyleBuilder<'a> {
    target: &'a mut Style,
}

impl StyleBuilder<'_> {
    pub fn font_size(&mut self, size: Length<Positive>) -> &mut Self {
        self.target.font_size = size;
        self
    }

    pub fn line_height_factor(&mut self, factor: f32) -> Result<&mut Self> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err(Error::InvalidGeometry);
        }
        self.target.line_height_factor = factor;
        Ok(self)
    }

    pub fn color(&mut self, color: Color) -> &mut Self {
        self.target.color = color;
        self
    }

    pub fn paragraph_spacing(&mut self, spacing: Length<NonNegative>) -> &mut Self {
        self.target.paragraph_spacing = spacing;
        self
    }

    pub fn align(&mut self, align: TextAlign) -> &mut Self {
        self.target.align = align;
        self
    }

    pub fn first_line_indent(&mut self, indent: Length<NonNegative>) -> &mut Self {
        self.target.first_line_indent = indent;
        self
    }
}
```

---

#### 2. 内部样式与节点模型（`src/layout/style.rs` & `src/layout/paragraph.rs`）

```rust
// --- src/layout/style.rs ---

/// 解析后的段落样式（值类型，不可变快照）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Style {
    pub(crate) font: FontId,                       // 基准字体 (FontId(0))
    pub(crate) font_size: Length<Positive>,         // 默认 12 pt
    pub(crate) line_height_factor: f32,            // 默认 1.2
    pub(crate) color: Color,                       // 默认 RGB(0,0,0)
    pub(crate) paragraph_spacing: Length<NonNegative>, // 默认 0 pt
    pub(crate) align: TextAlign,                   // 默认 Start
    pub(crate) first_line_indent: Length<NonNegative>, // 默认 0 pt (NEW)
}

// --- src/layout/paragraph.rs ---

/// 具有明确字体绑定的语义文本 Span。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextSpan {
    pub(crate) text: String,
    pub(crate) font: FontId,
}

/// 待排版的段落块。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextNode {
    pub(crate) spans: Vec<TextSpan>,
    pub(crate) style: Style,
}

/// 一行排版输出：包含多个按连续同字体切分的 GlyphRun，共用同一基线。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineBox {
    pub(crate) runs: Vec<GlyphRun>,
}

/// 段落排版结果。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParagraphNode {
    pub(crate) lines: Vec<LineBox>,
    pub(crate) line_height: f32,
    pub(crate) spacing_after: f32,
    pub(crate) width: f32, // 记录完整排版外框宽 W，非 W'
}
```

---

#### 3. 解耦的断行接口（`src/text/line_break.rs`）

```rust
// --- src/text/line_break.rs ---

/// 行宽预算提供者：供断行器查询指定视觉行的可用宽度。
pub(crate) trait LineWidthBudget {
    fn avail_for_line(&self, line_index: usize) -> f32;
}

/// 单一固定行宽实现（100% 兼容现有断行调用方与单元测试）。
impl LineWidthBudget for f32 {
    #[inline]
    fn avail_for_line(&self, _line_index: usize) -> f32 {
        *self
    }
}

/// 首行缩进两段式行宽预算。
#[derive(Debug, Clone, Copy)]
pub(crate) struct DualWidth {
    pub(crate) first: f32,
    pub(crate) rest: f32,
}

impl LineWidthBudget for DualWidth {
    #[inline]
    fn avail_for_line(&self, line_index: usize) -> f32 {
        if line_index == 0 {
            self.first
        } else {
            self.rest
        }
    }
}

/// 纯文本断行算法：接收任意符合 `LineWidthBudget` 的行宽模型。
pub(crate) fn break_lines<'text, B: LineWidthBudget>(
    text: &'text ClusteredText,
    budget: B,
    mut width_of: impl FnMut(Cluster<'text>) -> f32,
) -> Result<BrokenLines<'text>> {
    let line0_avail = budget.avail_for_line(0);
    if !line0_avail.is_finite() || line0_avail <= 0.0 {
        return Err(Error::InvalidGeometry);
    }
    // ...
    // pack_lines 内部：
    // let current_avail = budget.avail_for_line(records.len());
    // 当行内不可断内容超出 current_avail 时：
    // Err(Error::UnbreakableWiderThanFrame { width, avail: current_avail })
}
```

---

### 排版执行流水线（Layout Algorithm）

在 `ParagraphNode::layout(fonts, spans, style, frame_width)` 中执行：

1. **几何前置检验**：
   - 提取缩进量 `indent = style.first_line_indent.as_pt()`。
   - 若 `frame_width <= 0.0` 或 `frame_width - indent <= 0.0`，立即返回 `Err(Error::InvalidGeometry)`。
   - 所有检查均在 `start_page` 之前完成。

2. **多 Span 测量与全局字形流映射**：
   - 拼接各 Span 文本为总文本 `full_text`，构建 `ClusteredText::new(&full_text)`。
   - 由于 Unicode grapheme 不会跨 Span 分割（相邻同类 Span 已合并），测量器为每个 Span 分别调用 `measure_text(fonts, span.font, &span.text)`。
   - 检验字体：若 `span.font` 对应 face 未注册，返回 `Err(Error::UnknownFont { id: span.font })`；若某字符未在 cmap 命中，返回 `Err(Error::MissingGlyph { font, scalar })`（严格落实提案书 §4 缺字整份失败）。
   - 测量结果直接保留每个 glyph 的 `(glyph_id, advance_norm, font_id, byte_range)`。

3. **变宽断行**：
   - 构造 `DualWidth { first: frame_width - indent, rest: frame_width }`。
   - 传递给 `break_lines`。
   - `line 0` 处可用宽度为 `W - indent`；后续各行可用宽度恢复为 `frame_width`。

4. **基准指标与基线**：
   - 采纳自主权默认值 `regular-metrics`：整段的 `line_height` 与 `baseline` 均由基准字体 `style.font` 的度量计算（`line_metrics`）。
   - 混排的一行内，所有字体共用同一基线。

5. **生成多段 `GlyphRun`**：
   - 对断行器生成的每一条 `BrokenLine`：
     - 若为首行（`line_index == 0`）：
       - 内容宽度为 `line.width()`；
       - 行内起点 `origin_x = indent + align_x(frame_width - indent, line.width(), style.align)`；
     - 若为续行（`line_index > 0`）：
       - 行内起点 `origin_x = align_x(frame_width, line.width(), style.align)`；
     - 在该行内，将连续相同 `font_id` 的字形聚类为一个 `GlyphRun`：
       - 第一个 run 的 `x = origin_x`；
       - 后续每个 run 的 `x = prev_run.x + prev_run.width`；
       - 每个 run 的 `origin = Point::try_pt(run_x, baseline)`。
     - 最终该行包装为 `LineBox { runs: vec![run1, run2, ...] }`。

6. **分页切片（Pagination Split）**：
   - `ParagraphNode.width` 记录完整框宽 `frame_width`（确保在跨页迁移时不被放进更窄的框中）。
   - 跨页切片时直接复制已计算好 `origin` 的 `LineBox` 集合，**后续页面的首行不会被二次应用首行缩进**（首行缩进仅烘焙进段落视觉第 0 行）。

---

## 边界与不变式验证

| 约束 | 保证机制 | 失败时机 |
| --- | --- | --- |
| 首行缩进非负 | `Length<NonNegative>` 类型化约束，拒绝负数与 NaN | 构造期 / DSL 组装期 |
| 字号正数 | `Length<Positive>` 类型化约束，拒绝 `<= 0` 与 NaN | 构造期 / DSL 组装期 |
| `W - indent <= 0` | 几何前置预检返回 `Error::InvalidGeometry` | `TextNode::split` 规划期（早于 `start_page`） |
| 首行不可断内容超限 | `break_lines` 校验 `width <= avail`，报错携带 `avail = W - indent` | `break_lines` 阶段（早于 `start_page`） |
| 粗体字体缺失 | 测量期查表 `fonts.face(FontId(1))`，未注册返回 `Error::UnknownFont` | 测量期（早于 `start_page`） |
| 粗体字形缺失 | 测量期 charmap 缺失返回 `Error::MissingGlyph`，不回退、不出 `.notdef` | 测量期（早于 `start_page`） |
| 跨页后缩进不重复 | 首行缩进坐标在 `ParagraphNode::layout` 中静态烘焙进第 0 行，跨页仅切片行盒 | 结构上不可被重复计算 |
