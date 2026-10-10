# Knife 17 取舍说明

## 选择摘要

选择两条最短的既有管线：

- 富段落：`Cell::paragraph` 保存完整 `Paragraph`，与 `Flow::paragraph` 共用
  `Paragraph -> FlowNode::Text` lower。
- 列表：`Cell::ul/ol` 保存作用域 `FnOnce`，到 `Table::lower(&Flow)` 才建立
  `List { parent: flow.child(), ... }` 并执行。

布局层已经支持 table cell 中的 `Text` 和 `List`，所以不新增 layout node、不改
allowlist、不改 `natural_height`。因列表 seam 会传播 lifetime 并移除 `Cell` 的
若干 trait，实现拆为 17a/17b。

全部选择都在 frame 的 autonomy defaults 内；没有启用 reverse word。

## 为什么这样选

### 提案忠实度

§2.6 允许格内段落（含填空）并随对应能力开放列表，同时明确禁止 columns、
pageBreak、keep-together 和 nested table。§2.8 又规定列表项可含段落、线、
spacer、嵌套列表，并明确允许格内列表。

现有 `CellBody::new` 与 `ListItem::new` 已分别编码这两层矩阵。让新公共入口继续走
这两个构造点，比在 `document.rs` 复制一份规则更可靠。

### 调用面与正文一致

调用方已经认识 `Paragraph`、`List`、`Flow::paragraph`、`Flow::ul/ol`。
`Cell::paragraph/ul/ol` 使表格行里的代码与正文同词汇，并能与
`Cell::hline/spacer/image` 并排。没有必要再公开 `CellParagraph`、`CellList` 或
只能表达部分 inline 能力的 builder。

`Cell::new(str)` 保留纯文本快捷路径，避免普通表格为富文本能力支付额外语义。

### 富段落必须延迟断行，而不是延迟 builder

`Paragraph` builder 不依赖 `Flow`，可以立即执行并保存 owned inline items；
真正依赖上下文的是 style 和 cell 内宽。共享 lower 在 table lower 时继承 style，
输出未断行的 `TextNode`；table 测量再以 `inner_w` 完成换行。

这样 bold face、fixed/em/fill blank、首行缩进和 URI link 都自然走同一管线。
若另建 cell rich-text builder，错误规则、coalescing 和 blank/link 行为会出现第二
份实现。

### 列表必须延迟 builder

现有 `List::item` 会立即调用 `parent.nested(f)`；没有 parent `Flow` 就无法正确继承
style、`DocToken`，也不能复用 item allowlist。独立 `Cell::ul` 构造时没有这个
上下文，而 `Table::lower(&Flow)` 恰好有。

因此保存 `FnOnce(&mut List)` 并在 lower 时注入 `flow.child()` 是最小的语义桥。
它也保证列表项中的非法节点仍首先由 `ListItem::new` 报
`container: "list item"`，而不是到了外层 cell 才得到错误容器名。

### fit-only 测量保持诚实

正文段落和列表可以产生 `Partial`，但 table 必须先取得整格自然高，再按整行或
rowspan group 分页。现有 `natural_height` 把 cell 内的 `Partial` 和
`MoveToNextPage` 都转成 `UnsplittableTallerThanFrame`。

保留这一点避免引入 cell tail、跨页边框、重复表头和 rowspan 重算。完整行只是在
当前页放不下时整体移到下一页；过高 cell 不会伪装成可续页内容。

## 为什么拆 17a / 17b

这是按 frame 的条件默认拆分，不是选择 `force-both`。

实际 seam 显示两半规模不同：

| 项目 | 17a paragraph | 17b list |
| --- | --- | --- |
| builder 是否需 Flow | 否 | 是 |
| 存储 | owned `Paragraph` | scoped trait object |
| 公共 lifetime 传播 | 无 | `Cell<'a>`、`Table<'a>`、row/header bounds |
| `Cell` traits | 仅 `Eq` 不再诚实 | closure 使 Clone/PartialEq/Eq 均不诚实 |
| 错误来源 | paragraph 自身 | deferred item build + list lower |
| 编译夹具 | 普通 API | borrowed capture、From 推断、混合 cell 行 |

17a 是一个可独立验证的窄桥；17b 是公共所有权模型变更。把两者强塞进同一评审，
会让布局行为与 Rust API 兼容性问题互相遮蔽，也超过“约一晚”的低风险范围。

## 被拒绝的方案

### `items-only`

拒绝该 reverse word。只给 cell 暴露 text item 数组或残缺 inline API 会绕过
`Paragraph` 的 coalescing、fill blank sealing、link target 和首错保存，形成第二套
富文本语义。

### `rich-new`

拒绝该 reverse word。让 `Cell::new(str)` 偷换成富段落不会给纯字符串增加能力，却
会改变已有快捷路径和 trait/错误特征。富能力应由明确的 `Cell::paragraph` opt in。

### `text-only`

拒绝该 reverse word。既然 `Paragraph -> TextNode::from_items` 已支持 bold、blank
和 link，主动截断为纯 text 既违反 §2.6 填空要求，也制造正文/cell 差异。URI link
不访问网络，不需要 cell 特例。

### `list-closure-on-row`

拒绝该 reverse word。`t.row(|row| row.ul(...))` 可让 `Table` 持有 child Flow 并
立即执行，但会增加另一种 row DSL，使 list cell 不能像 `Cell::hline` 一样作为值
参与数组、跨度和条件组装。

### `table-child-flow`

拒绝该 reverse word。仅给 `Table` 增加 parent Flow 仍不能让上下文无关的
`Cell::ul(...)` 在传入 row 前执行；最终还得改成 table/row 专用 cell 工厂。它扩大
公共调用面，却没有消除核心上下文问题。

### 给 `Cell::ul` 伪造默认 Flow

拒绝。当前 style 看似接近默认值不代表所有权边界正确；伪造 Flow 会丢失所在文档的
style 和 `DocToken`，未来样式继承一落地就产生静默差异。图片 handle 等检查也不能
依赖偶然相同的默认上下文。

### 要求 deferred closure 为 `'static`

拒绝。它能保留无 lifetime 参数的 `Cell`，但普通 DSL 往往借用合同数据、行模型或
字符串切片。强迫调用方 clone 后 `move` 只是在 API 边界掩盖实现限制。
`Flow::table` 在回调后立即 lower，作用域 lifetime 能准确表达真实存活期。

### 把 `FnOnce` 改成 `Fn`，再用 `Arc` 保住 Clone

拒绝。现有 `Flow::ul/ol` 接受 `FnOnce`；改窄 closure trait 就不再是镜像 API。
共享 one-shot closure 的 Cell clone 也会出现“哪个 clone 先消费”的不确定语义。
诚实移除 trait 比伪 Clone 更安全。

### 预先 lower 成 `FlowNode::List`

拒绝。构造 `List` 所需的 Flow 正是缺失上下文；预先 lower 只能通过伪 Flow、额外
参数或第二套 declarative item tree 达成。前两者错误，后者明显更大。

### 新建 owned declarative cell/list tree

拒绝。它可以避免 closure lifetime，却会复制 `Flow`/`List` 的 item 组装 API，
并顺手打开多 payload cell 的产品决策。当前目标是桥接已有能力，不是设计第二棵
文档树。

### `stack-cell`

拒绝该 reverse word。布局内部的 `CellBody(Vec<FlowNode>)` 不等于公共 API 已决定
支持混合孩子。段落或列表各自作为唯一 payload 已足够完成本刀，且符合 frame 明确
范围。

### `split-in-cell`

拒绝该 reverse word。让 `natural_height` 接受 Partial 不是局部改动；table 需要
保存每格 tail、同步跨页行高、边框、rowspan 和重复表头。它还冲突于 §2.6 的整物理
行分页单位。

### `force-both`

拒绝该 reverse word。段落桥很小，但列表 seam 有可测的 lifetime 与 trait 影响。
先交付 17a、再用专门的编译和渲染测试审 17b，风险更低且不改变最终 API 方向。

## 风险与约束

- `Cell<'a>` 是公开类型形状变化；显式写 `Cell` 类型注解的调用方可能需要补
  lifetime。17b 必须用 compile fixtures 验证常见推断路径。
- deferred list error 晚于 `Cell::ul` 调用，但仍早于分页/绘制；文档组装保持原子。
- `Cell` 失去 Clone/比较 traits 是实际兼容性成本。仓库当前没有依赖这些 traits，
  但 0.x 也不应把它当成无关实现细节。
- paragraph/list 的过高测试必须使用 full inner frame，不应把“当前页剩余不足、
  整行移页”误断言成过高。
- marker column 固定 18 pt；过窄 cell 的 `InvalidGeometry` 不是 allowlist 错误。
- 正向断言必须看 RecordingCanvas 的 font id、stroke/link rectangle 与 marker x，
  不能只看文本存在或页数。
- 17a 不提前引入 17b lifetime scaffolding；17b 堆叠后统一更新公开 rustdoc。
