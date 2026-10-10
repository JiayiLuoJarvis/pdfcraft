# 刀 10 实现状态

**Branch:** `feat/section` @ `9eda8da`  
**Base tip before work:** `a12b610`  
**Synthesis:** C2 底 + C1 嫁接（见 `arena/synthesis/design.md`；cross-judge 27–26 推 C2）

## 调用方得到什么

- `document(PageSize, …)`。竖向快捷常量 `PageSize::A4` 等；横向 `Paper::A4.landscape()` 或 `PageSize::Paper(_, Landscape)`；自定义 `PageSize::custom(Size)`。
- 单节：`doc.margins` / `header` / `footer` / `behind` / `front` 与正文写在隐式节上。`section_*` 等于全文页码（唯一 span 盖住全文）。
- 多节：`doc.section(|s| { …; Ok(()) })` 强制新物理页；节可覆盖纸张与边距、钩子；两套页码分页后填入。
- 混写 → `Error::ContentOutsideSection`（画布仍空）。`font` / `load_image` 不算写过 lead。

## 改过的文件

- `src/geom.rs` — `Paper` / `Orientation` / `PageSize` / `resolve` / 关联常量；无 `From<Size>`
- `src/error.rs` — `ContentOutsideSection`
- `src/layout/decorate.rs` — `PageInfo` 节计数；`measure_bands` 按 `SectionSpan`
- `src/layout/paginate.rs` — `SectionJob` / `SectionSpan` / `paginate_sections`；`PagePlan` 仍 `{ size, placed }`；`paginate_root` 单 job 包装
- `src/document.rs` — `Section`、隐式/显式节、糖转调、`Page::from` 抄真值
- `src/lib.rs` — 重导出
- `tests/document.rs` — 入口改为 `PageSize::…`；新集成测试

## 与合成的差别（实现选择）

- `SectionJob` / `SectionSpan` 持有 `&Decorations`（`Box<dyn Fn>` 不可克隆）
- 正文状态用 `Sections::Implicit | Explicit` 枚举
- 父会话在实现后去掉 `impl Into<PageSize>` / `From<Size>`，对齐 cross-judge

## 验证（父会话 2026-10-09 重跑，退出码 0）

七道门禁全绿。`--features backend-krilla`：144 lib + 1 ignored，30 integration。

## 提交

`9eda8da`（仅 `src/` + `tests/`；`.audit/` 仍未跟踪）。无 remote/PR。
