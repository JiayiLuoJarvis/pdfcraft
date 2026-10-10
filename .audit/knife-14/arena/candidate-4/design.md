# Knife 14: 填空下划线（Inline Blank）详细设计方案

**版本基线：** `feat/style-builders` @ `5deb0df` (feature tip `aaf14bf`)  
**权威规范：** `docs/提案书.md` §2.2（填空下划线）、§2.1（段落换行宽 `W` 与首行可用宽 `W'`）、§2.0（长度与颜色约束）  
**对齐标准：** 完全遵守 `.audit/knife-14/arena/frame.md` 裁决规则与全部 10 项自主默认值（Autonomy Defaults）。

---

## 1. 使用范例（Usage Sketch）

填空下划线是段落内与 `p.text` / `p.bold` 并列的行内元素，用于合同、公文中的签字、金额、日期预留横线。它不是给已有文字加 `decoration: underline`，也不是块级分隔线 `HLine`。

```rust
use quire::{document, Length, Color, PageSize};

let doc = document(PageSize::A4, |doc| {
    // 1. 定宽填空：指定绝对长度（pt / mm）
    doc.paragraph(|p| {
        p.text("借款人（签字）：");
        p.blank(Length::pt(120.0));
        p.text("（盖章）");
    });

    // 2. 汉字字数语法糖：约 N 个汉字宽（N * 当前字号），适应未来字号变化
    doc.paragraph(|p| {
        p.text("身份证号：");
        p.blank_em(18);
        p.text("联系电话：");
        p.blank_em(11);
    });

    // 3. 铺到行尾（ToEndOfLine）：自当前光标吃满本行剩余宽度 W_remain
    // 注意：吃到行尾必须是段落最后一个 run，后续禁止再追加内容
    doc.paragraph(|p| {
        p.text("补充协议条款：");
        p.blank_to_end();
    });

    // 4. 自定义样式：线粗、线色、基线偏移
    doc.paragraph(|p| {
        p.text("核验人：");
        p.blank_with(Length::pt(80.0), |b| {
            b.thickness(Length::pt(1.5));
            b.color(Color::rgb(180, 0, 0));
            b.baseline_offset(Length::pt(2.0));
        });
    });

    // 5. 首行缩进配合：首行预算为 W' = W - indent，后续行为 W
    doc.paragraph(|p| {
        p.first_line_indent(Length::pt(24.0));
        p.text("甲方确认银行账号：");
        p.blank_to_end();
    });
})?;
```

---

## 2. 公共 API 设计（Public API）

保持公共 API 紧凑、符合惯用 Rust 闭包 DSL 风格，遵循自主默认约定。

### 2.1 `Paragraph` 上的扩展方法

```rust
impl Paragraph {
    /// 追加定宽填空下划线。
    ///
    /// `width` 允许为 0 pt（此时不绘制横线，水平推进 0 pt）。
    pub fn blank(&mut self, width: Length<NonNegative>) {
        self.blank_with(width, |_| {});
    }

    /// 追加约 `count` 个汉字宽的填空下划线（在布局期按 `count * style.font_size` 解析）。
    pub fn blank_em(&mut self, count: u32) {
        self.blank_em_with(count, |_| {});
    }

    /// 追加铺到行尾的填空下划线（占据当前视觉行剩余宽 `W_remain = avail - line_width`）。
    ///
    /// 必须是段落最后一个 run；组装期若在其后追加任何非空内容，整个文档将在组装/排版期报错。
    pub fn blank_to_end(&mut self) {
        self.blank_to_end_with(|_| {});
    }

    /// 带样式配置的定宽填空下划线。
    pub fn blank_with(
        &mut self,
        width: Length<NonNegative>,
        f: impl FnOnce(&mut BlankBuilder),
    ) {
        let mut builder = BlankBuilder::default();
        f(&mut builder);
        self.append_blank(BlankWidth::Fixed(width), builder);
    }

    /// 带样式配置的汉字字数填空下划线。
    pub fn blank_em_with(
        &mut self,
        count: u32,
        f: impl FnOnce(&mut BlankBuilder),
    ) {
        let mut builder = BlankBuilder::default();
        f(&mut builder);
        self.append_blank(BlankWidth::Em(count), builder);
    }

    /// 带样式配置的铺到行尾填空下划线。
    pub fn blank_to_end_with(
        &mut self,
        f: impl FnOnce(&mut BlankBuilder),
    ) {
        let mut builder = BlankBuilder::default();
        f(&mut builder);
        self.append_blank(BlankWidth::ToEndOfLine, builder);
    }
}
```

### 2.2 样式构建器 `BlankBuilder`

```rust
/// 行内填空下划线的属性配置。
#[derive(Debug, Clone, PartialEq)]
pub struct BlankBuilder {
    pub(crate) thickness: Length<Positive>,
    pub(crate) color: Option<Color>,
    pub(crate) baseline_offset: Length<NonNegative>,
}

impl Default for BlankBuilder {
    fn default() -> Self {
        Self {
            // 自主默认值：线粗 1 pt
            thickness: Length::pt(1.0),
            // 自主默认值：默认继承当前文本样式色 style.color
            color: None,
            // 自主默认值：基线下方 1 pt（y 向下）
            baseline_offset: Length::pt(1.0),
        }
    }
}

impl BlankBuilder {
    /// 设置下划线粗细（默认 1 pt）。
    pub fn thickness(&mut self, thickness: Length<Positive>) -> &mut Self {
        self.thickness = thickness;
        self
    }

    /// 设置下划线颜色（默认继承当前段落文字颜色）。
    pub fn color(&mut self, color: Color) -> &mut Self {
        self.color = Some(color);
        self
    }

    /// 设置下划线相对于基线向下的偏移量（默认 1 pt）。
    pub fn baseline_offset(&mut self, offset: Length<NonNegative>) -> &mut Self {
        self.baseline_offset = offset;
        self
    }
}
```

---

## 3. 内部数据结构与类型定义（Types & Internals）

### 3.1 行内项联合 `InlineItem`

`Paragraph` 不再只保存 `Vec<TextRun>`，而是维护有序的 `InlineItem` 序列：

```rust
// src/layout/paragraph.rs

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InlineItem {
    Text(TextRun),
    Blank(BlankItem),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BlankItem {
    pub(crate) width: BlankWidth,
    pub(crate) thickness: Length<Positive>,
    pub(crate) color: Option<Color>,
    pub(crate) baseline_offset: Length<NonNegative>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BlankWidth {
    Fixed(Length<NonNegative>),
    Em(u32),
    ToEndOfLine,
}
```

### 3.2 组装期状态机（`Paragraph`）

```rust
// src/document.rs

pub struct Paragraph {
    pub(crate) items: Vec<InlineItem>,
    pub(crate) first_line_indent: Length<NonNegative>,
    pub(crate) has_fill_blank: bool,
    pub(crate) error: Option<Error>,
}

impl Default for Paragraph {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            first_line_indent: NonNegative::ZERO,
            has_fill_blank: false,
            error: None,
        }
    }
}
```

### 3.3 断行器原子扩展 `Atom`

```rust
// src/text/line_break.rs

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Atom {
    Hard { cluster: usize },
    Ink {
        pair: PairClass,
        width: f32,
        cluster: usize,
    },
    BlankFixed {
        width: f32,
        item_index: usize,
    },
    BlankFill {
        item_index: usize,
    },
}
```

### 3.4 换行后行内片段 `LineSegment` 与 `LineBox`

`BrokenLine` 向 `layout_runs` 传递可回放的行内片段结构：

```rust
// src/text/line_break.rs

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LineSegment {
    /// 连续字形范围，对应 MeasuredInline.glyphs 的切片
    Glyphs(std::ops::Range<usize>),
    /// 填空项及在当前视觉行最终解析出的物理宽度
    Blank {
        item_index: usize,
        width: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineRecord {
    pub(crate) segments: Vec<LineSegment>,
    pub(crate) width: f32,
}
```

`LineBox` 烘焙水平几何与待绘制命令：

```rust
// src/layout/paragraph.rs

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BlankStroke {
    pub(crate) x_start: f32,
    pub(crate) x_end: f32,
    pub(crate) y: f32, // baseline + baseline_offset
    pub(crate) thickness: Length<Positive>,
    pub(crate) color: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineBox {
    pub(crate) runs: Vec<GlyphRun>,
    pub(crate) strokes: Vec<BlankStroke>,
}
```

---

## 4. 模块职责地图（Module Map）

| 模块文件 | 现有职责 | Knife 14 接入点与变更 |
| :--- | :--- | :--- |
| `src/document.rs` | DSL 构建器、`Paragraph`、`Flow::paragraph` | 增加 `blank` / `blank_em` / `blank_to_end` / `blank_with` 等 builder；`Paragraph` 增加 `has_fill_blank` 拦截与错误暂存；`Flow::paragraph` 检查组装错误并接入 `Flow::push`。 |
| `src/layout/paragraph.rs` | `TextNode`、`TextRun`、`measure_inline`、`LineBox`、`fragment_from_lines` | `TextNode` 改为包含 `Vec<InlineItem>`；`measure_inline` 跳过 Blank（不进 skrifa）；在 `layout_runs` 中为每个 broken line 顺序推进 x 游标，记录 `BlankStroke` 并平移后续 glyph；`fragment_from_lines` 产出 `FragmentCommand::Stroke`；行高保持仅由字体度量决定。 |
| `src/text/line_break.rs` | `Atom`、`break_lines`、`pack_lines`、`nearest_break` | `Atom` 增加 `BlankFixed` 与 `BlankFill`；`Hard` 携带真实 `cluster` 索引（彻底解耦 atom 与 cluster 1:1 隐式假设）；`pack_lines` 支持 Blank 前后合法断行、动态解析 `BlankFill` 宽度并计入 `LineRecord.width`；生成 `LineSegment` 列表。 |
| `src/lib.rs` | 公共重导出 | 重导出 `BlankBuilder`（若对外公开）。 |
| `src/layout/style.rs` | 段落样式 | 不变。Blank 继承 `style.color` 与 `style.font_size`。 |
| `src/layout/hline.rs` | 块级水平分隔线 | 不变（严格隔离，仅做绘制类型参考）。 |
| `src/geom.rs` | 长度、颜色、画笔 | 不变。直接复用 `Length<NonNegative>`, `Length<Positive>`, `Color`, `Path::line`, `Stroke::new`。 |
| `src/canvas.rs` & `src/backend/` | 画布与 krilla 转换器 | 不变。现有 `FragmentCommand::Stroke` 与 backend `Mark::Stroke` 完全可直接消费。 |

---

## 5. 排版排布算法（Layout Algorithm）

全管线流程图：

```text
Paragraph DSL (text / bold / blank / blank_to_end)
   │ 校验：has_fill_blank 后禁止再有 run；相邻同 Face 文本合并
   ▼
Flow::paragraph 捕获 Style (font, font_size, first_line_indent, color, align)
   │ 传递组装错误；生成 FlowNode::Text(TextNode)
   ▼
TextNode::split 首次获取当前容器宽度 W
   │
   ▼
ParagraphNode::layout_runs(fonts, items, style, W)
   │
   ├─► 1. 验证几何：W > 0 且 W' = W - indent > 0，否则 Err(InvalidGeometry)
   ├─► 2. 测量文本项：仅对 Text run 调 cluster_tagged + skrifa cmap（Blank 不进测量）
   ├─► 3. 投影断行原子（project_atoms）：
   │      - Text cluster → Atom::Ink / Atom::Hard
   │      - BlankFixed / Em → Atom::BlankFixed { width: N * font_size 或 pt }
   │      - BlankFill → Atom::BlankFill
   ├─► 4. 贪心断行与回退（pack_lines）：
   │      - 行首 Sp 丢弃，但 Blank 决不丢弃
   │      - BlankFixed 允许在自身之前或之后断行；超限尝试回退断点
   │      - 溢出且无断点时报错 Err(UnbreakableWiderThanFrame { width, avail })（首行为 W'）
   │      - BlankFill 动态计算 w_fill = max(0, avail - line_width)，计入 line_width
   │      - 输出每行的 LineSegment 切片序列
   ├─► 5. 构建 LineBox（烘焙水平几何）：
   │      - line_x = frame_x + align_x(frame_width, line.width(), style.align)
   │      - current_x = line_x
   │      - 遇到 Glyphs：放置 glyphs，current_x += glyphs_advance
   │      - 遇到 Blank：若 width > 0 记录 BlankStroke(current_x, current_x + width)，current_x += width
   │      - 随后的 Glyphs 起点直接从更新后的 current_x 开始（向右平移！）
   │
   ▼
ParagraphNode::split(avail)
   │ 垂直分页切片（以整行 LineBox 为单位），不重新换行、不重新计算 W_remain
   ▼
fragment_from_lines
   │ y = line_index * line_height
   ├─► 产出 FragmentCommand::Glyphs(run.with_y(y + baseline))
   └─► 产出 FragmentCommand::Stroke(path.with_y(y + baseline + offset))
   （Fragment.height = line_count * line_height，线粗不计入高度！）
```

### 5.1 组装阶段（Assemble）

1. **追加文本**：
   - 若 `text.is_empty()`，直接返回（空字符串不视作有效 run，不打断合并，也不触发 fill 终止报错）。
   - 若 `self.has_fill_blank` 为 true，说明在吃到行尾横线之后又追加了文本，记录 `self.error = Some(Error::InvalidGeometry)`。
   - 若前一个 item 是 `InlineItem::Text` 且 `face` 相同，就地合并 `push_str`；否则追加 `InlineItem::Text`。
2. **追加 Blank**：
   - 若 `self.has_fill_blank` 为 true，记录 `self.error = Some(Error::InvalidGeometry)`。
   - 若宽度类型为 `BlankWidth::ToEndOfLine`，标记 `self.has_fill_blank = true`。
   - 无论何种 Blank，均强制**打断**左右文本的合并。
3. **错误传递**：
   - `Flow::paragraph` 执行闭包后，先检查 `paragraph.error`。若为 `Some(err)`，直接调用 `self.push(Err(err))`。
   - 该错误进入 `Flow.error`，在 `paint` 时先于任何画布操作返回，符合规范。

### 5.2 测量阶段（Measure）

1. **字形测量隔离**：
   - `measure_inline` 遍历 `items`，仅提取 `InlineItem::Text` 的文本切片送入 `cluster_tagged`。
   - Blank 绝对不生成 Unicode 标量、不分配伪字形、不参与 cmap 查询，避免触发 `MissingGlyph`。
   - 仅当存在非空 `Face::Bold` 的 Text run 时才查询 `fonts.bold()`。纯 Blank 段落或只有 Body 文本的段落不会触发粗体字体缺失错误。
2. **行高与基线度量**：
   - 获取 Body 字体（`style.font`）的 `line_metrics`。
   - 基线 `baseline = metrics.ascent`。
   - `line_height = max(metrics.ascent - metrics.descent + metrics.leading, font_size * line_height_factor)`。
   - 即使是全 Blank 段落，其行高与基线仍然由该公式决定，确保排版垂直节律完全一致。

### 5.3 断行阶段（Line Break & Atom Packing）

#### 5.3.1 原子投影（`project_atoms`）
将文本字素簇与 Blank 项按顺序投射为 `Atom` 序列：
- 文本簇：根据 Unicode 分类投射为 `Atom::Hard { cluster }` 或 `Atom::Ink { pair, width, cluster }`。
- `BlankWidth::Fixed(w)`：投射为 `Atom::BlankFixed { width: w.as_pt(), item_index }`。
- `BlankWidth::Em(n)`：在此时乘以 `style.font_size.as_pt()`，投射为 `Atom::BlankFixed { width: n as f32 * font_size, item_index }`。
- `BlankWidth::ToEndOfLine`：投射为 `Atom::BlankFill { item_index }`（宽度待在 packing 时根据本行剩余宽决定）。

#### 5.3.2 贪心装包（`pack_lines`）
维护当前行累积宽度 `line_width`，行预算 `avail = budget.for_line(records.len())`：
1. **行首处理**：
   - 行首普通的 `PairClass::Sp` 丢弃（不占宽度）。
   - Blank 绝不被视作 `Sp`，行首 Blank 严禁丢弃！
2. **`Atom::BlankFixed { width, .. }` 处理**：
   - 若 `line_width + width <= avail`：当前行可容纳，`line_width += width`，继续装入下一原子。
   - 若 `line_width + width > avail`：发生溢出！
     - 调用 `nearest_break` 寻找断点。
     - **断行规则（对齐自主默认值）**：Blank 作为整体不可拆，但**允许在 Blank 紧邻的前方断行**（将整个 Blank 移到下一行）；也**允许在 Blank 紧邻的后方断行**。
     - 若 `line_width > 0`，则 `split = i`（即当前 Blank 前方）就是一个合法断点！
     - 此时将 `line_start..split` 提交为一行，新行从当前 Blank 开始，`line_width = 0`，下一行预算为 `W`。
     - 若新行行首仍然装不下该 Blank（即当前视觉行预算为 `avail`，而 `width > avail`，且前方无断点）：
       直接返回错误：
       ```rust
       Err(Error::UnbreakableWiderThanFrame {
           width,
           avail,
       })
       ```
       注意：如果该 Blank 出现在段落首行行首且 `width > W'`，报告的 `avail` 正是首行预算 `W'`；若在后续行，报告的 `avail` 为 `W`。
3. **`Atom::BlankFill { .. }` 处理**：
   - 计算本行剩余宽：`let w_remain = (avail - line_width).max(0.0);`。
   - 动态解析宽度：`let fill_w = w_remain;`。
   - 累加到行宽：`line_width += fill_w;`（此时整行宽度恰好等于 `avail`）。
   - 将 `LineSegment::Blank { item_index, width: fill_w }` 记录到当前行。
   - 由于组装期保证了 Fill 是段落最后一个原子，该行即刻收尾。

### 5.4 盒组装与水平定位（LineBox Assembly）

对每一行计算水平偏移：
```rust
let frame_width = if index == 0 { first_width } else { width };
let frame_x = if index == 0 { indent } else { 0.0 };
let line_x = frame_x + align_x(frame_width, line.width(), style.align);
```
遍历当前行的 `LineSegment`，按流水线推进 `current_x`：
1. **初始状态**：`let mut current_x = line_x;`
2. **遇到 `LineSegment::Glyphs(range)`**：
   - 从 `current_x` 开始为切片构建 `GlyphRun`。
   - 累加字形 advance：`current_x += glyphs_advance;`。
3. **遇到 `LineSegment::Blank { item_index, width }`**：
   - 若 `width > 0.0`：
     创建横线描述：
     ```rust
     strokes.push(BlankStroke {
         x_start: current_x,
         x_end: current_x + width,
         y: metrics.baseline + blank.baseline_offset.as_pt(),
         thickness: blank.thickness,
         color: blank.color.unwrap_or(style.color),
     });
     ```
   - 若 `width == 0.0`（例如 `W_remain == 0` 或 `Fixed(0)`）：按自主默认值，**不生成 Stroke**，但正常推进 0 pt。
   - 推进光标：`current_x += width;`。
4. **遇到随后的 `LineSegment::Glyphs`**：
   - 其第一个字形的 x 坐标即为当前的 `current_x`！
   - **由此实现：中间 blank 丝滑地将后续文字向右平移，两端文字与中间下划线在同一基线系统下绝对对齐！**
5. **对齐一致性**：
   - 因为 Fill blank 的 `fill_w` 已经完整计入 `line.width()`，整行宽度等于 `frame_width`。
   - 无论是 `Start`、`Center` 还是 `End` 对齐，`frame_width - line.width()` 均为 0，整行不会产生不对称的偏移错位。

### 5.5 分页与命令生成（Fragment & Pagination）

1. **整行分页跨页（Rides its line across pages）**：
   - `TextNode::split` 完成上述水平烘焙后转为 `ParagraphNode`。
   - 后续 `ParagraphNode::split` 仅以整行 `LineBox` 为粒度按垂直可用高度切片。
   - 跨页时不会重新触发断行，也不会重新计算 `W_remain`。Blank 横线与其所在的文本行紧密绑定，天然随行换页。
2. **命令发射（`fragment_from_lines`）**：
   - 计算行垂直偏移：`line_y = line_index * line_height`。
   - 遍历 `line.runs`：发射 `FragmentCommand::Glyphs`，y 坐标加上 `line_y`。
   - 遍历 `line.strokes`：
     ```rust
     let y = line_y + stroke.y;
     let path = Path::line(
         Point::try_pt(stroke.x_start, y)?,
         Point::try_pt(stroke.x_end, y)?,
     )?;
     commands.push(FragmentCommand::Stroke {
         path,
         paint: Paint::solid(stroke.color),
         stroke: Stroke::new(stroke.thickness),
     });
     ```
3. **行高绝对不膨胀（No line-height growth）**：
   - Fragment 总高度为 `lines.len() * line_height`。
   - 绝不把 `thickness`、`baseline_offset` 加入行盒高度，不影响上下行间距与分页游标推进。

---

## 6. 错误处理与非法状态消除（Error Handling）

| 错误场景 | 触发时机 | 拦截方式 / 错误变体 | 行为表现 |
| :--- | :--- | :--- | :--- |
| 定宽为负数或 NaN / 极值 | 组装期 | `Length<NonNegative>::pt(val)` 构造期拒绝 | 返回 `Error::NegativeLength` 或 `Error::NonFiniteLength`。 |
| 线粗为 0、负数或 NaN | 组装期 | `Length<Positive>::pt(val)` 构造期拒绝 | 返回 `Error::NonPositiveLength` 或 `Error::NonFiniteLength`。 |
| 铺到行尾后追加非空文本或 Blank | 组装期 | `Paragraph::append` / `append_blank` 拦截 | 记录 `Paragraph.error = Some(Error::InvalidGeometry)`，由 `Flow::paragraph` 捕获并在 `paint` 前失败，空画布。 |
| 定宽大于首行可用宽 `W'`（首行行首） | 排版断行期 | `pack_lines` 判定溢出且无可回退断点 | 返回 `Error::UnbreakableWiderThanFrame { width, avail: W' }`。 |
| 定宽大于后续行可用宽 `W` | 排版断行期 | `pack_lines` 判定溢出且无可回退断点 | 返回 `Error::UnbreakableWiderThanFrame { width, avail: W }`。 |
| 铺到行尾遇 `W_remain == 0` | 排版断行期 | 正常状态，不报错 | 消费 0 pt 宽度，省略 stroke 命令，不报错。 |
| 纯 Blank 段落 | 排版期 | 正常状态，不报错 | 使用 Body 字体度量计算正常行高和基线，绘制下划线，行高不坍缩。 |

---

## 7. 测试验证方案（Tests Plan）

所有排版断言均基于 `RecordingCanvas` 记录的 `CanvasCommand` 坐标与属性进行精确比对，避免脆弱的二进制断言；仅在 `tests/backend_krilla.rs` 中保留 `%PDF` 冒烟测试。

### 7.1 单元测试（`src/layout/paragraph.rs` & `src/text/line_break.rs`）

1. **`test_blank_fixed_shifts_following_glyphs`**：
   - 段落包含 `"甲"` + `blank(60pt)` + `"乙"`。
   - 断言第一个字形 x 为 `0`，`StrokePath` 的起点 x 为 `"甲"` 的宽度，终点 x 为起点 `+ 60pt`。
   - 断言 `"乙"` 的字形 x 精确等于起点 `+ 60pt`。
2. **`test_blank_baseline_and_offset`**：
   - 验证 `StrokePath` 的 y 坐标精确等于 `metrics.baseline + offset`（默认 `baseline + 1.0`）。
3. **`test_blank_line_height_unchanged`**：
   - 比较相同文字无 Blank 段落与包含 `thickness(5pt)` 粗下划线段落的 `natural_height`，断言行高完全相同。
4. **`test_blank_fill_consumes_remainder`**：
   - 框宽 200 pt，文本宽 50 pt，追加 `blank_to_end()`。
   - 断言横线起点为 50 pt，终点为 200 pt，宽度为 150 pt。
5. **`test_blank_fill_zero_remainder_omits_stroke`**：
   - 框宽 100 pt，文本排满 100 pt，后接 `blank_to_end()`。
   - 断言画布中不出现 `StrokePath` 命令，且不报错。
6. **`test_blank_em_sugar_scales_with_font_size`**：
   - 字号 12 pt 下 `blank_em(4)` 生成宽度为 48 pt 的横线；字号 16 pt 下生成 64 pt。
7. **`test_blank_fixed_zero_width_omits_stroke`**：
   - `blank(Length::ZERO)` 产生 0 宽，不产生 `StrokePath` 命令。

### 7.2 断行与边界测试（`tests/document.rs`）

1. **`test_blank_wraps_to_next_line_as_unit`**：
   - 前置文字使当前行剩余 40 pt，后接 60 pt 定宽 Blank。
   - 断言 Blank 整体换到下一行行首，起点从下一行行首 x 开始。
2. **`test_blank_fixed_overflow_first_line_reports_w_prime`**：
   - 页面框宽 100 pt，首行缩进 30 pt（`W' = 70 pt`），行首放置 80 pt Blank。
   - 期望捕获 `Error::UnbreakableWiderThanFrame { width: 80.0, avail: 70.0 }`。
3. **`test_blank_fill_forbids_subsequent_runs`**：
   - 测试 `p.blank_to_end(); p.text("后置文本");`。
   - 期望失败并返回 `Error::InvalidGeometry`，且在进入 `paint` 前截断，画布记录为空。
   - 测试 `p.blank_to_end(); p.blank(Length::pt(20.0));` 同样失败。
   - 测试 `p.blank_to_end(); p.text("");`（空文本）允许通过，不报错。
4. **`test_blank_rides_across_pages`**：
   - 多行段落中某一行含有 Blank，分页发生在 Blank 所在行之前或之后。
   - 验证跨页后 Blank 横线与其所在行的文字出现在同一页，坐标平移正确。
5. **`test_blank_only_paragraph`**：
   - 段落内仅有一个 `blank(100pt)`。
   - 验证产生正常单行高度，横线正确画出。

### 7.3 端到端与适配器冒烟（`tests/backend_krilla.rs`）

1. **`test_krilla_pdf_smoke_with_blanks`**：
   - 运行包含定宽、汉字字数、吃到行尾 Blank 的文档生成。
   - 断言输出以 `%PDF-` 开头，krilla backend 适配器无 panic。
