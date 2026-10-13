# Knife 15 — §8 文字外链 URI（candidate-4）

**基线：** `feat/inline-blank` @ `a232ce1`  
**权威：** `docs/提案书.md` §8、§2.0（类型化空 URI 构造期拒绝）  
**范围：** 正文 `Paragraph` + 页眉页脚 `Band` 整槽外链 URI；复用已有 `LinkTarget` / `Canvas::annotate_link` / `backend-krilla` 队列。不做文内跳转（`linkToPage` / 锚点）、图链、`Layer::text_at`、表格单元格富文本链、自动下划线/颜色修饰。

**Autonomy：** 全部采纳 frame.md 默认值（**无 reverse**）。

---

## 1. Usage sketch（调用方 DSL）

### 1.1 正文段落中的文字链接

正文通过 `Paragraph::link` 闭包提供作用域，与 `p.text` / `p.bold` / `p.blank` 风格保持一致：

```rust
use quire_pdf::{document, PageSize, Margins, NonNegative, Result};

fn build_notice() -> Result<Rendered> {
    document(PageSize::A4, |doc| {
        doc.font(FONT_BYTES.to_vec())?;
        doc.margins(Margins::uniform(NonNegative::pt(36.0)?))?;

        doc.paragraph(|p| {
            p.text("欢迎访问官方主页：");
            p.link("https://example.com/docs", |p| {
                p.text("查看");
                p.bold("最新文档");
                p.text("与规范指南");
            });
            p.text("。如有疑问请联系管理员。");
        });

        Ok(())
    })
}
```

#### DSL 语义与约定

1. **统一输入入口（`IntoLinkTarget`）**：
   - `p.link(target, |p| { ... })` 第一个参数支持 `&str`、`String`、`&String`、`LinkTarget` 以及 `&LinkTarget`。
   - 传入字符串时在进入闭包前调用 `LinkTarget::uri(s)`；空字符串立即产生 `Error::EmptyLink`。
   - 若调用方已持有预先构造好的 `LinkTarget`，可直接传入，零额外开销且类型期保证非空。
2. **闭包作用域与状态注入**：
   - 闭包执行期间，内部调用的 `p.text(...)` 和 `p.bold(...)` 会自动打上当前作用域的 `LinkTarget`。
   - 嵌套调用安全：若在 `link` 闭包内再次调用 `p.link`，内层 URI 覆盖外层；内层闭包退出时恢复外层 URI（单级 `Option<LinkTarget>` 暂存或微型栈）。
3. **错误处理符合 DSL 惯例（无 `?` 编译报错风险）**：
   - `Flow::paragraph` 的签名为 `pub fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph))`，闭包返回值是 `()`。
   - 若 URI 为空串，`Paragraph` 将错误记录至内部 `self.error`（与 `blank_to_end` 后写入触发 `Error::RunAfterFillBlank` 的设计完全一致）。
   - 在 `Flow::paragraph` 结束时检查 `paragraph.error`，若有错则推入 `Flow.error`，并在 `build(&mut doc)?` 或 `doc.paint()` 时直接返回 `Err(Error::EmptyLink)`，**在任何页面开启（`start_page`）之前拒绝，不留下半截脏页面**。
4. **与其他 inline 元素的交互**：
   - 空文本 `p.text("")` 沿用现有逻辑，在 `append` 处直接返回，不生成无 ink 的空 run，亦不生成命中矩形（`skip-empty`）。
   - 填空下划线 `p.blank(...)` 在链接中不可点（`blank-no-link`）：blank 会主动截断当前测量组，使得链接文字与 blank 分裂为独立段落片段，blank 仅发出 `LinePiece::Stroke`，不产生链接注解。
   - 无任何内置视觉装饰（`no-decoration`）：库不自动为链接添加蓝色或下划线。文字颜色与下划线由调用方按需使用 `p.bold` 或将来的颜色 API 自由定义。

---

### 1.2 页眉 / 页脚整槽链接

页眉页脚通过 `Band::left_link` 与 `Band::right_link` 提供整槽外链（`band-slot-link`）：

```rust
doc.header(|h| {
    // 左槽：整槽带外链的企业名称
    h.left_link("https://acme.example.com", "Acme Corporation");

    // 右槽：无链接的纯文本页码
    h.right(format!("第 {} 页 / 共 {} 页", h.page().number, h.page().count));
});
```

#### Band 语义约定

- `Band::left_link(target, text)` / `Band::right_link(target, text)` 支持 `IntoLinkTarget`。
- 若随后调用 `h.left(text)`，将覆盖并清除左槽的链接属性；反之调用 `left_link` 覆盖已有文本并设置链接。
- 空 URI 时同样记录 `self.error = Some(Error::EmptyLink)`，在 `Band::lower` 时返回 `Err`，阻止整份文档渲染。
- `Band::lower` 为该槽生成单一 `TextRun { face: Face::Body, link: Some(target), text }`，随后复用整条排版、测量、断行、栏平移（`Columns`）及绘制管线。

---

## 2. Types / data shapes

### 2.1 公共 API 类型（`src/document.rs` / `src/lib.rs`）

```rust
/// 支持无痛传入 &str 或 LinkTarget 的转换 trait。
pub trait IntoLinkTarget {
    fn into_link_target(self) -> Result<LinkTarget>;
}

impl IntoLinkTarget for &str {
    fn into_link_target(self) -> Result<LinkTarget> {
        LinkTarget::uri(self)
    }
}

impl IntoLinkTarget for &String {
    fn into_link_target(self) -> Result<LinkTarget> {
        LinkTarget::uri(self)
    }
}

impl IntoLinkTarget for String {
    fn into_link_target(self) -> Result<LinkTarget> {
        LinkTarget::uri(&self)
    }
}

impl IntoLinkTarget for LinkTarget {
    fn into_link_target(self) -> Result<LinkTarget> {
        Ok(self)
    }
}

impl IntoLinkTarget for &LinkTarget {
    fn into_link_target(self) -> Result<LinkTarget> {
        Ok(self.clone())
    }
}
```

### 2.2 组装层状态（`src/document.rs`）

```rust
pub struct Paragraph {
    items: Vec<InlineItem>,
    first_line_indent: Length<NonNegative>,
    error: Option<Error>,
    active_link: Option<LinkTarget>, // NEW: 当前嵌套层级的活动链接
}

pub struct Band {
    page: Page,
    left: Option<String>,
    right: Option<String>,
    left_link: Option<LinkTarget>,   // NEW: 左槽链接
    right_link: Option<LinkTarget>,  // NEW: 右槽链接
    error: Option<Error>,            // NEW: 捕获空 URI 错误
}
```

### 2.3 排版与断行内部类型（`src/layout/paragraph.rs`）

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextRun {
    pub(crate) text: String,
    pub(crate) face: Face,
    pub(crate) link: Option<LinkTarget>, // NEW: 承载该段文字的 URI 目标
}

#[derive(Debug, Clone, PartialEq)]
struct MeasuredInline {
    text: TaggedClusteredText<Face>,
    glyphs: Vec<InlineGlyph>,
    link: Option<LinkTarget>, // NEW: 该连续测量组的链接属性（组内完全同构）
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LinePiece {
    Glyphs(GlyphRun),
    Stroke(InlineStroke),
    Link {
        x0: f32,
        x1: f32,
        target: LinkTarget,
    }, // NEW: 行内视觉命中段，x 已含对齐与缩进，y 留待 fragment_from_lines 叠加
}
```

### 2.4 片段命令（`src/layout/flow.rs`）

```rust
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FragmentCommand {
    Glyphs(GlyphRun),
    Fill {
        path: Path,
        paint: Paint,
    },
    Stroke {
        path: Path,
        paint: Paint,
        stroke: Stroke,
    },
    Image {
        image: DecodedImage,
        rect: Rect,
    },
    Link {
        rect: Rect,
        target: LinkTarget,
    }, // NEW: 片段局部坐标系下的超链接注释矩形
}
```

### 2.5 核心不变式（Invariants）

1. **不可表现性**：非法空 URI 在进入排版前通过 `LinkTarget::uri` 拦截并化为 `Error::EmptyLink`，排版管线内部各类型持有的 `LinkTarget` 100% 保证包含非空字符串。
2. **组内同构性**：一个 `MeasuredInline` 内部的所有 glyph 具有完全一致的 `Option<LinkTarget>`。断行器输出的 `Seg::Glyphs { group, .. }` 隐式继承该组的 URI，断行原子 `Atom` 与 `Seg::Glyphs` 无需携带复杂堆分配 payload。
3. **几何严谨性**：`Rect` 的宽高必须严格大于 0（`geom.rs` 约束）。`LinePiece::Link` 到 `FragmentCommand::Link` 的转换中，若 `x1 - x0 <= 0` 或存在非有限浮点数，直接省略（`skip-empty`），绝不生成无效 `Rect` 触发崩溃。
4. **行盒高度对齐**：命中矩形的高采用正文行盒高 `metrics.line_height`，顶边取行盒顶边 `y0`（非基线），确保用户即使将鼠标悬停在文字行间与升部、降部也能正常点击。

---

## 3. Module map（变更文件清单）

| 模块文件 | 职责变更 | 依赖合规 |
|---|---|---|
| `src/document.rs` | 1. 声明 `IntoLinkTarget` trait 及常见类型实现。<br/>2. `Paragraph` 增加 `active_link` 字段，实现 `Paragraph::link` 方法。<br/>3. `Paragraph::append` 合并键增加 `last.link == link`。<br/>4. `Band` 增加 `left_link`、`right_link`、`error` 字段，实现 `Band::left_link` / `right_link`。<br/>5. `Band::lower` 校验错误并向 `TextNode::from_items` 填入带链接的 `TextRun`。 | 保持不变，不引入外部依赖。 |
| `src/layout/paragraph.rs` | 1. `TextRun` 增加 `pub(crate) link: Option<LinkTarget>`，构造器设默认值。<br/>2. `MeasuredInline` 增加 `link: Option<LinkTarget>` 字段。<br/>3. `prepare_inline` 在 `last.link != run.link` 时切分并 flush 文本组。<br/>4. `LinePiece` 增加 `Link { x0, x1, target }` 变体。<br/>5. `place_line` 计算 seg advance 时若属于带链接组且 advance > 0，发出 `LinePiece::Link`（Bold/Body 变化不切分该 piece）。<br/>6. `fragment_from_lines` 将 `LinePiece::Link` 升格为 `FragmentCommand::Link`（`y0 = index * line_height`）。 | 不接触 `krilla`，只依赖 `canvas::LinkTarget`。 |
| `src/layout/flow.rs` | 1. `FragmentCommand` 增加 `Link { rect: Rect, target: LinkTarget }` 变体。<br/>2. `Fragment::with_offset` 增加 `Link` 臂，调用 `rect.translated(dx, dy)?`。<br/>3. `draw_fragment` 增加 `Link` 臂，调用 `canvas.annotate_link(rect.translated(ox, oy)?, target.clone())?`。 | 单向调用 `Canvas` trait。 |
| `src/layout/list.rs` | `first_baseline` 穷尽 match 增加 `FragmentCommand::Link { .. } => None`。 | 无。 |
| `src/canvas.rs` | **零改动**（`LinkTarget`、`CanvasCommand::AnnotateLink`、`Canvas::annotate_link`、`RecordingCanvas` 均已就绪）。 | — |
| `src/backend/*` | **零改动**（`KrillaCanvas::annotate_link`、`QueuedLink`、`emit_page` 恒等映射已就绪）。 | — |
| `src/error.rs` | **零改动**（`Error::EmptyLink` 已存在）。 | — |
| `tests/` | 新增集成测试（断言 `RecordingCanvas` 记录的命中矩形与 URI）。 | 仅测试代码。 |

---

## 4. Layout algorithm（排版算法流程）

```
[DSL 阶段]
  Paragraph::link(uri, |p| { p.text("a"); p.bold("b"); })
     │  
     ├─ 校验 LinkTarget::uri(uri) ──(空串)──> 记录 self.error = Some(EmptyLink)
     └─ 成功: 设置 active_link = Some(target)
           │
           ▼
[组装合并: Paragraph::append]
  比较: last.face == face && last.link == active_link
     ├─ 相同: last.text.push_str(text) （coalesce-same-uri）
     └─ 不同: items.push(TextRun { text, face, link })
           │
           ▼
[测量分组: prepare_inline]
  遍历 InlineItem:
     ├─ 遇到 Blank: flush_text()，插入 Span::Blank
     ├─ 遇到 TextRun:
     │     若 pending 非空且 pending.last().link != run.link:
     │         flush_text() ──> 独立生成 MeasuredInline { link, .. }
     │     pending.push(run)
     └─ 循环结束: flush_text()
  注：同链接下的 Body 与 Bold 混排时不触发 flush，归入同一 MeasuredInline 组！
           │
           ▼
[断行执行: break_spans -> pack_lines -> emit_line]
  不改动断行器内部算法。
  换行处自然切分组，输出每个视觉行对应的 Seg::Glyphs { group, start, end }。
           │
           ▼
[几何放置: place_line] (确定 x0, x1)
  line_x = frame_x + align_x(...)，x = line_x
  遍历 seg:
     ├─ Seg::Blank: x += width
     └─ Seg::Glyphs:
           advance = cluster_width_pt(...)
           若 groups[group].link 为 Some 且 advance > 0:
               推入 LinePiece::Link { x0: x, x1: x + advance, target: target.clone() }
           即使该 seg 内部包含多字体切分的 GlyphRun，整段 seg 依然只发出一条 Link piece！
           x += advance
           │
           ▼
[片段构建: fragment_from_lines] (确定 y0, line_height, Rect)
  遍历 line:
     y0 = index * line_height
     遍历 piece:
        LinePiece::Link { x0, x1, target } =>
            w = x1 - x0
            若 w > 0:
                rect = Rect::try_pt(*x0, y0, w, line_height)?
                推入 FragmentCommand::Link { rect, target: target.clone() }
           │
           ▼
[变换平移: Fragment::with_offset]
  父级容器（Columns / Stack / List / Table）在组装复合片段时：
  FragmentCommand::Link { rect, .. } => *rect = rect.translated(dx, dy)?
           │
           ▼
[画布绘制: draw_fragment]
  ox = origin.x(), oy = origin.y()
  FragmentCommand::Link { rect, target } =>
      canvas.annotate_link(rect.translated(ox, oy)?, target.clone())?
           │
           ▼
[Backend 呈现: KrillaCanvas -> emit_page]
  排入 open_page.links 队列；
  在 surface 内容流绘制完成后，恒等映射坐标并写入 PDF LinkAnnotation。
```

### 算法细节剖析

1. **为什么在 `place_line` 而不是在 `pack_lines` 算 x？**  
   `pack_lines` 只处理未对齐的断行累加，尚不知道首行缩进（`frame_x`）、段落文本对齐偏移（`align_x`）以及 fill blank 的拉伸。只有在 `place_line` 里，当前行所有前置元素已经排布到位，`x` 的物理绝对位置才唯一确定。
2. **为什么在 `fragment_from_lines` 算 y？**  
   视觉行的 `LineBox` 在跨页重试与分页时会被重用；`ParagraphNode::split` 切分尾部时，tail 片段从 `index = 0` 重新对齐。如果在 `place_line` 里写死 y，跨页会导致坐标被叠加两次。在 `fragment_from_lines` 里按片段局部索引计算 `y0 = index * line_height` 是与 `InlineStroke` 完全一致的标准模式。
3. **如何保证 Bold/Body 同链接在一行上只有一个矩形？**  
   在 `prepare_inline` 中，由于同链接下 `last.link == run.link`，不同 face 的 run 被收集进同一个 `pending` 组，成为同一个 `MeasuredInline`。在 `place_line` 中，虽然 `glyph_runs_for_line` 会在 bold/body 边界将文字切分成两个 `LinePiece::Glyphs`，但我们在 seg 级计算整段的 `advance`，因此只生成**一个**覆盖整个 seg 的 `LinePiece::Link`！完美满足 rubric #3。

---

## 5. Error cases（错误分支与边界情况矩阵）

| 异常场景 | 表现与处理 | 抛出阶段 | 保证与依据 |
|---|---|---|---|
| `p.link("", ...)` 空 URI | 捕获并记录 `Error::EmptyLink`；闭包提前退出，段落放弃推入节点 | DSL 组装阶段（`build` 闭包内） | 提案书 §8 / §2.0：构造期拒绝，绝不推入 layout。在 `start_page` 之前失败。 |
| `Band::left_link("", ...)` | 记录 `Band::error`，在 `Band::lower` 时返回 `Err(Error::EmptyLink)` | 带测量前阶段 | 同上，不会打开或渲染任何页面。 |
| 仅包含空格的 URI `" "` | 校验通过，正常生成 `LinkTarget` 并写入 PDF | DSL 组装与绘制 | 提案书 §8：只写入字符串，不校验 scheme，不进行网络访问，不裁切内部字符。 |
| 非 http URI（如 `mailto:a@b.c`） | 校验通过，原样写入 PDF 注解 | DSL 组装与绘制 | 提案书 §8：协议无关。 |
| 链接内文本为空 `p.link("url", \|p\| p.text(""))` | 忽略空文本；不发出 `LinePiece::Link`，不调用 `annotate_link` | 组装与放置 | `skip-empty` 默认，不构造非法零宽 `Rect`。 |
| 首行缩进导致可用宽度 `<= 0` | 抛出 `Error::InvalidGeometry` | 段落排版初期（`layout_runs`） | 现有基线行为，发生在任何片段生成之前。 |
| `blank_to_end` 之后调用 `p.link(...)` 写入文字 | 记录 `Error::RunAfterFillBlank`，丢弃整段 | 组装阶段 | 密封段落禁止追加。 |
| 链接内部含有 `p.blank(...)` | blank 强制触发 `flush_text`；链接被切分为两段，blank 区域无链接注解 | 测量与放置 | `blank-no-link` 默认；blank 仅生成 `Stroke`。 |
| 页眉带高超出 margin | 抛出 `Error::DecorationTallerThanMargin` | `measure_bands` 阶段 | 现有基线行为，`start_page` 之前干净退出。 |

---

## 6. Test plan（RecordingCanvas 行为断言规划）

测试均基于自有 `RecordingCanvas` 断言，结合 `FontStore` 与标准 A4 / 自定义测试页面，**断言 `CanvasCommand::AnnotateLink { rect, target }`** 的精确数值（容差 `PT_ABS_TOLERANCE = 1e-4`）：

### T1: 单行文本链接命中矩形精确性
- **操作**：创建单行段落 `"Check "` + `link("https://docs.rs", "quire")` + `" for info."`。
- **断言**：
  1. `commands` 中仅有 1 个 `AnnotateLink`。
  2. `target.as_str() == "https://docs.rs"`。
  3. `rect.origin.y()` 精确等于当前段落顶边 `y0`。
  4. `rect.size.height()` 精确等于正文 `line_height`。
  5. `rect.origin.x()` 精确等于前置文字 `"Check "` 的排版宽度。
  6. `rect.size.width()` 精确等于 `"quire"` 的排版宽度。

### T2: 窄栏自动换行产生多矩形（Wrap to 2 Rects）
- **操作**：在固定宽度（如 100 pt）的窄栏中放置较长链接文字，使之恰好断成两行。
- **断言**：
  1. 产生恰好 2 个 `AnnotateLink`，且 `target` 相同。
  2. 第一个 rect 的 `y` 为 0，高度为 `line_height`，宽度为第一行文字的实际 advance。
  3. 第二个 rect 的 `y` 为 `line_height`，高度为 `line_height`，宽度为第二行折行文字的 advance。

### T3: 链接内 Bold + Body 混排保持单矩形（No Split Across Fonts）
- **操作**：`p.link("url", |p| { p.text("normal "); p.bold("bold"); })`，整行不换行。
- **断言**：
  1. 该行产生 2 个 `CanvasCommand::DrawGlyphRun`（分别由 Body 字体与 Bold 字体绘制）。
  2. 该行产生**恰好 1 个** `AnnotateLink`，其宽度等于 normal 与 bold 文字 advance 之和。

### T4: 空 URI 构造期拦截（EmptyLink Rejection）
- **操作**：
  - 调用 `p.link("", |p| p.text("abc"))`。
  - 调用 `Band::left_link("", "corp")`。
- **断言**：
  1. `document()` 返回 `Err(Error::EmptyLink)`。
  2. `RecordingCanvas` 记录的 `pages().len() == 0`，未打开任何页面。

### T5: 链接内 Blank 打断链接（Blank Interruption）
- **操作**：`p.link("url", |p| { p.text("Left"); p.blank_em(2); p.text("Right"); })`。
- **断言**：
  1. 产生 2 个 `AnnotateLink`，分别对应 `"Left"` 与 `"Right"` 的范围。
  2. 产生 1 个 `CanvasCommand::StrokePath`（blank 下划线）。
  3. 两个 link rect 之间存在宽度为 `2 * font_size` 的空隙，该空隙无任何 link 覆盖。

### T6: 无额外视觉下划线或改色（No Decoration Side Effect）
- **操作**：对比有 link 的文本与无 link 的相同文本。
- **断言**：有 link 的段落除了新增 `AnnotateLink` 之外，其发出的 `DrawGlyphRun` 的 `font`、`font_size`、`color` 以及 `StrokePath` 数量与无链接版本完全一致，绝无隐式添加的下划线 stroke。

### T7: 页眉/页脚整槽链接（Header Slot Link）
- **操作**：设置页边距 36 pt，页眉配置 `band.left_link("https://acme.org", "Acme Corp")`。
- **断言**：
  1. 第一页包含 1 个来自页眉的 `AnnotateLink`。
  2. `rect.origin.y() == 0.0`。
  3. `rect.origin.x() == 36.0`（通过 `Columns` 自动内嵌正文边距）。

### T8: 复合容器嵌套坐标平移（Container Translation）
- **操作**：将含链接段落放入 `columns`（两栏并排）以及带缩进的 `list` 中。
- **断言**：第二栏内的 link rect 其 `origin.x()` 包含了栏间距和第一栏宽度的平移量，断言其与同行 glyph run 的 `origin.x()` 严格对齐。

### T9: 相邻同 URI 同 Face 文本合并（Coalescing）
- **操作**：`p.link("url", |p| { p.text("hello "); p.text("world"); })`。
- **断言**：在 `Paragraph.items` 中被合并为单一 `TextRun`，无额外换行时对应唯一样条的 link rect。

### T10: backend-krilla 烟雾与 URI 字符串验证
- **操作**：在开启 `backend-krilla` 条件下，渲染含链接的文档至 PDF 字节。
- **断言**：
  1. 返回字节以 `%PDF` 开头。
  2. PDF 字节串中包含明文 URI 字符串（或 `/Action << /S /URI ... >>` 结构）。
