# Knife 15 — §8 文字外链 URI

基线 `feat/inline-blank` @ `a232ce1`。权威是 `docs/提案书.md` §8 与 §2.0：文字上挂外部 URI；断行后按行上的那一段写 PDF 链接注解；库只保存字符串，不访问、不校验 scheme；空串在构造期拒绝。正文，以及已经走同一条 `TextNode` → 片段 → `draw_fragment` 的页眉页脚，属于本刀。文内跳转、仅图可点、层文字、单元格、下划线都不在本刀。

自治默认全部沿用，没有反转。调用是 `Paragraph::link` 闭包；页眉页脚是整槽 `Band::left_link` / `right_link`。

下游已经完成，本刀只接线：`LinkTarget::uri`、`Error::EmptyLink`、`Canvas::annotate_link`、`RecordingCanvas` 的 `AnnotateLink`、`KrillaCanvas` 的 `QueuedLink`、`emit` 里的 `LinkAnnotation` + `LinkAction`。layout 只调 `Canvas` trait，不 `use krilla`。

## 1. Usage sketch

`document()` 闭包仍返回 `Result`。`link` / `left_link` / `right_link` 和 `text` 一样不返回 `Result`，空 URI 记在构建器上，整份文档在 `paint` 失败。

```rust
let rendered = document(PageSize::A4, |doc| {
    doc.font(font_bytes)?;
    doc.bold_font(bold_bytes)?;
    doc.margins(Margins::uniform(NonNegative::pt(36.0)?));

    doc.header(|band| {
        // 整槽一个 URI。right 仍是普通字符串，不带链接。
        band.left_link("https://example.test/home", "quire");
        band.right(band.page().number().to_string());
    });

    doc.paragraph(|p| {
        p.text("见");
        // 闭包里的 text/bold 带上同一个 URI。Body 与 Bold 换行后仍是每视觉行一个矩形。
        p.link("https://example.test/a", |link| {
            link.text("说明");
            link.bold("全文");
        });
        p.text("。");
    });

    // 同一 URI、同一 Face、中间没有 blank：组装时拼成一段。
    doc.paragraph(|p| {
        p.link("https://example.test/a", |link| link.text("甲"));
        p.link("https://example.test/a", |link| link.text("乙"));
    });

    // blank 写在闭包外，不带 URI，并把前后两段拆开。
    doc.paragraph(|p| {
        p.link("https://example.test/a", |link| link.text("甲"));
        p.blank(NonNegative::pt(12.0)?);
        p.link("https://example.test/a", |link| link.text("乙"));
    });

    Ok(())
})?;
```

页码写进 URI 也走同一钩子。钩子每页执行，`lower` 时才构造那一页的 `LinkTarget`：

```rust
doc.header(|band| {
    let uri = format!("https://example.test/p/{}", band.page().number());
    band.left_link(&uri, "本页");
});
```

调用方不自己持有 `LinkTarget`。`link` / `left_link` / `right_link` 收 `&str`（实现用 `impl AsRef<str>`），内部立刻 `LinkTarget::uri`。存下来的字段只有 `Option<LinkTarget>`，空串进不了 run。

`Linked` 只有 `text` 和 `bold`。它借走 `Paragraph`，闭包里写不了外层的 `p.text`、`p.blank`、`p.link`。换 URI 就再调一次 `p.link`。`first_line_indent` 仍写在 `Paragraph` 上，缩进整段的首行，不是链接的属性。

`left` 与 `left_link` 写同一个槽，后一次覆盖前一次。`left` 覆盖后该槽没有 URI。`right` / `right_link` 同样。

## 2. Types / data shapes

链接是 run 上的属性，不是新的流节点，也不是新的 `InlineItem`。

```text
组装
  TextRun { text: String, face: Face, link: Option<LinkTarget> }
  InlineItem::Text(TextRun) | InlineItem::Blank(InlineBlank)   // Blank 没有 link 字段

测量组（prepare_inline 在 link 变化处切开之后）
  MeasuredInline { text, glyphs, link: Option<LinkTarget> }
  Span / Seg 保持现状：Seg::Glyphs { group, start, end } 不带 payload

放置（片段局部，先只有 x）
  LinePiece::Glyphs(GlyphRun)          // 墨迹，按 FontId 切开
  LinePiece::Stroke(InlineStroke)      // 填空
  LinePiece::Link { x0, x1, target }   // 命中范围；x1 是这段 advance 的终点

片段
  FragmentCommand::Link { rect: Rect, target: LinkTarget }

绘制
  rect 再加 with_offset 与页原点 → Canvas::annotate_link(rect, target)
```

`LinkTarget` 继续是现在的 newtype（`src/canvas.rs`）：`uri("")` → `Error::EmptyLink`；不 trim；不看 scheme；`PartialEq` 按字符串内容。它不是 `Copy`，所以不放进 `cluster_tagged<T: Copy>`，也不放进 `Span<T: Copy>`。URI 只活在组旁边，`Seg::Glyphs.group` 指回这个组。

`TextRun::new(text, face)` 保持原签名，`link` 填 `None`。现有相等性测试不用改。新增：

```rust
pub(crate) fn with_link(
    text: impl Into<String>,
    face: Face,
    link: Option<LinkTarget>,
) -> Self
```

`TextNode::new` 与 `Flow::text` 仍走 `TextRun::new`，单元格和未加链接的槽因此没有 URI。

公共面只有这些新增：

| 符号 | 形状 |
| --- | --- |
| `Paragraph::link` | `fn link(&mut self, uri: impl AsRef<str>, f: impl FnOnce(&mut Linked<'_>))` |
| `Linked::text` / `Linked::bold` | 与 `Paragraph::text` / `bold` 相同的 `impl AsRef<str>`，返回 `()` |
| `Band::left_link` / `right_link` | `fn left_link(&mut self, uri: impl AsRef<str>, text: impl AsRef<str>)` |
| `quire::Linked` | 与 `Blank` 一样从 `lib.rs` 再导出。字段私有，没有公开构造函数 |

`Linked` 内部：

```rust
pub struct Linked<'a> {
    paragraph: &'a mut Paragraph,
    target: LinkTarget,
}
```

`Band` 的槽从 `Option<String>` 换成 `Option<Slot>`，并加上与 `Layer` 相同的 `error: Option<Error>`：

```rust
struct Slot {
    text: String,
    link: Option<LinkTarget>,
}
```

`left` / `right` 写入 `link: None`。`left_link` / `right_link` 在 `LinkTarget::uri` 成功时写入 `Some(target)`。`Band::lower` 对非空 `text` 做 `TextRun::with_link(text, Face::Body, slot.link)`，再 `TextNode::from_items`。空 `text` 仍是空 `items`（与今天的 `TextNode::new("")` 一样），URI 不落地。对齐仍是左槽 `Start`、右槽 `End`。栏宽、左右 margin 用空栏缩进，这段布局不动。

`FragmentCommand::Link` 的 `rect` 在片段局部坐标。宽高都要能通过现有的 `Rect::try_pt`（严格为正）。零宽不构造 `Rect`。

## 3. Module map

| 文件 | 改动 |
| --- | --- |
| `src/document.rs` | `Paragraph::link`、`Linked`、`append` 的合并键、`Band` 的 `Slot` 与 `error`、`band_content` 在 `lower` 前返回 `error`。测试放这里 |
| `src/lib.rs` | 再导出 `Linked` |
| `src/layout/paragraph.rs` | `TextRun.link`、`TextRun::with_link`；`prepare_inline` 按 link flush；`MeasuredInline.link`；`place_line` 记 `x0`/`x1`；`fragment_from_lines` 发出 `FragmentCommand::Link` |
| `src/layout/flow.rs` | `FragmentCommand::Link`；`with_offset` 平移 `rect`（与 `Image` 同一臂）；`draw_fragment` 调 `annotate_link` |
| `src/layout/list.rs` | `first_baseline` 对 `Link` 返回 `None`（与 `Image` 一样）。这是除 `flow.rs` 以外唯一要补的穷尽匹配 |

`Fragment::with_offset` 已经是栏（`columns.rs` 的 `dx`）、栈（`flow.rs` 的 `dy`）、列表缩进（`list.rs`）的唯一平移点。这些文件不用改。`decorate.rs` 的 `measure_band` 与 `paginate.rs` 的 `paint_pages` 已经把带和正文送进同一个 `draw_fragment`，也不改。

不改：`src/canvas.rs`、`src/error.rs`、`src/geom.rs`、`src/text/line_break.rs`、`src/text/cluster.rs`、`src/backend/**`、`Cell`、`Layer::text_at`、`single_line_run`、`ImageNode`。不新增错误变体。不新增 PDF 后端。

## 4. Layout algorithm

### 组装合并

`Paragraph::text` / `bold` 调用 `append(face, None, text)`。`Linked::text` / `bold` 调用 `append(face, Some(self.target.clone()), text)`。

`append` 的顺序：

1. `text` 为空：返回。这一步仍在 `sealed()` 之前，所以 `blank_to_end` 之后的 `link.text("")` 不产生 `RunAfterFillBlank`。
2. 上一项是 `blank_to_end`：`note(RunAfterFillBlank)` 并返回。
3. 上一项是 `Text`，且 `face` 相等、`link` 相等（`Option<LinkTarget>` 的字符串相等）：`push_str`。
4. 否则 `TextRun::with_link` 推一个新 run。

因此：

- 两次相邻的 `link(同一个 URI)`，中间没有别的 item，Face 又相同：一个 run。
- 同一次闭包里的 `text` + `bold`：两个 run，URI 相同，Face 不同，不在这里拼接。
- `None` 与 `Some`、以及两个不同 URI：两个 run。
- 任何 `Blank` 打断相邻关系，后面的 run 不能与前面的 `push_str`。

`Paragraph::link` 先 `LinkTarget::uri`。失败则 `note(EmptyLink)` 并且不调用闭包。成功才把 `target` 交给 `Linked`。`note` 只在 `error` 还是 `None` 时写入，与今天 `RunAfterFillBlank` 的“第一次为准”相同。`Flow::paragraph` 已有的路径不变：`paragraph.error` 有值就 `push(Err)`，不把 items 放进流。

### prepare_inline flush

`pending` 仍是一组即将交给 `measure_inline` 的 `TextRun`。flush 条件变成两个：遇到 `Blank`，或下一个 `Text` 的 `link` 与 `pending` 最后一项的 `link` 不同。`None → Some`、`Some → None`、`Some(a) → Some(b)` 都 flush。Face 变化不 flush。

同一组里的 run 共享一个 `Option<LinkTarget>`。`measure_inline` 把 `runs[0].link.clone()` 放进 `MeasuredInline`。组内 Face 仍交给现有的 `cluster_tagged`，再按 `FontId` 画成多个 `GlyphRun`。

这里只多推 `Span::Group`。不插入 blank，不改 `line_break.rs`。断点仍由 pair 表和 blank 的 advance 决定；组边界只让 `pack_lines` 关闭当前 `Seg`，不额外制造换行。一个链接组若被换行切开，每一视觉行各有一个 `Seg::Glyphs`，`group` 相同。这就是「每行一个矩形」。

### place_line 的 x

`line_x = frame_x + align_x(frame_width, line.width(), align)` 保持现状。首行 `frame_x` 是 `first_line_indent`，其余行是 0。`Start` 的 `align_x` 是 0；带的右槽是 `End`，`align_x = frame_width - line.width()`。含 `ToEndOfLine` 的行宽等于预算，`align_x` 为 0。链接使用同一个 `x` 游标，不另算对齐。

对每个 `Seg::Glyphs { group, start, end }`：

1. 字形范围空：`continue`（行首被丢掉的 ASCII 空格走这里）。
2. `advance = cluster_width_pt(整个范围)`。
3. `glyph_runs_for_line` 的结果照旧推成 `LinePiece::Glyphs`。Face 切换可以产生多个 `GlyphRun`，`x` 仍按整个 seg 前进。
4. `groups[group].link` 为 `Some`，且 `advance` 有限并 `> 0`：再推 `LinePiece::Link { x0: x, x1: x + advance, target }`。一个 seg 一条，不按 `GlyphRun` 拆。
5. `x += advance`。`saw_glyphs` 只在字形臂上置位。纯填空行插入的空 `GlyphRun` 规则不变，那种行没有 `Link`。

`Seg::Blank` 只产生 `Stroke`，不产生 `Link`。

### fragment_from_lines 的 y 与 FragmentCommand::Link

`y0 = index * line_height`，`line_height` 来自正文字体的 `line_metrics`（`baseline = ascent`，`line_height = max(natural, font_size * factor)`，默认 factor 1.2）。Bold 不改变行高。

```text
Rect::try_pt(x0, y0, x1 - x0, line_height)
```

矩形顶边是行盒顶，不是 glyph origin（origin 的 y 是 `y0 + ascent`）。高度是整行行盒。坐标相对当前片段：跨页时 `ParagraphNode::split` 按整行切开 `LineBox`，tail 再进 `fragment_from_lines` 时 index 从 0 计，y 重新从 0 起。`place_line` 不写页坐标。

`x1 > x0` 已在 `place_line` 保证。`line_height` 由 `line_metrics` 保证为正。`Rect::try_pt` 若仍失败，把 `InvalidGeometry` 向上传，与同函数里 `Point::try_pt` 一样。

一条命令序列里，某个 seg 的 `Glyphs` 紧挨在它的 `Link` 前面。同一行两个 URI 就是：字形、链接、字形、链接。同一链接里的 Body+Bold 是：两个 `Glyphs`，然后一个 `Link`。

`Link` 不参与行高、段后距、`saw_glyphs`。`first_baseline` 跳过它，列表标记仍对齐到第一个 `Glyphs` 的基线。

### 平移与绘制

`Fragment::with_offset`：

```rust
FragmentCommand::Link { rect, .. } => *rect = rect.translated(dx, dy)?,
```

这条臂覆盖栈的 `dy`、栏的 `dx`（页眉左右槽和左右 margin 缩进）、列表缩进。漏写成原样返回会留下栏局部坐标，穷尽匹配只保证写了分支，测试要锁住平移结果。

`draw_fragment` 在 `PlacedFragment.origin` 上再平移一次，然后：

```rust
canvas.annotate_link(rect.translated(ox, oy)?, target.clone())?
```

`paint_pages` 的顺序不变：behind → header → body → footer → front，全部经过 `draw_fragment`。页眉原点 `(0, 0)`，页脚原点 `(0, page_height - bottom)`，正文原点是四边 margin。链接注解不进入内容流，层的绘制顺序不删除已经记下的注解；本刀不做“后画的层盖住注解”这种命中裁剪。

`annotate_link` 只出现在 `draw_fragment`。`paint_pages` 调用它时页已经 `start_page`。不在测量阶段打注解，避免 `KrillaCanvas` 的 `InvalidCanvasState`。

字形管线不变：charmap、advance、断行、同一批 glyph id。链接不改颜色、不追加下划线 `Stroke`、不换字体。

## 5. Error cases

不新增 `Error` 变体。空 URI 就是现有的 `EmptyLink`（显示 `"link URI is empty"`）。

| 情况 | 结果 |
| --- | --- |
| `p.link("", …)` | `note(EmptyLink)`，闭包不执行。`Flow::paragraph` 丢弃该段。`paint` 在 `section.lower` → `into_nodes` 返回，不进入 `paginate_sections`，canvas 没有任何命令，也没有 `start_page` |
| `band.left_link("", …)` / `right_link` | 钩子在 `measure_bands` 才跑（URI 可以含当页页码，不能在 `document()` 闭包里提前验）。`note(EmptyLink)`，不改槽。`band_content` 先返回 `error`，再谈 `lower`。失败点在 `plan_pages` 之后、`paint_pages` 之前，canvas 仍空 |
| 段落上已有别的错误再 `link("")` | `note` 不覆盖。闭包仍不执行。对外是先记下的那个错误 |
| `blank_to_end` 之后 `link(合法 URI, \|l\| l.text("字"))` | 闭包执行，`append` 记 `RunAfterFillBlank`。文档失败，canvas 空 |
| `blank_to_end` 之后 `link(合法 URI, \|l\| l.text(""))` | 空串在 `sealed` 检查前返回。不报错，没有 run，没有注解 |
| `blank_to_end` 之后 `link("", …)` | 先拒绝 URI，闭包不执行。错误是 `EmptyLink` |
| 合法 URI，闭包里只有 `text("")` / `bold("")` | 成功。没有 run，没有 `AnnotateLink` |
| `left_link(合法 URI, "")` | 成功。槽仍在（与今天 `left("")` 一样占一栏），`items` 为空，没有注解 |
| URI 为 `" "`、`"https://example.test/a"`、`"mailto:a@b.c"`、`"not a uri"`、`"javascript:alert(1)"` | 全部接受，字符串原样写入注解。不 trim，不访问 |
| 链接文字缺字 | 现有 `MissingGlyph`，测量期失败，canvas 空 |
| 链接里 `bold` 但没有 `bold_font` | 现有 `UnknownFont { id: 1 }`，canvas 空 |
| 带内链接把行盒撑得高于 margin | 现有 `DecorationTallerThanMargin`，canvas 空。链接不改变行高 |
| 行首 ASCII 空格被断行丢掉，该 seg 为空 | 不发矩形，不报错 |
| seg 的 advance 不是有限数或不大于 0 | 不发矩形，不报错 |
| 行尾空格仍留在 seg 里 | 矩形宽度含这段 advance。这是现有断行结果，不另做裁剪 |
| 单元格字符串、`Layer::text_at`、图片、列表标记 | 没有链接入口。它们不产生 `AnnotateLink` |

`Rect` 构造失败只可能来自非有限坐标，按 `InvalidGeometry` 上抛，不单开链接错误。

## 6. Test plan

行为断言打在 `RecordingCanvas` 上，测试放在 `src/document.rs` 的现有 `mod tests`。坐标用 `pt_near`。URI 与条数用相等。夹具字体是 `fixture_bytes()`，样例用它有的拉丁字母，不用会 `MissingGlyph` 的汉字。

从一页的命令里取出 `DrawGlyphRun` 与 `AnnotateLink`。一个 run 的宽是 `Σ advance_norm * font_size`。

**几何关系（每个成功用例都用得上）**

- 链接矩形的 `x` 与该段第一个 glyph run 的 `origin.x` 用 `pt_near`。
- 矩形顶边在基线之上：`rect.y < glyph.origin.y`，且 `rect.y + rect.height > glyph.origin.y`。
- 换行时，两行 glyph origin 的 `y` 差等于两行链接矩形的 `y` 差，也等于矩形高度。这就把高度钉在行盒上，而不是钉在 glyph origin 上。
- 矩形宽等于该视觉行里、这次链接所覆盖的那些 glyph run 的 advance 之和。
- 无链接的段落：`AnnotateLink` 条数为 0。
- 同一段文字，一次包在 `link` 里、一次不包。去掉 `AnnotateLink` 之后，其余 `CanvasCommand` 相同（字形、颜色、没有额外 `Stroke`）。

**用例**

| 测试 | 断言 |
| --- | --- |
| `one_line_link_rect_matches_glyph_advance` | `p.link(uri, \|l\| l.text("Hello"))`，margin 0。命令是一个 `DrawGlyphRun` 后接一个 `AnnotateLink`。URI 字符串相等。`rect.x` 与 glyph `x` 都在 0 附近。宽等于该 run 的 advance |
| `wrap_emits_one_rect_per_visual_line` | 页宽窄于 `"Hello World"`、宽于 `"Hello"`。同一个 URI。两个 `AnnotateLink`，URI 相同。第二矩形的 `y` 等于第一矩形的 `y + height`。两宽分别等于两行的 advance。第二行 `x` 在 0 附近 |
| `bold_and_body_share_one_rect_per_line` | 先 `bold_font`。闭包里 `text("Ab")` + `bold("Cd")`，放得下一行。两个 `DrawGlyphRun`，一个 `AnnotateLink`。宽等于两段 advance 之和 |
| `unlinked_prefix_stays_outside_the_rect` | `text("See ")` 然后 `link(..., "this")`。一个注解。`rect.x` 等于链接 run 的 origin，等于前缀 advance。宽只等于 `"this"` |
| `different_uris_are_adjacent_rects` | 同一行两个 `link`，URI 不同。两个注解，目标字符串不同。第二个 `x` 等于第一个 `x + width` |
| `adjacent_same_uri_coalesces` | 两次 `link(同一 URI)` 写 `"A"` 与 `"B"`。组装后的 `TextNode` 是一个 `Face::Body` run，`link == Some`，文本 `"AB"`。画出来只有一个 `AnnotateLink`，宽等于 `"AB"` |
| `blank_splits_same_uri_into_two_rects` | 两次相同 URI，中间 `blank(12pt)`。两个注解，中间有一条填空 `Stroke`，没有第三个注解。第二矩形的 `x` 大于第一矩形的 `x + width` |
| `first_line_indent_shifts_only_the_first_rect` | 缩进后的两行链接。首行 `rect.x` 等于缩进，次行 `rect.x` 在 0 附近。两行 glyph `x` 与各自矩形一致 |
| `second_paragraph_rect_includes_stack_dy` | 一段无链接文字，再一段链接。链接 `rect.y` 等于第二段 glyph 的行盒顶，大于 0。与第一段 glyph 的 `y` 差等于行盒高度。这锁住 `with_offset(0, dy)` |
| `list_item_rect_includes_indent` | `ul` 里的 `paragraph` + `link`。`rect.x` 等于该段 glyph 的 `x`，且大于 0 |
| `header_and_body_rects_use_their_own_origins` | top/left margin 36。`left_link` 短字符串，正文另有一个 `link`。页眉注解 `y` 在 0 附近，`x` 等于页眉 glyph 的 `x` 且在 36 附近。正文注解 `y` 在 36 附近。两条 URI 各自正确 |
| `footer_rect_sits_on_the_bottom_band` | 只设 bottom margin。`footer` 里 `right_link`。`rect.y` 等于 `page_height - bottom`。`rect.x` 等于该槽 glyph 的 `x`（`End` 对齐已经进 `place_line`） |
| `header_uri_can_differ_per_page` | `page_break` 两页，`left_link` 的 URI 含 `page().number()`。两页各一个注解，字符串分别为 `…/1` 与 `…/2`，`y` 都在 0 附近 |
| `left_after_left_link_clears_the_uri` | 先 `left_link` 再 `left`。`AnnotateLink` 为 0。反过来先 `left` 再 `left_link`：一条注解 |
| `empty_uri_on_paragraph_paints_nothing` | 用现成的 `assert_fails_with_empty_canvas(..., &Error::EmptyLink)`。闭包内层没有把文字写进 `TextNode`（闭包未执行） |
| `empty_uri_on_band_paints_nothing` | `header` 里 `left_link("", "x")`，margin 足够高。同样是 `EmptyLink` 且 `commands()` 为空 |
| `empty_text_inside_link_has_no_annotation` | `link(合法 URI, \|l\| l.text(""))`。`document()` 成功，注解 0 条 |
| `space_uri_is_stored_as_given` | URI 为单个空格，文字 `"A"`。成功，`target.as_str() == " "` |
| `unrecognized_scheme_is_stored_as_given` | URI `"not a uri"`。成功，目标字符串就是这 9 个字符 |
| `cell_and_layer_text_do_not_annotate` | 同一文档里：单元格 `"https://example.test"`、`behind` 的 `text_at`、一段真正的 `p.link`。注解只有 1 条，URI 是段落那个 |
| `link_across_pages_restarts_y` | 页高只够一行。两页各一个注解，两页的 `rect.y` 都等于该页正文顶（margin 0 时为 0） |

组装期再加一条与 `paragraph_coalesces_adjacent_faces_and_ignores_empty_appends` 同类的结构断言：不同 URI 得到两个 run；相同 URI 不同 Face 得到两个 run 且 `link` 相同。这只防合并键，不代替上面的矩形断言。

`src/backend/emit.rs` 的 `observable_probe_contains_path_ops_and_annot_rect` 已经对直接 `annotate_link` 检查了 `%PDF` 和 URI 字符串。本刀不改 backend，不新增内容流数字断言。若要冒烟，只在 `backend-krilla` 下对含链接的 `Rendered::to_pdf` 断言字节以 `%PDF` 开头。
