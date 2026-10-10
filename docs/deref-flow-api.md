# `Deref` 到 `Flow`：用途与已确认方向

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-10 |
| 状态 | **已落地。** `FlowWriter` + 公开 `add` + 糖；`Document` / `Section` 不再 `Deref` 到 `Flow`。 |
| 相关 | [`docs/code-quality-assessment.md`](code-quality-assessment.md)；执行规格 [`docs/flow-writer-spec.md`](flow-writer-spec.md) |
| 代码 | `src/document/`（`flow_writer.rs` 等）；正文方法在 `FlowWriter` |

## 已确认

1. **不做** `Deref` / `DerefMut` 到 `Flow` 的伪继承。旧 tradeoff（「看起来像继承，换来方法只写一份」）**不予接受**。
2. 用 **`FlowWriter` trait** 表达「能往流里追加内容」。`Flow`、`Section`、`Document` 都实现它。
3. **元素是独立值**，经公开 **`add`** 入流；`doc.table(|…|)` / `doc.paragraph(|…|)` 等是语法糖（内部等价于 `add(El::build(…))`）。
4. **文件按元素拆模块**（组织）；与去 `Deref`、引入 `add` 同一方向，实现时可分 PR。
5. **日常文档与示例只教糖**；`add` 公开，供辅助函数返回元素再挂上等高级用法。
6. **rejecting 语义保持**：显式 `section()` 成功后，文档顶栏正文入口仍指向拒收 flow，再写糖或 `add` 记 `Error::ContentOutsideSection`。分支放在 `Document` 的 `flow_mut()`（或等价物）里，不依赖 `Deref`。

### 目标形状（示意，非最终签名）

```rust
pub trait FlowWriter {
    fn flow_mut(&mut self) -> &mut Flow;

    fn add(&mut self, item: impl IntoFlowItem /* 名待实现时定 */) {
        // 入流；Document 经 flow_mut 走到隐式节或 rejecting
    }

    fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph)) {
        self.add(Paragraph::build(f));
    }
    fn table(&mut self, f: impl FnOnce(&mut Table)) {
        self.add(Table::build(f));
    }
    // columns / ul / ol / image / hline / spacer /
    // page_break / keep_together / stack — 同理，默认方法
}

impl FlowWriter for Flow { /* flow_mut → self */ }
impl FlowWriter for Section<'_> { /* → self.flow */ }
impl FlowWriter for Document<'_> {
    fn flow_mut(&mut self) -> &mut Flow {
        // Implicit → 该节 flow；Explicit → rejecting flow
    }
}
```

调用方日常：

```rust
use quire::FlowWriter; // 若方法解析需要

doc.paragraph(|p| p.text("..."));
doc.table(|t| { /* ... */ });
```

高级：

```rust
fn risk_table(data: &Risk) -> Table { Table::build(|t| { /* ... */ }) }

doc.add(risk_table(&r));
```

### 实现时约定

| 项 | 约定 |
| --- | --- |
| trait 名 | `FlowWriter` |
| `add` | **公开**；rustdoc 说明用途，主路径示例不主推 |
| `flow_mut` | 为 trait 所需；文档不把它当日常 API（实现时可再收紧可见性/sealed） |
| `IntoFlowItem` 等名 | 实现 PR 再定；勿提前冻进提案书 |
| rejecting | 同一套糖 / `add`，换底层 flow；不改为类型级禁止 |
| 教法 | README / 示例只教糖；`use FlowWriter` 写进主示例如有需要 |
| Cell 入口与 `add` 对齐 | **下一刀**，不与去 `Deref` 缠同一 PR |

迁移已完成。下文「现在用来干什么」保留为迁移前说明，便于对照。

---

## 现在用来干什么（迁移前说明）

不是为了让 `Document`「变成」`Flow`。是为了让调用方在文档和节上直接写正文方法，而不用把 `Flow` 的 API 抄两遍。

模块文档曾写明：

> Document settings and page-time closures. Body methods come from `Flow`.

现为：

> Body methods come from `FlowWriter`.

### 转发了哪些方法

这些方法定义在 `Flow` 上，不在 `Document` / `Section` 的固有 `impl` 里。

- `paragraph`
- `columns`
- `table`
- `ul` / `ol`
- `image`
- `hline`
- `spacer`
- `page_break`
- `keep_together`
- `stack`

因此可以写：

```rust
doc.paragraph(|p| p.text("..."));
doc.table(|t| { /* ... */ });
section.paragraph(|p| p.text("A"));
```

方法解析时，固有方法优先。`doc.font`、`doc.margins`、`doc.header`、`doc.section` 仍是 `Document` 自己的。只有正文流方法才走 `Deref`。

### `Section` 与 `Document` 不一样

`Section` 很直。`Deref` / `DerefMut` 指向自己的 `flow`。

`Document` 多一层开关。

- 还没调用过 `section()` 时，指到隐式那一节的 `flow`。`doc.paragraph` 往默认正文追加。
- 一旦成功进过显式 `section()`，`DerefMut` 改指到 `Flow::rejecting(...)`。之后再 `doc.paragraph` 不会写入某一节，而是记 `Error::ContentOutsideSection`。

所以它同时做两件事。

1. 共享正文 API（省重复）。
2. 用同一套调用入口，表达「显式分节后禁止再在文档上写正文」。

第 2 点在换掉 `Deref` 之后保留（见「已确认」§6）。

### 和 Rust 惯用法差在哪

Rust API Guidelines（C-DEREF）把 `Deref` / `DerefMut` 留给智能指针语义（`Box`、`String`、`&T` 一类）。  
这里借用它做 API 转发，让 `Document` / `Section` 看起来「继承」了 `Flow` 的方法。

库内自洽。调用体验也清楚。偏离的是惯例，不是功能坏了。  
**已确认：生产前收掉，不用「0.x 先记一笔」搪塞。**

---

## 讨论过的替代（归档）

目标：共享正文方法；保留 Implicit / rejecting；尽量少伤 `doc.paragraph` 手感。

| 方案 | 结论 |
| --- | --- |
| A. Trait | **采用**，名定为 `FlowWriter`（讨论稿里曾写 `FlowExt`） |
| B. `body()` | 不采用为主路径；多一层，示例面改动大 |
| C. 宏/手写固有转发 | 不采用；消掉 `Deref` 却仍是两套表面方法表 |
| 继续 `Deref` 当继承 | **不接受** |
| `enum Body` 只为挂方法 | 不推荐，换皮 |

另定（讨论中超出原 A 的部分）：元素独立值 + 公开 `add` + 糖；文件按元素拆；日常只教糖。
