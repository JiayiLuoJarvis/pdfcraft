# R1 候选 2：内容盒优先，再水平偏移

`Band::lower` 先算出内容盒宽度 `content_w = page_width − inset_left − inset_right`，左右槽只在这个盒子里排（单槽占满盒子，双槽是盒内 gap 0 的两条 `Ratio(1)`）。盒子再在全页宽的带框里平移 `inset_left`。`measure_band` 仍用整页宽、原点 x = 0。lowering 只留在 `src/document.rs`。

页眉页脚的绘制原点保持页面 x = 0。inset 为 0 时字形可以落在 x = 0 和 x = `page_width`。inset 非 0 时，字形和链接矩形的 x 是「盒子内对齐」再加上 `inset_left`。

## 1. 用法

```rust
// 默认：横跨 0 … page_width（§2.10）。不读左右 margin。
doc.header(|b| {
    b.left("ACME");                                  // 左槽原点 x = 0
    b.right(format!("第 {} 页", b.page().number));   // 右槽右缘 x = page_width
});

// 想跟正文齐：调用方把当页 margin 传进 inset
doc.header(|b| {
    let m = b.page().margins;
    b.inset(m.left(), m.right());
    b.left("文号");                                  // 左槽原点 x = m.left()
    b.right(format!("第 {} 页", b.page().number));   // 右槽右缘 x = page_width − m.right()
});

// 只缩左边。右缘仍贴页边。
doc.footer(|b| {
    b.inset(NonNegative::pt(10.0).unwrap(), NonNegative::ZERO);
    b.left("页脚");                                  // 原点 x = 10
});
```

`inset` 与 `left` / `right` / `left_link` / `right_link` 的调用顺序无关。多次 `inset` 以最后一次为准。不调用就是 `(0, 0)`。没有槽时 `inset` 不产生节点。

`page().margins` 仍传给闭包（§2.10）。lower 不再用它铺轨。

## 2. 类型与签名

只改 `src/document.rs`。不新增公共类型，不改 `lib.rs`，不改 `layout/`。

```rust
pub struct Band {
    page: Page,
    inset_left: Length<NonNegative>,   // 默认 NonNegative::ZERO
    inset_right: Length<NonNegative>,  // 默认 NonNegative::ZERO
    left: Option<BandSlot>,
    right: Option<BandSlot>,
    error: Option<Error>,
}

impl Band {
    /// Insets the slot box from the page edges. Both default to zero, so slots
    /// span `0 … page_width`. Pass `page().margins` sides to line up with the body.
    pub fn inset(&mut self, left: Length<NonNegative>, right: Length<NonNegative>);

    // 保持：page / left / right / left_link / right_link
}
```

- 参数是 `Length<NonNegative>`，与 `Margins::left()` / `right()` 相同，工单示例 `b.inset(m.left(), m.right())` 不用转换。非负在构造期已经保证，`inset` 返回 `()`，不走 `note`。
- 字段私有，和 `left` / `right` / `error` 一样。工单里的字段名是内部数据名。
- `band_content` 构造 `Band` 时两个字段填 `NonNegative::ZERO`。
- 结构注释删掉 “Slots inset by the left and right margins so they line up with the body.”，改成槽横跨整页宽，调用方用 `inset` 缩进内容盒。`Layer` 的 “not inset by margins” 不动。

## 3. Lower 伪代码

槽文字的 `TextNode` 构造与现在相同：左 `TextAlign::Start`，右 `TextAlign::End`，空串仍是空 items，链接仍挂在 `TextRun` 上。`self.error` 仍在造节点之前返回。

```text
fn lower(self, style) -> Result<Vec<FlowNode>>:
    if self.error is Some(e): return Err(e)

    slots = 按 (left, Start), (right, End) 收集存在的槽
    if slots 为空: return []          // 无槽则 inset 被忽略

    page_w    = self.page.size.width().as_pt()    // 与 measure_band 的 frame.width 同一页宽
    inset_l   = self.inset_left.as_pt()
    inset_r   = self.inset_right.as_pt()
    content_w = page_w - inset_l - inset_r
    if not (content_w > 0.0):
        return Err(DecorationWiderThanFrame {
            width: inset_l + inset_r,
            avail: page_w,
        })

    content = slot_box(slots)         // 见下。这是唯一的槽排版
    return place(content, inset_l, inset_r, content_w)


fn slot_box(slots) -> Vec<FlowNode>:
    if slots.len() == 1:
        return [那个 TextNode]       // 单槽占满内容盒，不切半
    // 双槽：盒内两条等分，gap 0。Start / End 各在自己的半盒
    return [Columns(gap = NonNegative::ZERO, [
        (Ratio(1), [左 Text]),
        (Ratio(1), [右 Text]),
    ])]


fn place(content, inset_l, inset_r, content_w) -> Result<Vec<FlowNode>>:
    if inset_l == 0 and inset_r == 0:
        return content                // 带框已经等于内容盒。单槽不包 Columns

    tracks = []
    if inset_l > 0:
        tracks.push((Fixed(Positive::pt(inset_l)?), []))
        // 空轨只编码平移。ColumnsNode::split 会 with_offset(x, 0)
    tracks.push((Fixed(Positive::pt(content_w)?), content))
        // 内容盒。宽度在进求解器之前就定死，不靠 Ratio 吃剩余

    if tracks.len() == 1:
        // 只可能是 inset_l == 0 且 inset_r > 0。
        // ColumnsNode::new 拒绝单列，Fixed 又不能为 0，无法做一条宽度 0 的前导轨。
        // 这条尾轨的宽度等于已经从 content_w 里减掉的余量，不参与决定槽宽。
        tracks.push((Fixed(Positive::pt(inset_r)?), []))

    return [Columns(gap = NonNegative::ZERO, tracks)]
```

`place` 的外层列没有 `Ratio` / `Auto`。`resolve_column_widths` 在「全是 Fixed、fixed 之和 ≤ 页宽」时允许 `rest > 0`。因此 `inset_l > 0` 时右缩进是框右端没被占用的余量，不需要第三条空轨。

四种外形：

| inset | 槽 | lower 产出 | 左槽原点 | 右槽右缘 |
| --- | --- | --- | --- | --- |
| (0, 0) | 单槽 | 一个 Text | 0 | 该槽右缘 = page_width（End 时） |
| (0, 0) | 双槽 | `R1 R1`，gap 0 | 0 | page_width |
| (l, r)，l > 0 | 单或双 | `F(l) 空` + `F(content_w) 槽盒` | l | page_width − r |
| (0, r)，r > 0 | 单或双 | `F(content_w) 槽盒` + `F(r) 空` | 0 | page_width − r |

双槽时每槽宽是 `content_w / 2`，与现在「各占一半、互不重叠」相同。单槽占满 `content_w`，与现在「只剩一个槽就不包 Columns、用整段可用宽」相同。

`second_section_header_stays_at_page_edge_until_caller_insets` 的第二节是单左槽 `inset(48pt, 10pt)`。外形是 `F(48) 空` + `F(page_width − 58) Text`，没有尾部 `F(10)`。左字形 x = 48。10pt 是页框右侧余量。

## 4. 为什么平移落在列偏移上

想要的测量是：槽在宽度 `content_w` 的框里 split，再把片段平移 `inset_left`，带的放置原点仍是页面 `(0, y)`。

库里已有 `Fragment::with_offset`（`src/layout/flow.rs`），`ColumnsNode::split` 用它把每条轨的字形、路径和链接矩形平移到轨的 x。`Band::lower` 用不上这个函数：

- lower 的返回值是 `Vec<FlowNode>`，测量发生在之后的 `measure_band`。
- lower 没有 `LayoutCtx`，不能自己 split。
- `FlowNode` 没有平移变体。`Stack` 只做 `with_offset(0, y)`。
- `FlowNode::Spacer` 是竖直高度（§2.13），不能当作水平偏移。

所以内容盒的水平位置用一条前导 `Fixed(inset_left)` 空轨编码。split 时这条轨把 x 光标拨到 `inset_left`，下一条 `Fixed(content_w)` 的片段被 `with_offset(inset_left, 0)` 平移。链接矩形走同一条路径，和字形一起动。带片段的页面原点仍由 `measure_bands` 写成 `Point::try_pt(0.0, y)`，`measure_band` 的 frame 仍是 `width = size.width()`。

`inset_left == 0` 时不插前导轨（`ColumnWidth::Fixed` 要求 `Positive`，0 不合法）。右缩进已经写进 `content_w`。仅当左缩进为 0、右缩进大于 0 时，才追加一条尾部空 `Fixed(inset_right)`，用来满足 `ColumnsNode::new` 的「至少两列」。槽不和这条尾轨做兄弟；槽在 `F(content_w)` 里面。

`check_content` 已允许带内嵌套 `Columns`（`Container::Band` 会递归列栈）。外层 `ColumnsNode::new` 对内容轨的子节点按 `Container::Column` 检查，列容器允许再嵌 `Columns`。空轨是空 `Vec`，检查通过，split 高度为 0，不改变带高。

## 5. 错误

- 行盒高于带高，或槽文字宽于它拿到的框：仍由 `measure_band` 报 `DecorationTallerThanMargin` / `DecorationWiderThanFrame`。不撑开正文，不改这条映射。
- `content_w` 不是正数（含 `inset_left + inset_right >= page_width`）：在 lower 里直接 `DecorationWiderThanFrame`。此时还没有合法的 `Fixed(content_w)` 可建，不能把 `Positive::pt(0)` 的 `NonPositiveLength` 漏给调用方。无槽时不走这条检查。
- 带高 0 仍拒内容。空的前导轨高度是 0，不改变 `header_with_zero_margin_is_rejected`。
- 不新增 `Error` 变体。

过宽 inset 在钩子返回时失败，发生在任何一页开画之前，`RecordingCanvas` 仍是空的。

## 6. 验收（已有 RecordingCanvas 测试）

| 测试 | 这条设计如何满足 |
| --- | --- |
| `header_slots_span_full_page_width_without_inset_and_body_sits_below_the_band` | margin 36、不写 inset：走 `(0, 0)` 双槽，左 x = 0，右缘 = page_width。正文框仍由分页的 margin 决定 |
| `header_inset_aligns_slots_with_caller_chosen_side_margins` | `inset(36, 36)`：前导 `F(36)` + 内容盒。左 x = 36，右缘 = page_width − 36 |
| `second_section_header_stays_at_page_edge_until_caller_insets` | 第一节无 inset，x = 0。第二节 `inset(48, 10)`，单左槽 x = 48 |
| `header_and_footer_slots_annotate_at_the_band_origin` | 无 inset，链接矩形与字形同在 x = 0；页脚单左槽是整页宽的 Text |
| `header_with_zero_margin_is_rejected` | 带高检查不经过 inset |

正文坐标与有无页眉无关：lower 不改正文流。

## 7. 不在本刀

不改 `measure_band` 的 x = 0 与页宽 frame。不把 `layout` 改名为 `elements`。不把 `Band` 做成 `Flow`。不做带内 path / 图、第三槽、居中槽、左右槽重叠在同一行盒里。不给 section 加默认 inset。
