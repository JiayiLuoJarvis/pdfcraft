# 模块依赖方向

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-14 |
| 状态 | 工作项 G 已落地。本文是 `quire-pdf` 的目标图。完整边表在 [`scripts/quire-pdf-module-edges.txt`](../scripts/quire-pdf-module-edges.txt)，CI 用它核对源码 |
| 相关 | [`AGENTS.md`](../AGENTS.md)；[`enterprise-readiness-spec.md`](enterprise-readiness-spec.md) 5.5 与 5.6 |

## 硬规则

1. **禁止循环依赖**（crate 间与 `src/` 模块间同一标准）。边只许沿 DAG 向下。
2. **单 crate 能编译出环，也不接受。** Rust 允许同 crate 模块互引用。本仓库仍按 DAG 约束，发现环就拆。
3. **测试代码不反转生产依赖。** 生产模块 A 的测试如果要用生产上位于上游的 B，把测试放到 `#[cfg(test)]` 的旁路模块、`layout/node_tests`，或集成测试。A 的生产代码不 `use` B。

## 检查

在仓库根目录运行：

```bash
python3 scripts/check_quire_pdf_modules.py
```

退出码 0 时，脚本打印边数，并打印两条自检。退出码 1 时，它列出树里多出来的边、边文件里树没有的边，或一条环。

改完真实的 `use` 之后，增删 [`scripts/quire-pdf-module-edges.txt`](../scripts/quire-pdf-module-edges.txt) 里对应的一行。下面的命令把树里扫到的边打到标准输出，用来和边文件对差：

```bash
python3 scripts/check_quire_pdf_modules.py --print
```

脚本每次运行都做两件自检。

- 生产图里不得出现 `layout/decorate` → `layout/paginate`。
- 把这条边加进图的副本之后必须成环，因为图里已经有 `layout/paginate` → `layout/decorate`。

要确认多出来的边会失败，在 `crates/quire-pdf/src/layout/decorate.rs` 加上 `use crate::layout::paginate::PaginateConfig;`，再跑脚本。确认之后删掉这一行。

## 节点

脚本只扫 `crates/quire-pdf/src` 里的生产代码。

| 路径 | 节点 |
| --- | --- |
| `src/lib.rs` | `lib` |
| `src/<name>.rs` | `<name>`（`error`、`geom`、`limits`、`canvas`、`fonts`） |
| `src/layout/<name>.rs` | `layout/<name>` |
| `src/text/` 下全部文件 | `text` |
| `src/spec/` 下全部文件 | `spec` |
| `src/backend/` 下全部文件 | `backend` |

父模块上的 `#[cfg(test)] mod …`，以及文件内部的 `#[cfg(test)]` 项，不计入生产边。因此 `layout/decorate_tests.rs`、`layout/node_tests/`、`layout/test_util.rs`、`spec/tests.rs` 都不在图里。

`text`、`spec`、`backend` 各是一个节点。节点内部的文件互相引用，不记成边。

## 目标图

箭头表示「可以使用」。外部 crate（`krilla` 只在 `backend`）不进边文件。

```text
geom        → error
error                          # LimitKind 定义在此
limits      → error
fonts       → error, limits
canvas      → error, geom, fonts, limits
text        → error, fonts, geom
layout      → text, fonts, canvas, error, geom, limits
spec        → layout 子模块, canvas, fonts, error, geom, limits
backend     → canvas, fonts, error, geom, limits
lib         → spec, backend, canvas, fonts, error, geom, limits
```

`layout` 这一行是子模块出边的汇总。子模块之间的边如下。

```text
layout/style         → error, fonts, geom
layout/frame         → canvas, error, fonts, geom
layout/nesting       → error, layout/frame
layout/column_width  → error, geom
layout/line_run      → canvas, error, geom, layout/frame, layout/style, text
layout/paragraph     → canvas, error, fonts, geom, layout/frame, layout/line_run, layout/style, text
layout/columns       → error, geom, layout/column_width, layout/frame, layout/nesting
layout/table         → error, geom, layout/column_width, layout/frame, layout/nesting
layout/list          → error, geom, layout/frame, layout/line_run, layout/nesting, layout/style
layout/image         → canvas, error, geom, layout/frame, layout/style
layout/hline         → error, geom, layout/frame, layout/style
layout/flow          → error, geom, limits, layout/columns, layout/frame, layout/hline, layout/image, layout/list, layout/paragraph, layout/table
layout/decorate      → canvas, error, geom, layout/flow, layout/frame, layout/line_run, layout/nesting, layout/style
layout/paginate      → canvas, error, geom, limits, layout/decorate, layout/flow, layout/frame
```

`spec` 使用的 layout 子模块有 `column_width`、`columns`、`decorate`、`flow`、`frame`、`hline`、`image`、`list`、`paginate`、`paragraph`、`style`、`table`。

方向是 `paginate` → `decorate` → `flow` → 各叶子节点 → `frame`。叶子节点不使用 `flow`。`decorate` 的生产代码不使用 `paginate`。

**禁止：**

- `spec` → `backend`
- `layout` → `spec` 或 `backend`
- `backend` → `layout` 或 `spec`
- `error` → `limits`、`geom`、`fonts`、`layout`、`spec`、`backend`
- `layout/decorate` → `layout/paginate`
- `layout/table` → `layout/columns`
- `layout/list` 或 `layout/decorate` → `layout/paragraph`
- 任何模块 → `krilla`，除 `backend`

**图片注册缝。** 注册抽象在 `canvas`，即 `ImageRegistry`、`DecodedImage`、`ImageId`。krilla 解码表和 `to_pdf` 在 `backend`。没有 `backend-krilla` 时用 `RejectingImageRegistry`。`lib.rs` 的 `image_registry` 在打开 `backend-krilla` 时构造 `ImageTable`，否则构造 `RejectingImageRegistry`。`spec` 只调用这个函数，源码不引用 `backend`。

## 共享模块

`layout/column_width` 放 `ColumnWidth`、`Ratio` 和 `resolve_column_widths`。`columns` 与 `table` 都使用它。`table` 不使用 `columns`。

`layout/line_run` 放 `single_line_run`，以及段落也要的 `align_x` 和 `line_metrics_or_soft`。`list`、`decorate`、`paragraph` 使用它。`list` 和 `decorate` 不使用 `paragraph`。

两条都是普通函数，没有 trait 对象。

## 落地记录

2026-10-14，工作项 G。

| 项 | 落地形态 |
| --- | --- |
| `LimitKind` | 定义在 `error`，与 `ErrorKind::LimitExceeded` 相邻。`limits` 与 crate 根重新导出。`limits` → `error`。`error` 不引用 `limits` |
| `geom` → `error` | 承认这条边。`geom` 构造长度时返回 `Error` |
| `fonts` / `canvas` → `limits` | 按树写入。`layout/flow`、`layout/paginate`、`spec`、`backend` 同样使用 `limits` |
| `decorate` 测试 | 整段移到 `layout/decorate_tests.rs`，由 `layout/mod.rs` 的 `#[cfg(test)]` 挂上。测试经 `test_util` 调用 `paginate_root`。生产 `decorate` 没有这条边 |
| `list` 与 `paragraph`、`table` 与 `columns` | 抽到 `layout/line_run` 与 `layout/column_width`。生产图里没有这两对兄弟边 |
| 环检查 | `scripts/check_quire_pdf_modules.py` 读边文件，扫生产 `use` 与 `crate::` 路径。`.github/workflows/ci.yml` 在装 Rust 之前跑它 |
| 旧结案语 | 删去「全部拆环工作项已落地，生产代码里没有反向边」 |

`backend` 在这张图里是一个节点。`backend/emit.rs` 使用 `krilla_canvas` 里的页计划类型，`krilla_canvas.rs` 调用 `emit::render`。两份文件同属 `backend`，检查器不把它们拆开。拆成两个节点不在工作项 G 的边表里。

先前已经落地、这次图仍然承认的部分：`ErrorKind::MissingGlyph.font` 是 `u32`。`layout/nesting` 按 `NodeKind` 判断嵌套。`layout/frame` 持有 `Frame`、`Split`、`Fragment` 和库内 `Flowable`。`flow` 持有 `FlowNode`。静态分发，没有 trait 对象。`canvas::ImageRegistry` 是图片注册缝。`spec` 不引用 `backend`。

## 明确不做

- 为「无环」把 `quire-pdf` 再拆成多个 crate。
- 为消环把 `FlowNode` 改成 trait 对象热路径。
- 在 AGENTS 里把 `document` → `backend` 写成合法例外。`document` 模块已删除。
