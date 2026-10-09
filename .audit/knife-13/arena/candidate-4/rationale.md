# Knife 13 — Candidate 4: Rationale

## Problem

在 `feat/style-builders @ f208329`，段落排版被硬编码为单一已解析样式快照（默认 12 pt、1.2 行高、黑色、FontId(0)、无缩进），且整段文字被拼平为一个 `String`，无法区分段内 run，更无法设置任何样式属性。实现 knife 13（样式构建器 + `p.bold` + 首行缩进）的核心难点在于：如何在不引入冗余复杂的具名样式表（Named Styles Map）与层叠解析器的前提下，让调用方免于在几十个段落上重复配置样式；如何在同一行内混排独立字体的粗体字形并精准维持基线对齐；以及如何将首行缩进（`W' = W - indent`）正确注入既有的纯文本断行引擎，且确保所有几何与字体错误严格在开页绘制（`start_page`）之前返回。

## Usage (caller's view)

```rust
use quire::{document, Length, NonNegative, PageSize, Positive, TextAlign};

let pdf = document(PageSize::A4, |doc| {
    // 1. 字体注册：基准字体与专用粗体 face
    doc.font(simsun_regular)?;
    doc.bold_font(simsun_bold)?;

    // 2. 流级样式构建：统一声明公文排版风格（字号、行距、首行缩进），消除段落重复配置
    doc.style(|s| {
        s.font_size(Positive::pt(10.5)?);
        s.line_height_factor(1.4)?;
        s.first_line_indent(NonNegative::pt(21.0)?);
    });

    // 3. 提案书 §1 原生闭包语法：段内混排粗体 run，自动继承流级样式
    doc.paragraph(|p| {
        p.text("合同编号：");
        p.bold(&contract.id);
    });

    // 4. 普通正文段落：自动继承首行缩进，首行 W' = W - indent，续行满宽 W
    doc.paragraph(|p| {
        p.text("兹证明下列银行询证项经本机构严格核对无误，特此函复如下：");
    });

    // 5. 局部覆盖：针对特定标题段落，覆盖首行缩进与对齐方式
    doc.paragraph(|p| {
        p.first_line_indent(NonNegative::ZERO);
        p.align(TextAlign::Center);
        p.bold("询证函回复明细表");
    });
})?;
```

调用方仅接触直观的闭包式构建器与非负/正长度类型（`Length<NonNegative>` / `Length<Positive>`）。不暴露内部 `FontId`、不暴露 `TextNode` 与 `LineWidthBudget`。

## Shape

本设计的核心架构决策与数据流动如下：

1. **值语义样式与分层流转（Hierarchical Style Builders）**：
   - 数据结构：`Style` 是 crate 内部解析完毕的值对象，包含 `font`, `font_size`, `line_height_factor`, `color`, `paragraph_spacing`, `align`, `first_line_indent`。
   - 传播机制：`Flow` 持有当前的 `Style` 快照。调用 `doc.style(|s| ...)` 或 `flow.style(|s| ...)` 变更当前流的基准样式，派生子流（如分栏、列表）时直接按值 clone。`Paragraph` 在 `Flow::paragraph` 执行时拿到 `flow.style` 的副本，闭包内仅记录差量覆盖（`StylePatch`），在 lower 阶段生成最终的 `TextNode`（*per boundary-discipline*）。
   - 接口深度：公共表面仅暴露轻量链式构建器 `StyleBuilder` 与段落局部方法，隐藏了深层的样式快照复制与块级流转复杂度。

2. **自包含 Span 枚举流（TextSpan Stream）**：
   - 数据结构：`Paragraph` 收集 `PendingSpan::Regular(String)` 与 `PendingSpan::Bold(String)`，lower 时相邻同类合并为 `TextSpan { text: String, font: FontId }`。
   - 拒绝脆弱的并行字节切片（Parallel Byte-Range Vecs）：既有管线中 `ClusteredText::new` 会进行 CRLF 换行符规约（`\r\n` 缩短为 `\n`），且硬换行符不进入 `glyph_text`。使用并行 `byte_range` 极易导致坐标漂移或越界切片。Span 独立切分后，每个 Span 独立调用 `measure_text`，生成明确标注 `font_id` 的字形序列（*per encode-lessons-in-structure*）。

3. **解耦的断行预算契约（`LineWidthBudget`）**：
   - 数据结构：`src/text/line_break.rs` 引入 `trait LineWidthBudget { fn avail_for_line(&self, line_index: usize) -> f32; }`。
   - 适配层：`f32` 实现此 trait 提供恒定宽（保持现有代码 100% 兼容）；`DualWidth { first, rest }` 提供两段式行宽。
   - 职责单一：断行模块保持为纯粹的 Unicode 断行状态机，完全不知晓段落缩进、Style 等上层业务概念（*per laziness-protocol & minimize-reader-load*）。

4. **粗体 face 绑定与严格断言**：
   - `Document::bold_font` 注册第二个字体 face 并绑定角色。若未注册而调用了 `p.bold`，排版测量期返回 `Error::UnknownFont { id: 1 }`。
   - 粗体字符缺失严格按照提案书 §4 返回 `Error::MissingGlyph`，坚决不进行字体回退，不绘制 `.notdef`。
   - 几何检验：`W - indent <= 0` 在进入断行前立即拦截并返回 `Error::InvalidGeometry`。所有错误均在 `canvas.start_page` 之前返回。

## Synthesis decision

*(待 arena 综合决策评审阶段填充：记录本方案作为基准或被借鉴/放弃的决策细节)*

## Tradeoffs accepted

- **我们接受在 `Document` 和 `Flow` 上增加 `.style(|s| ...)` 方法，以换取公文长文档无需在每个段落重复声明样式的简洁性**：虽然略微增加了 `Flow` 的 API 表面，但彻底消除了调用方在几十个公文段落重复写 `.first_line_indent(two_em)` 的样板代码。
- **我们接受 `LineBox` 内部持有 `Vec<GlyphRun>` 而非单一 `GlyphRun`，以换取单行多字体混排与纯净的 baseline 统一**：一行拆分为多个同基线 run 使得绘制命令略有增加，但保证了每个 run 的字体、advance 与颜色完全同构，直接契合现有 Canvas 指令。
- **我们接受段落基准度量（`regular-metrics`）主导整行行高与基线，以换取稳定的公文行距**：当粗体 face 的 ascent/descent 略大时，不动态膨胀混排行的行高，避免正文中突然出现单行行距突变的视觉抖动。
- **我们暂不实现具名样式字典（Named Styles Map）与样式继承树，以换取零动态字符串查询与确定性的编译期类型安全**：按值传递与流级覆盖完全能满足当前公文与合同需求，不提前实现未经验证的 CSS 式层叠系统。

## Alternatives considered

- **仅在 `Paragraph` 上提供样式设置器（Candidate 1 方案）**：
  - *为何落选*：接口过于浅薄且缺乏实用深度。如果一个 20 页的公文合同包含上百个段落，调用方必须在闭包中对每一个段落手动调用 `p.first_line_indent(indent)`。这在真实生产场景中极其痛苦且容易遗漏，违背了 `Flow` 树状层级应当自然继承样式的设计初衷。
- **并行文本切片磁带（Parallel Byte-Range Vecs / RunTape）**：
  - *为何落选*：内部脆弱性极高。由于 `ClusteredText` 在进行 UAX #29 字素簇聚类前会规范化 CRLF 换行符（使得字符串长度收缩），且硬换行符不被纳入 `glyph_text`，并行的 `spans: Vec<Range<usize>>` 极易出现跨边界偏移或越界 panic。以 `TextSpan` 枚举流作为数据源不仅逻辑严密，且天然杜绝了切片漂移。
- **直接修改 `break_lines` 接收 `first_line_indent: f32` 参数**：
  - *为何落选*：产生严重的概念泄露（Information Leakage）。`text/line_break.rs` 是纯净的底层文本折行算法模块，不应了解“段落首行缩进”这种高级排版样式概念。采用 `LineWidthBudget` 特征既实现了首行与续行变宽断行，又保持了底层折行引擎的纯洁性与通用性（未来可无缝支持列表悬挂缩进或形状避让）。

## Open questions and risks

- 若未来引入列表项（List Items）自身的悬挂缩进（Hanging Indent），段落自身的首行缩进是与列表缩进直接相加（`W' = W_list - list_indent - first_line_indent`），还是在列表项内部禁止首行缩进？（目前建议作为正交层叠加处理）。
- 当调用方对同一段落混合使用极小字号与极大字号粗体时，基准字体的行高与基线是否可能引起视觉溢出？（本 knife 约定段内同字号混排粗体，跨字号段内混排暂延后）。

## Next implementation step

在 `src/text/line_break.rs` 中引入 `LineWidthBudget` 特征及其 `f32` 与 `DualWidth` 实现，使既有单元测试保持 100% 绿色的同时为变宽折行就绪。
