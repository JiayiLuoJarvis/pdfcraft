# Knife 14: 填空下划线设计决策与替代方案权衡（Rationale）

本文档记录候选方案 4 在设计 Knife 14（提案书 §2.2 行内填空下划线）过程中深入考虑并最终否决的替代方案，以及选择当前架构的根本技术理由。

---

## 1. 字符级伪装方案 vs 独立行内项联合（InlineItem）

### 考虑的方案
- **方案 A（下划线字形伪造）：** 将 `blank(pt)` 换算为若个下划线字符 `_`（U+005F），直接作为普通文本进入 `cluster_tagged` 和 skrifa 字形渲染管线。
- **方案 B（空格 + 样式下划线）：** 将空白填充为普通空格（U+0020）或全角空格（U+3000），并通过某种文本装饰画线。

### 为什么否决
1. **cmap 依赖与缺失字形风险：** `_` 必须经过 skrifa 的 charmap 查表。若调用方配置的中文字体或特定嵌入字体不含 U+005F，排版将直接抛出 `MissingGlyph` 错误导致文档生成中断，完全违背“填空下划线不进字形测量”的铁律。
2. **定宽不可控与亚点精度丢失：** 字符的 advance 受到字体设计网格的离散量化约束（`advance * font_size`），无法精确满足调用方要求的物理尺寸（如 `Length::pt(60.0)` 或 `Length::mm(25.0)`）。
3. **断行行为完全错乱：**
   - 若用普通空格 U+0020，断行器中的 `PairClass::Sp` 规则会在行首无条件丢弃连续空格（`pack_lines` 行首 drop 逻辑），导致换行后下划线凭空消失。
   - 若用 U+3000，其断行类别为 `Id`，会在连续汉字间任意拆断，无法保证“定宽 Blank 作为不可拆整体”的产品规范。
4. **铺到行尾（ToEndOfLine）无法预知字符数：** 在断行前根本不知道本行剩余多少 pt，不可能预先填入正确数量的字符。

### 采纳的设计
采用**结构化行内联合 `InlineItem::Text` 与 `InlineItem::Blank`**。Blank 项彻底绕过 `cluster_tagged`、cmap 查询与 skrifa 测量，仅在断行器中作为确定点数的水平占位原子参与排版，确保了 100% 的几何精度与字体无关性。

---

## 2. 块级 `FlowNode::HLine` 侵入 vs 行内 `FragmentCommand::Stroke`

### 考虑的方案
尝试复用已有的块级水平分隔线 `HLine`（`src/layout/hline.rs`），将其作为某种微型流节点嵌入段落。

### 为什么否决
1. **概念错位：** `HLine` 是块级（Block-level）流节点，其父级是 `Stack`，具有独立的分页能力（`Split::MoveToNextPage`）。填空下划线是行内（Inline-level）占位，生命周期从属于单个 `LineBox`。
2. **高度计算冲突：** `HLine` 会贡献 `space_before + thickness + space_after` 的物理高度给整个文档流，导致垂直游标下移；而提案书 §2.2 严格规定“填空下划线不增加行高”。
3. **对齐模型完全不同：** `HLine` 的对齐是相对于页面框宽或列宽的（`Start`/`Center`/`End`），居中时线会跑去整栏的正中央；而行内下划线的起点必须严格钉在它前面文字结束的当前笔触位置 `current_x`。

### 采纳的设计
**仅复用绘制底层命令，绝对不复用节点。** 布局期在 `LineBox` 中直接维护 `BlankStroke`。在 `fragment_from_lines` 阶段，利用已有的 `FragmentCommand::Stroke` 发射一条水平线段。该线段在 `flow.rs` 的平移与 `krilla_canvas.rs` 的绘制管线中已具备原生支持，既做到了零冗余代码，又彻底划清了块与行的架构红线。

---

## 3. Fill Blank 宽度解析时机：组装期 vs 断行器 Packing

### 考虑的方案
在 `Paragraph::blank_to_end()` 组装时，或者在 `measure_inline` 阶段就试图计算并填充其具体宽度。

### 为什么否决
1. **组装期未知框宽：** `Paragraph` 在闭包执行时根本不知道自己未来会被放入普通页面正文（例如 A4 宽 451.27 pt）、某一分栏（例如 100 pt）、还是表格单元格（例如 80 pt）。
2. **测量期未知行内断点：** 即使 `TextNode::split` 第一次获得了框宽 `W`，在实际运行断行算法之前，根本无法预知前序文字在何处发生折行，因此无法得知当前是第几行、本行已被前序文字占用了多少宽度。
3. **首行与后续行预算差异：** 首行缩进会使得首行可用预算变为 `W' = W - indent`，若文本在 fill 前折到了第 2 行，预算则是 `W`。

### 采纳的设计
将 `BlankFill` 作为专用原子 `Atom::BlankFill` 送入 `pack_lines`。在贪心装包过程中，当且仅当处理到该原子时，通过 `avail - line_width` 即时计算本行的剩余物理宽度 `w_remain`。并将计算得到的物理宽度直接累加进 `LineRecord.width`。这样使得整行的总宽度恰好等于 `avail`，确保随后执行 `Center` 或 `End` 段落对齐时，文字与下划线绝对不会发生任何相对位移。

---

## 4. 中间 Blank 对后续字形的水平位移方案

### 考虑的方案
- **方案 A（保持连续 glyph_range，仅在绘制时偏移）：** 断行器只给整行报告一个 `glyph_range`，但在绘制时根据某个外部记录去给特定字形打补丁。
- **方案 B（拆分字形段落 Tape）：** 让每行保存结构化的 `LineSegment` 切片。

### 为什么否决方案 A
当前 `glyph_runs_for_line` 是基于单调递增的字形切片连续推进 `x` 的。如果中间隔着一个 Blank，左右两段文字在字形数组中其实是紧邻的。若不显式切断字形区间，后续文字的 `GlyphRun` 将紧挨着前序文字绘制，导致 Blank 虽然占了行宽，但下划线画在空白处，文字却挤在左侧（字线重叠或错位）。

### 采纳的设计（方案 B）
将断行结果表示为有序的 `LineSegment` 序列：
- `LineSegment::Glyphs(range)`
- `LineSegment::Blank { item_index, width }`

在构建 `LineBox` 时，维护一个单调推进的游标 `current_x`：
1. 绘制前半段文字，`current_x += glyphs_advance`；
2. 遇到 Blank，记录起点为 `current_x`、终点为 `current_x + width` 的 `BlankStroke`，随后执行 `current_x += width`；
3. 紧接着绘制后半段文字时，其第一个字形的起点即是更新后的 `current_x`。

这一设计干净、确定、直截了当地保证了后续字形向右产生精确等于 Blank 宽度的位移。

---

## 5. 断行器原子与分类设计：Unicode PairClass 矩阵 vs 独立 Blank 原子

### 考虑的方案
在 `break_class.rs` 的 `PairClass` 枚举中增加一项 `Blank`，并将其扩展到已有的二维断行决策转移矩阵（`allows(prev, next)`）中。

### 为什么否决
1. **污染 UAX 规范分类系统：** `PairClass` 是专门用于表达真实 Unicode 字符间（标点、字母、汉字等）的折行分类法。把排版引擎合成的行内几何占位塞入该状态机，会成倍扩大矩阵大小（由 7x7 变为 8x8 甚至更大），增加推理复杂度和测试维护成本。
2. **Blank 的断行语义具有独特性：**
   - Blank 作为整体不可拆分（自身内部决不发生换行）；
   - 根据自主默认规范，Blank **允许在紧邻其前方断行**（即整条线移至下一行），也**允许在紧邻其后方断行**；
   - Blank 绝对不能被当成 `Sp` 丢弃。

### 采纳的设计
保持 `break_class.rs` 的纯净。在 `src/text/line_break.rs` 中，直接扩展 `Atom` 枚举引入 `Atom::BlankFixed` 与 `Atom::BlankFill`。在 `nearest_break` 与 `pack_lines` 中直接针对 Blank 判定断点位置：
- 当遭遇溢出时，若当前位置是 Blank 且本行已有内容，`split = i` 直接命中有效断点，Blank 整体换入下一行；
- 若 Blank 在行首即超过当前行预算 `avail`（首行为 `W'`，后续行为 `W`），立即且确定地构造并返回 `Error::UnbreakableWiderThanFrame { width, avail }`。

---

## 6. 行盒数据结构：通用 `FragmentCommand` 列表 vs 专用 `BlankStroke`

### 考虑的方案
直接将 `LineBox` 改为保存 `Vec<FragmentCommand>`，在 `layout_runs` 阶段就生成所有命令。

### 为什么否决
1. **跨页垂直平移繁琐：** `LineBox` 在第一次生成时只知道行局部坐标（`line-local`，y 为基线）。垂直分页切片由 `ParagraphNode::split` 在后期的分页循环中完成。如果提前构造了通用的 `FragmentCommand::Stroke`，后续每次切行、平移或计算段前段后距时，都需要对 `Path` 重新解构或平移。
2. **轻量与可测试性：** `BlankStroke` 仅包含 5 个标量字段（`x_start`, `x_end`, `y`, `thickness`, `color`），内存紧凑、极易在单元测试中直接比对数值。

### 采纳的设计
`LineBox` 仅维护 `runs: Vec<GlyphRun>` 与 `strokes: Vec<BlankStroke>`。在最终真正需要生成页面渲染命令的 `fragment_from_lines` 出口处，一次性将 `y = line_index * line_height + stroke.y` 计算完毕并生成 `FragmentCommand::Stroke`。这与现有 `GlyphRun` 的基线平移机制保持了绝对对称。

---

## 7. 组装期非法状态防御：即时 Panic vs 错误暂存

### 考虑的方案
- **方案 A（Panic）：** 当调用方在 `blank_to_end()` 后调用 `text("foo")` 时，直接 `panic!("run after fill blank")`。
- **方案 B（返回 Result）：** 将 `p.text()` 等所有 builder 方法签名改为返回 `Result<()>`。

### 为什么否决
1. **否决 Panic：** `quire` 是通用排版库，组装数据通常来自外部业务系统的动态输入，库代码中必须遵循无 panic 规范，任何排版与几何非法均应转化为严谨的 `Error`。
2. **否决返回 Result：** 提案书 §1 与当前 `src/document.rs` 风格中，`Paragraph` 的所有 builder（如 `text`、`bold`、`first_line_indent`）均为 `&mut self` 且返回 `()`，保持流畅自然的闭包书写体验。若仅为了拦截 fill 后的非法追加而将所有方法改为返回 `Result`，将极大破坏 DSL 的人体工程学。

### 采纳的设计
采用与 `Flow` / `Layer` 完全一致的**错误暂存模式**：
- `Paragraph` 内部维护 `error: Option<Error>` 和 `has_fill_blank: bool`；
- 在 `blank_to_end()` 之后若发生非空文本或 Blank 的追加，直接记录 `self.error = Some(Error::InvalidGeometry)`；
- 空文本（`text("")`）不产生有效 run，不触发该错误；
- 在 `Flow::paragraph` 闭包退出时，提取暂存的错误并通过 `self.push(Err(err))` 注入 `Flow` 现有的错误管线；
- 整个流程在进入 `paint` 之前立刻返回失败，保证不会生成任何残缺页面或脏画布，兼顾了极致的人体工学与严密的安全性。

---

## 8. 错误变体选用：复用 `InvalidGeometry` vs 新增专用变体

### 考虑的方案
在 `src/error.rs` 中新增枚举变体 `Error::RunAfterFillBlank`。

### 为什么否决
根据 frame rubric 2 明确规定：
> "Illegal widths unrepresentable or fail at assemble/layout with existing error variants where they fit. Public API small."

在现有 `Error` 枚举中：
- 非法宽度（负数、NaN）已被类型系统 `Length<NonNegative>` / `Length<Positive>` 在构造期完全消除；
- 填空超出可用框宽已被现有变体 `Error::UnbreakableWiderThanFrame` 完美覆盖；
- 铺到行尾后追加内容属于非法的几何排布声明，完全符合现有 `Error::InvalidGeometry` 的语义。

复用 `Error::InvalidGeometry` 可以保持 `Error` 公共枚举紧凑精炼，不增加多余的公共接口负担。

---

## 9. 恪守 10 项自主默认值（Autonomy Defaults）的理由

本设计无一例外地完全遵从了 `frame.md` 所列出的全部 10 项自主默认值，未触发任何一项反转词（Reverse word）：

| 默认决策项 | 本设计取值 | 不反转的理由 |
| :--- | :--- | :--- |
| **线粗（Thickness）** | `1 pt` (`Length<Positive>`) | 与公文、合同填空线的常规视觉习惯一致，与块级 `HLine` 默认值相符，避免过细（0.5 pt 在某些打印机上发虚）。 |
| **基线偏移（Baseline Offset）** | 基线下方 `1 pt`（y 向下） | 刚好避开汉字底边和英文降笔（如 g, p, y 的下沉部分），防止切字。 |
| **默认线色（Stroke Color）** | 继承 `style.color` | 自然契合正文段落文字颜色；当样式为黑时即为纯黑，样式为自定义色时整体统一。 |
| **定宽类型（Fixed Span Type）** | `Length<NonNegative>`（允许 0 pt） | 符合 §2.0 类型化约束。允许 0 pt 可避免动态计算宽度为 0 时的非必要报错；0 pt 时自动省略 stroke 命令，逻辑自洽。 |
| **溢出预算（Overflow Budget）** | 首行为 `W'`，后续行为 `W` | 严格贯彻提案书 §2.1 “换行可用宽”原则；首行缩进必然扣除首行可用空间，不能在首行偷跑整宽 `W`。 |
| **定宽折行（Fixed Blank Wrap）** | 允许在 Blank 前后折行，绝非 `PairClass::Sp` | 保证 Blank 作为整体下移；行首 Blank 绝不被误判为普通空格而遭丢弃。 |
| **剩余宽为 0（`W_remain == 0`）** | 省略 stroke，消费 0 宽，不报错 | 当文本恰好占满本行时，调用 `blank_to_end()` 表达的是“铺到本行结束”，此时剩余空间为 0，静默结束是最高容错且符合预期的行为。 |
| **汉字字数糖（Han Sugar）** | 布局期计算 `n * style.font_size` | 顺应未来段落字号样式的变化，不让 `Paragraph` 组装期耦合 `Style` 内部细节。 |
| **Fill API 形式** | 专用方法 `blank_to_end` 与定宽分离 | 在类型与方法语义上彻底实现互斥（Fixed XOR Fill），消除非法状态。 |
| **Cell / Band 范围** | 本 Knife 严格排除 Cell / Band | 严格遵守 Knife 14 范围与渐进开发边界，避免越界修改无关模块。 |
