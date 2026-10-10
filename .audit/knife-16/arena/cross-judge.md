# Knife 16 arena cross-judge

## Candidate 1

| Criterion | Score | Note |
| --- | ---: | --- |
| Proposal fidelity | 5 | 接受非负且可为零的高度，spacer 无绘制，未放开任何禁用 cell child。 |
| Caller surface | 5 | `Cell::spacer(height)` 与 `Flow::spacer`、`Cell::hline/image` 对齐，没有引入 cell Flow closure。 |
| Layout honesty | 5 | 复用 `FlowNode::Spacer` 和既有 allowlist，并准确说明 `natural_height` 会把 `Partial` 转成 `UnsplittableTallerThanFrame`。 |
| Fit to seams | 5 | 生产改动严格限于 `CellContent`、`Cell::spacer` 和 `Table::lower`，无第二种 spacer、无 backend 改动。 |
| Testability | 3 | 太高错误和 allowlist 测试正确，但主几何测试让 spacer 与 `"A"` 同行后仍声称相对零高度基线移动约 `H`；实际移动量是 `H - LINE`（当 `H > LINE`）。无绘制测试中的“归因于 spacer 列”也缺少可直接观测的判据。 |
| Laziness | 5 | 未扩展富段落、列表、多孩子 cell，也未改变正文 spacer 分页语义。 |

## Candidate 2

| Criterion | Score | Note |
| --- | ---: | --- |
| Proposal fidelity | 5 | 完整覆盖可为零、无绘制、禁用孩子保持禁用等要求。 |
| Caller surface | 5 | 单高度参数的命名构造函数清楚且与现有 DSL 一致。 |
| Layout honesty | 5 | 明确区分正文 spacer 的 `Partial` 与 cell 中 `natural_height` 的整行约束。 |
| Fit to seams | 5 | 仅桥接既有私有枚举和 lower arm；布局生产代码、allowlist、backend 均不动。 |
| Testability | 5 | 使用 spacer-only 行避免 `max(text_height, spacer_height)` 干扰，以 `pt_near` 比较后续 glyph y；另有无绘制、零高度、过高错误和可选 allowlist unwrap。 |
| Laziness | 5 | 严守单 payload cell 和既有正文语义，额外负长度测试虽非必需但不扩大产品面。 |

## Candidate 3

| Criterion | Score | Note |
| --- | ---: | --- |
| Proposal fidelity | 5 | 所有提案约束及禁用项均被明确保留。 |
| Caller surface | 5 | `Cell::spacer(Length<NonNegative>)` 是唯一新增调用面，无隐式转换或 closure。 |
| Layout honesty | 5 | 对 `natural_height`、行高取最大值、整行移动与正文分页的说明最精确。 |
| Fit to seams | 5 | 生产边界与候选 2 同样窄，没有新节点或跨层依赖。 |
| Testability | 5 | 精确钉住 `24 + BASELINE`、零高度、spacer-alone 空 ops、过高错误和 allowlist；还识别了短 spacer 不应压过文字行高。 |
| Laziness | 5 | 额外内容都只是既有语义的测试，没有引入新 API 或布局行为。 |

## Candidate 4

| Criterion | Score | Note |
| --- | ---: | --- |
| Proposal fidelity | 5 | 非负、零值、无绘制及禁用孩子约束均正确。 |
| Caller surface | 5 | API 形状与既有命名构造函数一致，没有增加 cell flow surface。 |
| Layout honesty | 5 | 正确复用 `FlowNode::Spacer`，并明确过高 spacer 由 `natural_height` 报不可拆错误。 |
| Fit to seams | 5 | 生产修改范围准确，无第二 spacer 类型或 krilla 泄漏。 |
| Testability | 4 | 几何和过高错误测试可行；但“过滤出源自 spacer cell 的绘制命令”不是 RecordingCanvas 直接提供的归因维度，独立 spacer-only 空 ops 断言会更扎实。 |
| Laziness | 5 | 未提前实现多孩子 cell、段落、列表或正文语义变化。 |

## Recommended base

推荐 **candidate 2**。

Candidate 2 与 candidate 3 在六项标准上并列满分；按“并列时偏好更干净的边界”，candidate 2 更适合作为基础：生产改动边界同样严格，但主证明更集中于本刀必需行为，且用 spacer-only 行与零高度对照直接证明 glyph y 的精确位移，避开同行文字行高的干扰。它也清楚保留了 `natural_height` 的不可拆语义，没有把正文 spacer 的跨页行为误投射到 cell。

## Grafts worth taking from losers

- 从 candidate 3 取独立的 `cell_spacer_alone_records_no_ops`：单独渲染 spacer-only 表并断言页面 ops 为空，比在含文字的页面中尝试按来源归因更强。
- 从 candidate 3 取“短 spacer 不压过同行文字行高”的可选测试；它能防止实现者把 row height 错写成求和，但不是合入本刀的硬门槛。
- 从 candidate 1 取 rustdoc 中正文与 cell 行为的简短对照：正文 spacer 可跨页，cell spacer 超过完整 inner frame 则报 `UnsplittableTallerThanFrame`。

Candidate 4 没有必须移植且未被上述方案覆盖的独有内容。

## Divergences from autonomy defaults

无。推荐方案保留全部默认项：`Cell::spacer(height: Length<NonNegative>)`、允许零高度、每个公开 Cell 单一内容、过高时报 `UnsplittableTallerThanFrame`，并以相邻文字 y 位移而非仅页数作为主证明。
