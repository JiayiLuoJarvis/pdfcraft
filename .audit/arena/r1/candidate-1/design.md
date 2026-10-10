# R1 候选 1：轨道垫片（inset 取代 margin）

方向：`Band::lower` 仍降成 `ColumnsNode`，两侧仍是空 `Fixed` 轨。唯一变化是轨宽来源：从 `self.page.margins` 改为 `self.inset_left` / `self.inset_right`。默认 0 时不插垫片，槽贴页边。`measure_band`、`layout/` 不动。

## 1. 用法

```rust
// 默认：横跨 0 … page_width（§2.10）
doc.header(|b| {
    b.left("ACME");                                  // 左槽原点 x = 0
    b.right(format!("第 {} 页", b.page().number));   // 右槽右缘 x = page_width
});

// 想跟正文齐：调用方自己用传入的 margin 缩
doc.header(|b| {
    let m = b.page().margins;
    b.inset(m.left(), m.right());
    b.left("文号");                                  // 左槽原点 x = m.left()
    b.right(format!("第 {} 页", b.page().number));   // 右槽右缘 x = page_width - m.right()
});

// 任意缩进，与 margin 无关
doc.footer(|b| {
    b.inset(NonNegative::pt(10.0).unwrap(), NonNegative::ZERO);
    b.left("页脚");
});
```

`inset` 可在 `left` / `right` 之前或之后调，结果相同。多次调用以最后一次为准。不调用就是 `(0, 0)`。

## 2. 类型与签名

只动 `src/document.rs`。不新增公共类型，`lib.rs` 不改。

```rust
pub struct Band {
    page: Page,
    inset_left: Length<NonNegative>,   // 新增，默认 NonNegative::ZERO
    inset_right: Length<NonNegative>,  // 新增，默认 NonNegative::ZERO
    left: Option<BandSlot>,
    right: Option<BandSlot>,
    error: Option<Error>,
}

impl Band {
    /// Insets both slots from the page edges. Both default to zero, so slots
    /// span `0 … page_width`. Pass `page().margins` sides to line up with the body.
    pub fn inset(&mut self, left: Length<NonNegative>, right: Length<NonNegative>);

    // 不变：page / left / right / left_link / right_link
}
```

- 参数类型选 `Length<NonNegative>`：`Margins::left()` / `right()` 直接返回它，工单示例 `b.inset(m.left(), m.right())` 不用转换。非负在类型上已保证，`inset` 不需要返回 `Result`，也不需要走 `note`。
- 字段保持私有，与 `left` / `right` / `error` 一致。工单说的「`Band` 字段」指内部数据，不是 `pub` 字段。
- `band_content` 构造 `Band` 时两字段填 `NonNegative::ZERO`。

文档注释同步改：

- `Band` 的类型注释删去 "Slots inset by the left and right margins so they line up with the body"，改成「槽横跨整页宽，减去 `inset`」。
- `Layer` 注释 "not inset by margins" 保持不变。

## 3. Lower 伪代码

与现状逐行对比，只改「margins → inset」一处来源，其余结构不动。

```text
fn lower(self, style) -> Result<Vec<FlowNode>>:
    if self.error: return Err
    column(slot, align) = 同现状（TextNode，Start / End）
    even  = Ratio(1)
    slots = [(left, Start), (right, End)] 中存在者 → (even, column)
    if slots 为空: return []                      // 无槽时 inset 不产生任何东西

    spacer(w: Length<NonNegative>) -> Option<(ColumnWidth, [])>:
        w == 0 → None
        else   → Some((Fixed(Positive::pt(w)?), []))

    tracks = spacer(self.inset_left) ++ slots ++ spacer(self.inset_right)
    //        ^^^^^^^^^^^^^^^^^^^^^^ 原为 page.margins.left()/right()

    if tracks.len() == 1: return tracks[0].nodes    // 0 inset 单槽：不包 Columns
    return [Columns(gap = 0, tracks)]
```

实际改动：

1. 把闭包 `inset` 改名为 `spacer`，避免和新方法同名。可选，不改也能编译。
2. 删掉 `let margins = self.page.margins;`。
3. 把 `inset(margins.left())` / `inset(margins.right())` 换成 `spacer(self.inset_left)` / `spacer(self.inset_right)`。

`self.page` 在 `lower` 里不再被读（`page()` 仍供闭包用）。

## 4. 几何结果

带框宽 = `page_width`（`measure_band` 不变，原点 x = 0）。列间距 0。

| inset | 槽 | 轨 | 左槽原点 | 右槽右缘 |
| --- | --- | --- | --- | --- |
| (0, 0) | 单槽 | 不包 Columns | 0 | page_width |
| (0, 0) | 双槽 | `R1 R1` | 0 | page_width |
| (l, r) | 双槽 | `F(l) R1 R1 F(r)` | l | page_width − r |
| (l, 0) | 仅左 | `F(l) R1` | l | — |

双槽时各占 `(W − l − r) / 2`，Start / End 各在自己半边，和现在的语义一致。

## 5. 错误

- 槽文字过高或过宽：路径不变，仍由 `measure_band` 报 `DecorationTallerThanMargin` / `DecorationWiderThanFrame`。带内装不下照样报错，不撑开正文。
- `l + r ≥ page_width`：`resolve_column_widths` 报 `Error::ColumnWidths { "fixed-sum-exceeds" | "no-remaining-width" }`，经 `measure_band` 的 `Err(other)` 分支原样上抛。仍在任何一页开画前失败。本刀不另造错误变体，也不在 `lower` 里做预检（见 rationale）。
- `header_with_zero_margin_is_rejected`：带高 0 的检查在 `measure_band`，与 inset 无关，行为不变。

## 6. 测试计划（RecordingCanvas）

工作区里已有的红灯测试（`tests/document.rs` 未提交 diff）覆盖了验收：

| 测试 | 断言 |
| --- | --- |
| `header_slots_span_full_page_width_without_inset_and_body_sits_below_the_band` | margin 36、不写 inset：左槽 x = 0，右槽右缘 = page_width；正文仍在 (36, 36+BASELINE) |
| `header_inset_aligns_slots_with_caller_chosen_side_margins` | `inset(m.left(), m.right())` 后左槽 x = 36，右缘 = W − 36 |
| `second_section_header_stays_at_page_edge_until_caller_insets` | 第 1 节 x = 0；第 2 节 inset 后 x = 48（读的是该页 margin） |
| `header_and_footer_slots_annotate_at_the_band_origin` | 链接矩形跟随字形 x = 0；y 不变 |

建议补一条 `src/document.rs` 单元测试：`inset` 的 `l + r = page_width` 时，`document(...)` 返回 `Err(ColumnWidths { .. })`，且 `RecordingCanvas` 为空（复用现有 `assert_fails_with_empty_canvas`）。这锁住第 5 节的错误路径。

## 7. 不在本刀

带内自由 path / 图、第三槽、居中槽、`Band` 变成 `Flow`、给 `Section` 加默认 inset、改 `measure_band` / `decorate.rs`。
