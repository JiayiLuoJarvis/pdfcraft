# Knife 15 — §8 文字外链 URI（candidate-1）

**基线：** `feat/inline-blank` @ `a232ce1`  
**权威：** `docs/提案书.md` §8、§2.0（空 URI 构造期拒绝）  
**范围：** 正文 `Paragraph` + 页眉页脚 `Band` 整槽 URI；复用现有 `LinkTarget` / `Canvas::annotate_link` / backend 队列。不做文内跳转、图链、`Layer::text_at`、表格单元格富文本链、链接视觉装饰。

**Autonomy：** 全部采用 frame 默认（未 reverse）。

---

## 1. Usage sketch（调用方 DSL）

### 正文段落

闭包式 `link`，与 `p.text` / `p.bold` 并列；URI 在构造期经 `LinkTarget::uri` 校验，空串在 `link` 返回 `Err(Error::EmptyLink)`，不进入 layout。

```rust
use quire_pdf::{Document, LinkTarget, /* … */};

doc.flow(|f| {
    f.paragraph(|p| {
        p.text("See ");
        p.link("https://example.test/docs", |p| {
            p.text("the ");
            p.bold("manual");
            p.text(" for details.");
        })?;
        p.text(" End.");
        Ok(())
    })?;
    Ok(())
})?;
```

约定：

- `Paragraph::link` 签名：`pub fn link(&mut self, uri: &str, f: impl FnOnce(&mut Paragraph)) -> Result<()>`。
- 闭包内 `text` / `bold` 写入的 run 携带同一 `LinkTarget`（由 `active_link` 作用域注入，见下）。
- 嵌套 `link`：内层临时覆盖外层 `active_link`，闭包结束恢复（栈式一层深度即可；不鼓励深层嵌套，但行为确定）。
- 闭包内 `blank*` 仍不可链（`blank-no-link`）：blank 结束测量组，链接在 blank 处断开。
- 空字符串 `text("")` 仍被忽略；有效 URI + 全无 ink 时不产生 `AnnotateLink`（`skip-empty`），不额外报错。
- 链接不改变字形颜色/下划线（`no-decoration`）；视觉与无链段落相同。

`Flow::paragraph` 闭包若改为返回 `Result` 会与现有 `FnOnce(&mut Paragraph)` 不兼容。**本设计保持 `paragraph` 签名不变**：`link` 在段落闭包内返回 `Result`，调用方在 `paragraph` 外一层用 `?`（或 `document` 层后续统一 `Result` 化时一并改）。若当前 `paragraph` 无法传播 `link` 的 `Err`，则 **`link` 失败写入 `Paragraph::error`**（与 `RunAfterFillBlank` 相同模式），在 `Flow::paragraph` 末尾 `push(Err(error))`。实现时二选一，**优先 `Paragraph::error` 以保持 `paragraph(f)` 无 `Result` 签名**：

```rust
p.link("https://…", |p| {
    p.text("…");
})?; // 若 API 为 Result — 或 —
p.link("https://…", |p| { p.text("…"); }); // 失败进 paragraph.error，Flow 末尾报错
```

**推荐：** `link -> Result<()>`，并在 `paragraph` 文档中说明需在闭包外手动 `if let Err(e) = …` 直到 DSL 统一 Result；同时为 knife 测试提供 `paragraph` 内可测路径：`link` 返回 `Result`，集成测试用小型 helper 或直接在 `document` 模块单测里构造 `Paragraph` 并 `assert!(p.link("", …).is_err())`。

（实现 PR 择一并在 CHANGELOG 写清；设计假定 **`link` 返回 `Result<()>`**，空 URI 在 DSL 边界失败，不 lazy 到 paint。）

### 页眉 / 页脚

整槽一个 URI（`band-slot-link`），与 `left` / `right` 互斥替换：

```rust
doc.header(|band| {
    band.left_link("https://corp.example/", "Acme Corp")?;
    band.right("Page 1"); // 无链
    Ok(())
})?;
```

- `Band::left_link(uri, text) -> Result<()>` / `right_link` 对称。
- 调用 `left(s)` 清掉该槽位的 link 元数据；调用 `left_link` 设置 `(LinkTarget, String)`。
- `lower` 时用 `TextNode::from_items` 生成**单个** `TextRun { face: Body, link: Some(target), text }`，之后与正文同一 `split → fragment_from_lines → draw_fragment` 路径。

---

## 2. Types / data shapes

### 公共（已有 + 新增 surface）

| 类型 | 变更 |
| --- | --- |
| `LinkTarget` | 无改；`uri(&str) -> Result<Self>`，`EmptyLink`  on `""` |
| `Paragraph::link` | **新增** |
| `Band::left_link` / `right_link` | **新增** |
| `Error::EmptyLink` | 已有 |

### 内部（`layout/paragraph.rs`）

```rust
pub(crate) struct TextRun {
    pub(crate) text: String,
    pub(crate) face: Face,
    pub(crate) link: Option<LinkTarget>,  // NEW, default None
}

pub(crate) enum LinePiece {
    Glyphs(GlyphRun),
    Stroke(InlineStroke),
    Link { x0: f32, x1: f32, target: LinkTarget },  // NEW, fragment-local x; y in fragment_from_lines
}

struct MeasuredInline {
    text: TaggedClusteredText<Face>,
    glyphs: Vec<InlineGlyph>,
    link: Option<LinkTarget>,  // NEW, uniform for whole measure group
}
```

### `FragmentCommand`（`layout/flow.rs`）

```rust
pub(crate) enum FragmentCommand {
    // … existing …
    Link {
        rect: Rect,
        target: LinkTarget,
    },
}
```

### `Paragraph` 组装态（`document.rs`）

```rust
pub struct Paragraph {
    items: Vec<InlineItem>,
    first_line_indent: Length<NonNegative>,
    error: Option<Error>,
    active_link: Option<LinkTarget>,  // NEW, default None; set only during link() closure
}
```

### `Band`（`document.rs`）

```rust
pub struct Band {
    page: Page,
    left: Option<String>,
    right: Option<String>,
    left_link: Option<LinkTarget>,   // NEW; Some iff left_link() used for that slot
    right_link: Option<LinkTarget>,  // NEW
}
```

`lower` 规则：`left_link` 与 `left` 二选一存储；构建 run 时 `link: left_link.clone()`。

### 不变量

1. 同一 `MeasuredInline` 组内所有 source run 的 `link` 相等（`prepare_inline` flush 保证）。
2. `Seg::Glyphs` 仍无 payload；URI 只存在于 `MeasuredInline.link`。
3. 命中矩形：片段局部坐标，`Rect::try_pt(x0, y0, width, line_height)`，`width = x1 - x0`，`y0 = index * line_height`（行盒顶，非 glyph baseline）。
4. `width <= 0` 或 non-finite：**省略** `FragmentCommand::Link`，不报错。
5. 库只写入 URI 字符串；不 fetch、不校验 scheme。

---

## 3. Module map

| 文件 | 改动 |
| --- | --- |
| `src/document.rs` | `Paragraph::active_link`、`link()`、`append` 合并键（face + link）；`TextRun` 构造带 link；`Band` 字段 + `left_link`/`right_link` + `lower` 建 linked `TextNode`；模块单测 |
| `src/layout/paragraph.rs` | `TextRun.link`；`prepare_inline` link 边界 flush；`MeasuredInline.link`；`LinePiece::Link`；`place_line` 记录 x0/x1；`fragment_from_lines` → `FragmentCommand::Link`；`TextRun::new` 默认 `link: None`；layout 单测 |
| `src/layout/flow.rs` | `FragmentCommand::Link`；`Fragment::with_offset`；`draw_fragment` → `annotate_link` |
| `src/layout/list.rs` | `first_baseline` 新变体 → `None` |
| `src/lib.rs` | 无新重导出（`LinkTarget` 已有） |
| `src/canvas.rs` | **不改** |
| `src/backend/*` | **不改** |
| `src/error.rs` | **不改** |
| `tests/` 或 `document.rs` / `paragraph.rs` `#[cfg(test)]` | RecordingCanvas 行为断言（见 §6） |

依赖方向不变：layout 不 `use krilla`；注解仅经 `Canvas::annotate_link`。

---

## 4. Layout algorithm

### 4.1 Assemble coalesce（`Paragraph::append`）

1. 空串 → return（与 today 相同）。
2. `sealed`（`blank_to_end` 后）→ `RunAfterFillBlank`。
3. 令 `link = self.active_link.clone()`。
4. 若最后一项为 `Text(last)` 且 `last.face == face` **且 `last.link == link`** → `push_str`，return。
5. 否则 push `InlineItem::Text(TextRun { text, face, link })`。

相邻同 face、同 `Option<LinkTarget>`（含 `None`）合并；URI 或 face 变化则新 run（`coalesce-same-uri` 仅在同 face 时合并相邻同 URI）。

### 4.2 `prepare_inline` flush（link 边界）

遍历 `items`：

- **`InlineItem::Text(run)`**  
  - 若 `pending` 非空且 `pending.last().link != run.link` → `flush_text`（与 blank 相同）。  
  - `pending.push(run)`。
- **`InlineItem::Blank`** → 先 flush pending，再 push blank span（today 逻辑）。

`flush_text` → `measure_inline` → push `MeasuredInline { …, link: pending[0].link.clone() }`（组内 link 一致；debug_assert 全 run 同 link）。

Bold/body 混排**不**因 face  flush，仍在一个组；断行与 today 相同。

### 4.3 断行（无 URI 逻辑）

`break_spans` / `pack_lines` / `emit_line` 不变。换行切开组 → 每视觉行一段 `Seg::Glyphs { group, start, end }`，同组共享 `MeasuredInline.link`。

### 4.4 `place_line`（记录 x）

对每个 `Seg::Glyphs { group, start, end }`：

1. 空 glyph range → `continue`（today）。
2. `advance = cluster_width_pt(…)`；`x` 为 seg 起点（已含 `line_x` 与 align / 首行缩进）。
3. 照常 `glyph_runs_for_line` → `LinePiece::Glyphs`（可多 run，bold/body 切分）。
4. **若 `groups[group].link.is_some()` 且 `advance > 0` 且 finite** → push `LinePiece::Link { x0: x, x1: x + advance, target: clone }`（**一条** link piece 覆盖整 seg 宽度，不按 font 切分）。
5. `x += advance`。

`Seg::Blank` 不变；blank 不带 link（`blank-no-link`）。

### 4.5 `fragment_from_lines`（y + `FragmentCommand::Link`）

对每个 `LinePiece`：

- `Glyphs` / `Stroke`：today。
- `Link { x0, x1, target }`：  
  - `y0 = index * line_height`（`lines_height(index, line_height)`）。  
  - `w = x1 - x0`；若 `w > 0` → `Rect::try_pt(x0, y0, w, line_height)?` → push `FragmentCommand::Link { rect, target }`。

命令顺序：与 `place_line` 的 piece 顺序一致（通常 Glyphs… 与 Link 交错；RecordingCanvas 只断言 link rect，不断言与 glyph 的相对顺序，除非测试需要）。

### 4.6 `FragmentCommand::Link` 绘制链

- **`Fragment::with_offset(dx, dy)`**：`Link { rect }` 与 `Image` 相同，`rect.translated(dx, dy)?`。
- **`draw_fragment`**：`canvas.annotate_link(rect.translated(ox, oy)?, target.clone())?`。
- **`paint_pages`**：behind / header / body / footer / front 均已走 `draw_fragment`，无需改 paginate。
- **`first_baseline`**：`Link` → `None`（列表 marker 对齐不受影响）。

Backend：`KrillaCanvas` 队列 → `emit_page` 在 content stream 之后 `add_annotation`（已有）。

---

## 5. Error cases

| 场景 | 结果 | 阶段 |
| --- | --- | --- |
| `LinkTarget::uri("")` / `Band::*_link("", …)` / `Paragraph::link("", …)` | `Error::EmptyLink` | DSL 构造 |
| URI 仅空格 `" "` | `Ok(LinkTarget)`，正常写入 PDF | 构造 |
| `blank_to_end` 后再 `text`/`bold`/`link` 内 append | `Error::RunAfterFillBlank` | assemble |
| 段落宽度 ≤ 0、首行缩进导致首行宽度 ≤ 0 | `Error::InvalidGeometry` | layout_runs |
| 链接 seg `advance == 0`、空 glyph range | 省略 link 命令 | layout |
| 仅 `link` 闭包且全 `text("")` | 无 `AnnotateLink`；段落仍可为空 items | assemble + layout |
| `KrillaCanvas::annotate_link` 无 open page | `InvalidCanvasState` | 仅误用 API；正常 `paint_pages` 不会 |
| `Rect::try_pt` 失败（non-finite） | `Error::InvalidGeometry` | 理论上不应出现若 advance 已过滤 |

**不做：** scheme 校验、网络访问、文内 destination、`linkToPage`、空 URI 在 layout 再拦（已在 newtype）。

---

## 6. Test plan（RecordingCanvas 行为断言）

测试字体：沿用 fixtures 与现有 paragraph/list 测试的 `RecordingCanvas` + `FontStore` 模式；断言 `RecordedPage` / `commands()` 中的 `CanvasCommand::AnnotateLink { rect, target }`。

| # | 场景 | 断言要点 |
| --- | --- | --- |
| T1 | 单行短链 | 恰好 1 个 `AnnotateLink`；`target.as_str()` 为 URI；`rect.y` = 片段 origin 对应行顶；`rect.height` ≈ body `line_height`；`rect.width` ≈ 链接文字 ink 宽（与同行 `Glyphs` 范围一致，容差 `PT_ABS_TOLERANCE`） |
| T2 | 窄栏换行 | 同一 URI 的长串 → **2+** `AnnotateLink`；各行 `rect.y` 相差 `line_height`；同行 width 为该行切片宽 |
| T3 | 同链内 bold + body | 单行 → **1** link rect（宽 = 整段 seg），非 2 个按 font 切分 |
| T4 | 空 URI | `Paragraph::link("", …)` 或 `LinkTarget::uri("")` → `EmptyLink`；RecordingCanvas **无** `AnnotateLink` |
| T5 | blank 打断链 | `link { text; blank_em(2); text }` → 2 个 rect，中间 blank 无 link；blank 仍仅有 `Stroke` |
| T6 | 无装饰 | 有链与无链段落 `FragmentCommand` 的 `Glyphs`/`Stroke` 集合一致（或 glyph 命令序列相同）；无额外 `Stroke` 当下划线 |
| T7 | 页眉 `left_link` | `measure` + `paint` header band → 至少 1 个 `AnnotateLink`；rect x 反映 slot align（Start） |
| T8 | `with_offset` | 链放在 `columns` 或带 dx 的 stack 内 → link rect 与 glyph origin 同幅平移（对比无链 baseline） |
| T9 | 相邻同 URI 同 face | `link { text("ab"); text("cd") }` assemble 为 1 run → 换行未发生时 1 rect |

**backend-krilla：** 保持现有 `emit` 探针 + 文档 `%PDF` 冒烟；不新增压缩流数字断言。若 T1–T2 仅走 RecordingCanvas，backend 可依赖已有 `annotate_link` 集成测试。

**回归：** 现有 `blank_breaks_text_coalescing`、`paragraph_coalesces_adjacent_faces` 等仍绿；为 `append` 增加「不同 link 不合并」单测。

---

## 7. 实现顺序（建议 PR 切分）

1. `TextRun` + `append` + `prepare_inline` + `MeasuredInline`（无 DSL 时可先用 `TextNode::from_items` 单测 layout）。
2. `place_line` / `fragment_from_lines` / `FragmentCommand::Link` / `with_offset` / `draw_fragment` / `first_baseline`。
3. `Paragraph::link` + `Band::*_link`。
4. RecordingCanvas 集成测试 T1–T9。
