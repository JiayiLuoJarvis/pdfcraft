# 模块依赖方向（已确认）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-10 |
| 状态 | **方向已确认，组织工作项已落地。** 硬规则见 [`AGENTS.md`](../AGENTS.md)；本文是目标 DAG、拆环工作项顺序与落地记录 |
| 相关 | [`code-quality-assessment.md`](code-quality-assessment.md) §7 |

## 硬规则

1. **禁止循环依赖**（crate 间与 `src/` 模块间同一标准）。边只许沿 DAG 向下。
2. **单 crate 能编译出环，也不接受。** Rust 允许同 crate 模块互引用；本仓库仍按 DAG 约束，发现环就拆。
3. **测试代码不反转生产依赖。** 若 A 的单元测试需要 B，而生产上是 B→A，把测试挪到 B、`layout/test_util`，或集成测试，而不是让 A 的非 test 模块 `use` B。

## 目标 crate 级 DAG

箭头表示「可以使用 / 依赖」。

```text
geom
error          → geom（若需要）；不依赖 fonts / layout / spec / backend
fonts          → error, geom
canvas         → error, geom
text           → fonts, error, geom
layout         → text, fonts, canvas, error, geom
spec           → layout, canvas, fonts, error, geom
backend        → canvas, fonts, error, geom, krilla
lib / 薄 glue  → spec, backend（`layout_spec` 与 `to_pdf`，不引入新业务环）
```

`spec` 是公共文档面。`layout_spec` 直接构造 layout 节点。没有 `document` 模块。

**禁止：**

- `spec` → `backend`（含 `ImageTable`、`KrillaCanvas`）
- `layout` → `spec` 或 `backend`
- `backend` → `layout` 或 `spec`
- `error` → `fonts`（`FontId` 由 fonts 拥有；错误里用 `u32` 或只在 Display 层格式化）
- 任何模块 → krilla，除 `backend/`

**图片注册缝。** 注册抽象在 `canvas`，即 `ImageRegistry`、`DecodedImage`、`ImageId`。krilla 解码表和 `to_pdf` 在 `backend`。没有 `backend-krilla` 时用 `RejectingImageRegistry`。`lib.rs` 的 `image_registry` 在打开 `backend-krilla` 时构造 `ImageTable`，否则构造 `RejectingImageRegistry`。`spec` 只调用这个函数，源码不引用 `backend`。

## 目标 layout 内部 DAG

今日环：`flow` 持有 `Frame`/`Split` 且枚举引用各节点，各节点又 `use flow` → **环**。目标拆成：

```text
layout/style
layout/frame          → Frame, Split, Fragment, LayoutCtx（无具体节点类型）
layout/nesting        → 原 lower：check_content / CellBody / ItemBody
                        （可依赖 FlowNode 若枚举上提；或只依赖节点 kind）
layout/paragraph ─┐
layout/columns   ─┤
layout/table     ─┼→ frame, style, nesting, text/fonts/canvas…
layout/list      ─┤
layout/image     ─┤
layout/hline     ─┘
layout/flow           → 上述节点 + frame：FlowNode 枚举与分发 split/draw
layout/decorate       → flow, nesting, …
layout/paginate       → decorate, flow, …
```

**方向：** `paginate` → `decorate` → `flow` → 各节点 → `frame`。  
**禁止：** `decorate`（生产代码）→ `paginate`；各节点 → `flow`（节点只依赖 `frame`）。

`lower` **改名为** `nesting`（或 `content_rules`），文档不再写「DSL lowering」。

## 与现状的差（拆环工作项）

| 环 / 偏离 | 处理 | 建议工作项 |
| --- | --- | --- |
| `error` ↔ `fonts` | `Error`/`ErrorKind` 不引用 `FontId` 类型 | 随错误工作项 1 |
| `flow` ↔ 各节点 | 抽出 `layout/frame`；节点只依赖 frame | 组织工作项（可先于或后于 document↔backend） |
| `decorate` 测试 → `paginate` | 测试挪走，生产边只保留 paginate→decorate | 同上组织工作项内 |
| `document` → `backend` | Image 注册缝 + `to_pdf` 进 backend | **独立工作项**（评估 §7-B） |
| `spec` 调用 `document`，并在 `spec` 里构造 `ImageTable` | `document` 已删除。`ImageTable` 的构造在 `lib.rs` 的 `image_registry` | **已落地** |
| `lower` 名实 | 改名 `nesting` | 小工作项，可并组织工作项 |
| 巨石 `document.rs` | 已拆模块 | **已缓解**，仅更新评估表述 |

## 落地记录

全部拆环工作项已落地，生产代码里没有反向边：

| 项 | 落地形态 |
| --- | --- |
| `error` ↔ `fonts` | 错误工作项 1 已解；`ErrorKind::MissingGlyph.font` 是 `u32` |
| `lower` 名实 | 改名 `layout/nesting`；规则矩阵按 `NodeKind` 判断，不依赖 `FlowNode` |
| `flow` ↔ 各节点 | `layout/frame` 持有 `Frame`、`Split<T>`、`Fragment`、`FragmentCommand`、`PlacedFragment`、`LayoutCtx`、`natural_height`、`stacked_height`、`draw_fragment`，以及库内 trait `Flowable`（split / stack / kind / nested）。叶子节点返回 `Split<Self>`；容器（栏、表、列表）对子节点类型泛型 `N: Flowable`。`flow` 持有 `FlowNode` 枚举，把各节点的 tail 映射回 `FlowNode`。静态分发，没有 trait 对象 |
| `decorate` → `paginate` | `SectionSpan` 归 `decorate`；`decorate` 测试的页面夹具挪进 `layout/test_util` |
| 图片注册缝 | `canvas::ImageRegistry`（`Any + Send + Sync`）是注册缝，`canvas::RejectingImageRegistry` 是无后端实现。`backend::ImageTable` 实现它。`backend::write_pdf` 把录制回放成 PDF。`lib.rs` 的 `image_registry` 在开 `backend-krilla` 时用 `ImageTable`，否则用 `RejectingImageRegistry`。`spec` 调用 `image_registry`，不引用 `backend`。`quire_pdf::to_pdf(rendered)` 调 `backend::write_pdf` |

**未清项：** 节点测试上的硬规则 3 已清。`paragraph`、`columns`、`table`、`list`、`image`、`hline` 的生产和测试代码都不引用 `crate::layout::flow`。原 `#[cfg(test)]` 在 `layout/node_tests/`，按节点分文件，可以依赖 `flow`、`paginate` 与 `test_util`。`layout/test_util` 仍使用 `flow` 与 `paginate`。调用方是 `node_tests`，以及 `paginate` 和 `flow` 的测试。

## 明确不做

- 为「无环」把 **`quire-pdf` 内部**再拆成多个 crates（模块 DAG 即可）。产品级旁挂（`quire-forms` / `quire-orgchart`）见 [`workspace-crates-spec.md`](workspace-crates-spec.md)，与消环无关。
- 为消环把 `FlowNode` 改成 trait 对象热路径（除非测量证明必要）。
- 在 AGENTS 里把 `document → backend` 写成合法例外。
