# 刀 10 合成设计 — section（C2 底 + C1 嫁接）

基线：`feat/section` @ `a12b610`。权威：提案书 §2.10 / §2.11 / §2.12 / §7。

## Synthesis decision

**Base: C2**（`SectionJob` → `paginate_sections` → 平坦 `PagePlan` 流 + 平行 `SectionSpan`）。与 [cross-judge](cross-judge.md) 一致（27 vs 26）。选 C2 因为：`plan_pages` / `PagePlan` 字段不动；节身份只在 span；纸张入口只收 `PageSize`，规格与最终页框不混成 `From<Size>→Custom`。

**Grafts from C1:**

1. **`Paper` 命名 + `#[non_exhaustive]`**（不用 `StandardSize`；总表名单可再钉）。
2. **不为断言新增公共 `Size` 常量。** 保留 `A4` / `LETTER`；`PageSize::A4.resolve()` 与 `A4` 做 `to_bits` 钉。
3. **C1 集成测试清单。** 两节 2+1 页页脚矩阵；第二节左边距 → 页眉左槽 x；空节 1 页+装饰；节末单 break 不多页；混写 → 错误且画布空。
4. **糖方法转调隐式节，不复制逻辑。**
5. **混写硬错误，不要「占用过的隐式节变成空白第 0 节」。** 若 lead 已写正文/边距/钩子再 `section`，或已 `section` 再写 `doc.*` 正文/边距/钩子 → `ContentOutsideSection`。`font` / `load_image` 不算写过 lead。pristine lead + 显式 section → 丢掉 lead，不多前言页。

**Rejected from C1:** `PagePlan.section`；`chunk_by` 当唯一节边界；`impl Into<PageSize>` / `From<Size>`。

**Rejected from C2:** 占用过的隐式节自动占一页空白前言；为断言导出 `A0`…`LEGAL` 的 `Size` 常量；`StandardSize` 名。

**Defaults locked:**

- 入口：`document(page_size: PageSize, …)`。测试把 `document(A4, …)` 改成 `document(PageSize::A4, …)`；页尺寸断言仍比 `geom::A4`。
- `PageSize::{Paper(Paper, Orientation), Custom(Size)}`；关联常量 `PageSize::A4` 等 = 竖向。横向：`PageSize::Paper(Paper::A4, Orientation::Landscape)` 或 `Paper::A4.landscape()`。
- `is_last` = 全文末页；节末用 `section_number == section_count`。
- 新节不继承上一节边距/钩子，只继承文档默认纸张。
- 刀 8 槽内缩不改。空 section 仍 1 页。节内 `page_break` 游标语义不变。

## Named data shape

| 结构 | 角色 |
| --- | --- |
| `Paper` | `A0`…`A6` `Letter` `Legal`，`#[non_exhaustive]` |
| `Orientation` | `Portrait` \| `Landscape` |
| `PageSize` | `Paper(Paper, Orientation)` \| `Custom(Size)`；`resolve() -> Size` |
| `Section` | 公共构建器：可选纸张、margins、四钩子、`Deref → Flow` |
| `SectionJob` | `{ config, decorations, root }` |
| `SectionSpan` | `{ start, len: NonZeroU32, margins, decorations }` |
| `PagePlan` | 仍 `{ size, placed }` |
| `PageInfo` | 加真值 `section_number` / `section_count` |
| `Error::ContentOutsideSection` | 隐式/显式混写；画布仍空 |

## Usage

```rust
document(PageSize::A4, |doc| {
    doc.font(font)?;
    doc.margins(m);
    doc.header(|h| { /* 单节糖 */ });
    doc.paragraph(|p| p.text("…"));
    Ok(())
})?;

document(PageSize::A4, |doc| {
    doc.font(font)?;
    doc.section(|s| {
        s.margins(m);
        s.footer(|f| {
            let p = f.page();
            f.left(format!("{}/{}", p.number, p.count));
            f.right(format!("{}/{}", p.section_number, p.section_count));
        });
        s.paragraph(|p| p.text("第一篇"));
        Ok(())
    })?;
    doc.section(|s| {
        s.page_size(PageSize::Paper(Paper::A4, Orientation::Landscape));
        s.paragraph(|p| p.text("附表"));
        Ok(())
    })?;
    Ok(())
})?;
```

`section` 闭包返回 `Result<()>`（与 `document` 一致）。

## Shape

- `PageSize::resolve` 在 document lower 边界完成。layout 只见 `Size`。
- `paginate_sections(jobs)`：逐节 `plan_pages` → 追加页并记 span → `measure` 按 span 填两套页码并选用该节 decorations/margins → `paint_pages` 不变。
- `paginate_root` = 单 job 包装。
- `From<&PageInfo> for Page` 抄真值。单节时相等是因为唯一 span 盖住全文。

## Tradeoffs accepted

- 现有测试改 `document(PageSize::A4, …)`，换规格与 Custom 永不混变体。
- 混写运行时报错，不是类型状态。
- 不导出 paginator / span / job。

## Open questions

- A 系列以 A0–A6 为实现期钉。
- `is_last` 保持全文。

## Next implementation step

geom → decorate/paginate span → DSL → 集成测试 → 七门禁。
