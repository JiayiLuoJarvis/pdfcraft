# 刀 12 交叉评审：流内分隔线

Tip：`feat/hline` @ `7cb1654`。依据：`docs/提案书.md` §2.3（270-285）、`how.md`、两份候选的 design/rationale。已对照源码核实：`document(page_size, build) -> Result<Rendered>`，crate 名 `quire-pdf`，`Cell` / `CellContent` 当前派生 `Debug, Clone, PartialEq, Eq`，`check_content` 中 `Image` 仅 Column + TableCell。

## 结论

**底稿选候选 1**（`hline.rs` + `HLine` 闭包构建器 + `Flow::hline` / `Cell::hline`）。候选 1 **28/30**，候选 2 **21/30**。

## 评分

| # | 维度 | 候选 1 | 候选 2 |
| --- | --- | --- | --- |
| 1 | §2.3 忠实度 | 5 | 5 |
| 2 | 公共面深度 | 4 | 4 |
| 3 | 契合既有模式 | 5 | 3 |
| 4 | 最小 diff / 克制 | 5 | 2 |
| 5 | 类型不变量 | 5 | 4 |
| 6 | 后续维护者照抄不踩坑 | 4 | 3 |
| | **合计** | **28** | **21** |

### 1. §2.3 忠实度（5 / 5）

两份都做到了：默认满宽 `W`（`None` 在 split 时取 `avail.width`），定长时默认 Center 并可改 Start/End，高度为 `space_before + thickness + space_after`，`spacing_after = 0`（页底空白不会被吞），不可拆（宽、整页高、剩余高三档判定），以及 Fragment 内一条水平 `Stroke`，线画在中线。两份都明确不走 Layer、填空或 canvas。这一项分不出高下。

### 2. 公共面深度（4 / 4）

- 候选 1：一个类型、七个 setter、`Flow::hline`、`Cell::hline`。列表项和栏通过嵌套 `Flow` 直接可用，不加 API。扣分点是 `Cell::hline` 只能造“整格只有一条线”的格子，没法在格内把段落和线叠在一起。不过 §2.3 没有要求格内组合，§2.6 也没写，所以这个缺口可以接受。
- 候选 2：`From<Pen>` 让最常见的写法很短；`Cell::flow` 让格内可以组合任意流块，覆盖面更广。扣分点是同一件事有两种入口（`Rule` 和 `Pen`），并且 `Cell::flow` 实际上是一个顺手做出来的通用格内 stack。

### 3. 契合既有模式（5 / 3）

- 候选 1 和 `ImageNode` / `ImageBox` 一一对应：独立文件、`Default` 加 `&mut self` setter、私有 `lower`、`CellContent` 新增一个枚举臂、`check_content` 把 HLine 并进 Spacer 那一支。唯一的结构差别（宽度延迟到 split 才解析）有正当理由。
- 候选 2 的链式值构建器与 `ImageBox` 的闭包写法不同，DSL 里因此出现两套习惯用法。`CellContent::Flow(Box<dyn FnOnce>)` 也打破了 `Cell` 现有的值语义。复用 `Pen` 与 `Layer::line` 用词一致，这一点算加分。

### 4. 最小 diff / 克制（5 / 2）

- 候选 1 只做 §2.3，并且明确不做通用格内 stack，与 `how.md` 的 Gotchas 一致。
- 候选 2 做了 `how.md` 点名不做的通用格内 stack，并因此改动了一个已经公开的类型：`Cell` 失去 `Clone` / `PartialEq` / `Eq`，改为手写 `Debug`，闭包还要求 `'static`。这是对现有公共 API 的破坏性变更，与分隔线本身无关，违反 AGENTS.md 的“不把未确认功能顺便做进当前 PR”。把节点放进 `flow.rs` 少一个文件，这点节省抵不上上面的代价。

### 5. 类型不变量（5 / 4）

两份都用 `Positive` 表示线粗和定长、用 `NonNegative` 表示空白，`lower` 都是全函数，失败都发生在 `plan_pages` 内、早于 `start_page`，并且都有“`rec` 为空”的测试。区别在于高度溢出：候选 1 让 `+inf > full_height` 自然落到 `UnsplittableTallerThanFrame`，不新增失败模式；候选 2 多出一支 `!is_finite → InvalidGeometry`，对同一种“太高”的输入给出另一种错误，语义不一致，而且这支检查本身多余。

### 6. 后续维护者照抄不踩坑（4 / 3）

- 候选 1 的好处：照抄 image 的人自然会落到 `hline.rs`；故意不提供 `center()`，抄 `ImageBox` 的人写出 `h.center()` 会直接编译失败，而不是悄悄写出冗余调用。扣分点是 README 示例和仓库不一致：写成了 `use quire::…` 和 `document(|doc| …)`，实际应为 `quire_pdf` 和 `document(PageSize::A4, |doc| { …; Ok(()) })`。照抄这段示例会编译不过。
- 候选 2 的示例与真实签名一致，这点比候选 1 好。但 `Cell::flow` 的 `'static` 闭包、`Cell` 去掉 `Eq` 的连锁影响、两种构建器写法并存，会让下一个在格内加内容的人不确定该沿用哪套模式。

## 合成时从候选 2 嫁接

1. 照候选 2 修正候选 1 的示例：crate 名改为 `quire_pdf`，`document` 带上 `PageSize` 并返回 `Ok(())`。
2. 采用候选 2 写成具体代码的 `split` 主体（`Point::try_pt` 两端点、`Paint::solid`、`Stroke::new`），其中去掉 `!is_finite → InvalidGeometry` 这一支，保留候选 1 让溢出落到 `UnsplittableTallerThanFrame` 的做法。
3. 采用候选 2 的测试表（框宽 200 时的默认线、定长 100 的三种对齐、`4 + 2 + 6 = 12` 时 `y = 5`、宽先于高、`MoveToNextPage` 后节点字段不变、永不返回 `Partial`），与候选 1 的 `overflowing_spaces_are_taller_than_frame` 和栏宽用例合并。
4. 在文档注释里互相指一下 `Flow::hline` 与 `Layer::line`，说明一个是流内线、一个是页面固定坐标线。

## 不采纳

- `Cell::flow` / `CellContent::Flow`：等将来提案明确要求格内组合时，单独开一刀做，并在那一刀里决定 `Cell` 是否保留 `Eq`。
- `From<Pen> for Rule` 和链式值构建器：会让 DSL 出现第二种构建器写法。
- 公共名用 `rule`：`how.md` 和分支名都倾向 `hline`，提案两者都允许，这里沿用 `hline`。

## 仍需提案裁决（两份共有）

- 默认 1 pt、黑色是实现自定的值。是否在 §2.3 表里补一行，或者改为默认跟随文字色？
- 格内的线吃的是内容宽（扣掉 padding 后），签字栏场景是否接受？
