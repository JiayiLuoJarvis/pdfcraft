# Knife 17 Arena Cross-Judge

## 结论

推荐以 **candidate 3** 为基底。它在正确复用 `Paragraph` / `List` 的同时，把 list ↔ Flow 桥收敛为 `Table::lower(&Flow)` 处的 deferred `FnOnce`，没有把生命周期扩散到公开的 `Cell` / `Table`，测试夹具也最具体。candidate 4 次之；同样方向正确，但 list 组装与编号映射的共享程度略弱，且 17a 的 derive 调整与后文切片说法不完全一致。

## Candidate 1

| 准则 | 分数 | 评语 |
| --- | ---: | --- |
| 1. Proposal fidelity | 5 | 覆盖格内富段落、blank/link/bold 与格内列表；保留单 payload 和既有禁用项。 |
| 2. Caller surface | 3 | 名称和 builder 复用正确，但 `Cell::ul/ol` 使用 `FnMut + 'static`，比正文现有的 `FnOnce` 更窄。把 owned capture 移入内层 item closure 的合法调用可能因此无法表达。 |
| 3. Layout honesty | 5 | 明确复用两层 allowlist、保持 `natural_height` Fit-only，并锁定 `"list item"` 容器名。 |
| 4. Fit to seams | 4 | deferred lower 与 `flow.child()` 是正确 seam；但 `FnMut` 并非 one-shot closure 的诚实类型，额外 `ListKindTag` 也可避免。 |
| 5. Testability | 5 | font id、cell-width blank、link rect、marker x、非法 item 和过高内容均有 RecordingCanvas/公开路径测试计划。 |
| 6. Laziness | 4 | 仅增加一个私有 seed，且允许 17a 先落；不过它错误地暗示 paragraph arm 本身必然阻止 `Clone`，derive 计划不够精确。 |
| **合计** | **26/30** | 方向正确，主要扣分点是 `FnMut` 对现有 `FnOnce` 调用面的不必要收窄。 |

## Candidate 2

| 准则 | 分数 | 评语 |
| --- | ---: | --- |
| 1. Proposal fidelity | 5 | 完整覆盖 §2.6 / §2.8，并明确 forbidden kids、单 payload 与整行分页。 |
| 2. Caller surface | 4 | `Cell::paragraph/ul/ol` 与 `FnOnce` 镜像正文，也支持借用外部数据；代价是公开类型变成 `Cell<'a>` / `Table<'a>`，显式类型注解和部分推断不再完全保持原貌。 |
| 3. Layout honesty | 5 | 对 allowlist、真实 cell 宽、Fit-only 和 list-item 诊断说明最完整。 |
| 4. Fit to seams | 3 | deferred lower 正确，但为非 `'static` capture 把 lifetime 传播到 `Cell`、`Table`、`Flow::table`、`From` 和 row/header bounds，明显大于本刀所需的最小桥。 |
| 5. Testability | 5 | 除渲染坐标外还要求 borrowed-capture 编译夹具，能验证其独特 API 承诺。 |
| 6. Laziness | 3 | 17a/17b 拆分诚实，但 17b 已成为公开所有权模型改造；相较 lifetime-free deferred seed，不够懒。 |
| **合计** | **25/30** | 最强的 borrowed-capture 方案，但不应作为首选基线；只有明确拒绝 `'static` 后才值得承担该 API 扩散。 |

## Candidate 3

| 准则 | 分数 | 评语 |
| --- | ---: | --- |
| 1. Proposal fidelity | 5 | 富段落、格内列表、禁用项、单 payload 和过高错误全部与提案一致。 |
| 2. Caller surface | 5 | 直接复用 `Paragraph`、`List`、`Numbering`；`Cell::new` 不变；list closure 保持 `FnOnce`。`'static` 是明确记录的 seam 成本。 |
| 3. Layout honesty | 5 | 不改 allowlist；paragraph 始终 lower 为未断行的 `FlowNode::Text`；精确区分 row move 与 full-frame 过高，并锁定 `"list item"`。 |
| 4. Fit to seams | 5 | 在 `Table::lower(&Flow)` 用 `flow.child()` 执行 deferred list；还抽取 paragraph、list assembly 和 numbering 映射供正文/cell 共用。无 krilla 泄漏。 |
| 5. Testability | 5 | 给出具体页面尺寸与预期坐标，覆盖 bold、cell-width blank、第二列 link rect、marker/嵌套 indent、所有关键非法 item、过高和窄 cell。 |
| 6. Laziness | 5 | 无新公开类型，仅一个私有 deferred 表示；layout 零改动；一刀两提交，list 真正超时才回退拆刀。 |
| **合计** | **30/30** | 最完整、边界最干净，可直接作为实现设计底稿。 |

## Candidate 4

| 准则 | 分数 | 评语 |
| --- | ---: | --- |
| 1. Proposal fidelity | 5 | 功能矩阵与禁止项均正确。 |
| 2. Caller surface | 5 | 复用现有 builder，采用 `Box<dyn FnOnce + 'static>`，不引入平行 DSL。 |
| 3. Layout honesty | 5 | 正确保留 `FlowNode::Text`、allowlist、Fit-only 与 list-item 容器诊断。 |
| 4. Fit to seams | 5 | `ListSeed` + lower-time `flow.child()` 是小桥，无 backend 越界；只是未像 candidate 3 那样明确统一正文/cell 的 list assembly 与 numbering helper。 |
| 5. Testability | 5 | RecordingCanvas 覆盖 rubric 要求，并补充 nested list、item image rejection 与 over-height。 |
| 6. Laziness | 4 | 类型增量很小且有 fallback；但切片章节把 17a/17b 同时说成子刀和同一刀内提交，并把 derive adjustment 放进 17a，与“list commit 才需移除 Clone/PartialEq”的更小 diff 不一致。 |
| **合计** | **29/30** | 与胜者非常接近；candidate 3 的共享 helper 和切片表述让边界更清楚。 |

## 推荐基底与 graft

以 candidate 3 为主，保留下列核心选择：

1. `Cell::paragraph` 立即执行 closure，保存完整 `Paragraph`；共享 lower 产出 `FlowNode::Text(TextNode::from_items(...))`。
2. `Cell::ul/ol` 保存 **`Box<dyn FnOnce(&mut List) + 'static>`**，到 `Table::lower(&Flow)` 才以 `flow.child()` 建 list。不要采用 candidate 1 的 `FnMut`。
3. 正文和 cell 共用 paragraph lower、list assembly、`Numbering -> MarkerKind` 映射；不改 layout allowlist 或 `natural_height`。

建议 graft：

- 从 candidate 2 graft **derive 变更的精确分层**：paragraph-only 阶段可给 `Paragraph` 增加 `Debug + Clone + PartialEq`，使 `Cell` 暂时保留 `Clone + PartialEq`、只失去 `Eq`；list closure 落地时再让最终 `Cell` 只保留手写 `Debug`。不要声称 paragraph 天然阻止 `Clone`。
- 从 candidate 1/4 graft **条件式 fallback**：若 list closure、trait 兼容性或测试未在时限内收口，17a paragraph 可独立发布，17b 再加 list；不要为赶同一提交牺牲 list 测试。
- 在最终文档/CHANGELOG 明示两项 API 成本：list closure 要求 `'static`；最终 `Cell` 移除 `Clone/PartialEq/Eq`。若产品明确要求借用非 `'static` 外部数据，再单独评审 candidate 2 的 lifetime 方案，而不是预先把 lifetime 扩散进本刀。

## 17a / 17b 建议

默认采用 **一刀、两个可独立审查的提交**：

1. **17a / commit 1：** shared paragraph lower、`Cell::paragraph`、paragraph RecordingCanvas 测试。
2. **17b / commit 2：** deferred `FnOnce` list、`Cell::ul/ol`、最终 `Cell` derive 调整、list/forbidden/over-height 测试。

只有第二提交超出约一晚、或 `'static` / trait 兼容性需要重新做产品决定时，才把它升级为真正的 17b 后续刀；此时先交付 17a。当前设计规模下不建议采用 candidate 2 的公开 lifetime 传播，也不建议强制把未完成的 list wiring 与 paragraph 绑在一个提交中。
