# 刀 7 arena 交叉评审

评审对象：`candidate-1/design.md`（下称 C1）、`candidate-2/design.md`（下称 C2）。
依据：`frame.md` 评分标准、`how.md`、`docs/提案书.md`（§2.1、§2.6、§2.8、§2.10、§2.0 已对照原文）。

**结论：以 C1 为底，从 C2 嫁接若干类型和测试。**

---

## 逐项评分

| 评分项 | C1 | C2 |
| --- | --- | --- |
| 1. 提案书一致性 | 5 | 3 |
| 2. 协议契合 | 4 | 4 |
| 3. 领域建模 | 4 | 3 |
| 4. 接口深度 | 5 | 2 |
| 5. 可测性 | 4 | 5 |
| 6. 克制 | 4 | 3 |
| **合计** | **26** | **20** |

### 1. 提案书一致性

- **C1 = 5。** 栏可跨页，tail 是新的 `Columns`；表支持 colSpan/rowSpan、rowSpan 块原子、tail 上重复全部表头；列表有五种枚举加逐项自定义串，悬挂缩进，标记只出现在首个片段。装饰是 `Fn(&PageInfo)` 闭包，`PageInfo` 带 `margins`，带高等于 margin，带宽等于页宽。这与 §2.10 原文「闭包……当页四边 margin」逐字对应。`FlowNode::Text` 让 §2.1「换行宽 = 当前框」成为协议本身的性质。
- **C2 = 3。** 栏、表合并、表头重复、列表三项都满足。装饰带宽也是全页宽。扣分有三处：
  - 装饰是静态的 `BandLine{left,center,right}` + `DecorationInline`，既不是闭包，也不把 margin 交给调用方，偏离 §2.10。刀 8 得推倒重做。
  - `TableNode::assemble` 要求传入 `full_body_height`，调用方还得自己算 `content_w`。宽度和高度在构造时定死，不随当前框变化，与 §2.1 冲突；表放进栏里时尤其容易传错。
  - 页脚从带顶往下排，作者自己也把这一点列为未决问题。

### 2. 协议契合（新节点走 `FlowNode` / `Split`；装饰不重切正文）

- **C1 = 4。** 三个新节点都是 `FlowNode` 变体，`Split` 签名不变，`plan_pages` 一行不改。扣分有两处：
  - `Text` 让 `split` 开始读 `avail.width`，于是协议里同时存在两种文本形态（未排的 `Text` 和已排的 `Paragraph`）。宽度守卫兜住了误用，但这是一处真实的语义扩张。
  - 装饰结果写进 `PagePlan.header/footer`，计划类型因此多了绘制期数据。
- **C2 = 4。** 装饰在 paint 阶段量，`measure_bands` 只拿到页尺寸序列，看不到 `placed`。「不重切正文」在结构上有保证，这是两份里最干净的做法。扣分有两处：
  - 引入 `Split<T>` 泛型，并把它抬升成 `Split<FlowNode>`，为刀 7 不需要的东西改动了协议核心。
  - `split_seq` 把 `split_stack` 的段后距规则复制了一遍，作者承认编译器不会提醒两处同步。

### 3. 领域建模（类型而非字符串/布尔；非法单元格内容不可表示或被校验）

- **C1 = 4。** 类型做得扎实：`Ratio` newtype、`Length<Positive/NonNegative>`、`NonZeroU16` 跨度、`MarkerKind`、`PageNumber/PageCount` newtype。单元格内容仍是 `Vec<FlowNode>`，靠 `check_content` 穷尽 match 校验，属于「被校验」而不是「不可表示」。另有 `head_started: bool`，错误字段 `container/node` 用的是 `&'static str`。
- **C2 = 3。** `CellContent` / `ListBody` 让非法内容直接不可表示，这一点满分。`ItemMarker::{Auto,Custom}`、`ListKind::Ordered{style,start}`、项上的 `Option<GlyphRun>` 也都好。但其余部分大量使用原始 `f32`：`ColumnWidth::Fixed(f32)`、`Ratio(f32)`、`padding`、`border_width`、`marker_width`。跨度是 `u32`，靠运行时检查。错误用 `reason/kind: &'static str` 区分七八种情况，正是标准里点名的 stringly 写法，也违反提案书 §2.0「约束优先类型化」。再加上宏生成两份同构枚举外加私有 `LimitedKind`，维护时有额外负担。

### 4. 接口深度

- **C1 = 5。** 每个节点只有 `new(spec) -> Result` 和 `split` 两个入口。列宽、grid、行高、边框、标记都藏在 `split/measure` 里，刀 8 的 lower 不需要知道任何宽度。
- **C2 = 2。** `TableNode::assemble` 有 10 个参数。调用方要先解列宽，再按 `widths[i] - 2*pad - border` 预排段落，还要传 `full_body_height`。`assemble_list` 要求调用方按 `outer_width - indent` 预排，然后再核对一遍宽度。宽度知识泄漏到栏、表、列表三处（正中 how.md 的 Gotcha 1），栏也没有守卫，错宽会静默溢出。另外，测试里传的是 `&mut fonts`，签名却写 `ctx: &mut LayoutCtx`，签名和用法本身不一致。

### 5. 可测性

- **C1 = 4。** 测试名具体，覆盖面完整：栏 x/gap、栏跨页、表头重复、rowSpan 块、边框矩形、罗马数字、自定义串过宽、空白页装饰、绘制顺序、出错零输出。约三分之一的测试只有注释桩，没有写出数值断言。
- **C2 = 5。** 测试写得完整，`to_bits()` 精确到续页 x、右槽右缘等于页宽、页脚基线；还有两项 C1 没有的断言：「有装饰与无装饰正文基线 bit 相同」和「出错时无 `StartPage`」。列宽每种失败原因各有一个测试。

### 6. 克制（无公共 DSL；无用不上的抽象）

- **C1 = 4。** 不加 `Fill`，不加 DSL，不加分隔线。扣分点：多了一个 frame 未列出的错误 `InvalidCellSpan`；`Container::Band` 允许装饰带里放表和栏，超出刀 7 需要。`Text` 有实际使用者，不算多余。
- **C2 = 3。** 不加 `Fill`、不加 DSL 是对的。扣分点：
  - 提前做了 `RuleNode`。§2.3 已定方向，但分隔线不在刀 7 的 frame 范围内，C1 也明确把它留给单独一刀。
  - `Split<T>` 泛型、宏、`LimitedKind` 都是本刀用不上的抽象。
  - 新开了 `column_width.rs` 文件。

---

## 推荐的底：C1

后续维护者在 C1 上扩展更安全，理由有四条：

1. **宽度只有一个来源。** 宽度只从 `Frame` 来，加上 `Paragraph` 的宽度守卫，「预排宽度传错、文字静默溢出」这一整类 bug 被关掉了。C2 把这份知识散到每个构造点，刀 8 的 lower 和后来加 image、分隔线的人都得重新算对。这是两份设计之间最大的安全差距。
2. **装饰形状与提案书一致。** C1 的装饰已经是 §2.10 的闭包 + margin 形状，刀 8 只需要把它公开，并补上 section 页码。C2 的 `BandLine` 静态结构到刀 8 必须整体替换。
3. **没有平行的手工同步点。** C1 加节点时，穷尽 match 会逼着维护者更新内容矩阵。C2 有两处靠人记：宏枚举之外的 `split_seq` 和 `split_stack` 段后距要同步，`Split<T>` 的抬升也要同步。
4. **接口小。** 刀 8 的 lower 只需要把 DSL 映射成 spec，不需要知道表的 padding 公式或页高。

C2 真正领先的地方只有两处：非法单元格内容不可表示，以及装饰与计划彻底隔离。两者都能以较小代价嫁接到 C1 上，见下一节。

---

## 从 C2 嫁接

1. **装饰不进 `PagePlan`。** 采用 C2 的隔离方式：`measure_bands(sizes: &[Size], margins, &Decorations, ctx) -> Vec<PageBands>` 只读页尺寸，在 `paginate_root` 中 plan 之后、paint 之前调用，结果作为局部 `Vec` 与 pages 一起 zip 交给 `paint_pages`。C1 的闭包 + `PageInfo` 保留。这样 `PagePlan` 仍只由 `plan_pages` 写入，「装饰重切正文」在签名层面就做不到。
2. **单元格内容只能经校验构造。** 不引入宏和双枚举，只把 C1 的 `Vec<FlowNode>` 包成私有字段的 newtype，例如 `CellBody(Vec<FlowNode>)` / `ItemBody(...)`，唯一构造途径是 `check_content`，`TableSpec`/`ListItem` 持有这个 newtype。效果接近「不可表示」，矩阵仍然只有一份。
3. **列表标记的类型。** 用 `ListKind::{Bullet, Ordered{style, start}}` 替换 `kind + start`，避免 Bullet 带一个没有意义的 start。每项改成 `marker: Option<GlyphRun>`，组装时烘焙好，替换 `head_started: bool`。
4. **错误字段带数据并类型化。** 采用 `DecorationTallerThanMargin { height, margin }`。`ColumnWidths` 如果要区分原因，用内部 enum，不要用 `&'static str`；`InvalidCellContent` 同理。
5. **测试。** 加入「有/无装饰时正文基线 bit 相同」；「出错时没有任何 `StartPage`」；右槽或居中对齐的精确 bit 断言（如果保留左中右对齐）；`column_widths_<reason>` 逐项测试。C1 的注释桩测试全部补成数值断言。
6. **明确的边界规则。** 沿用 C2 的几条规定：字母序号超过 Z、罗马数字超过 3999 时报错；自定义标记仍占用序号（是否如此要先确认）；自定义标记为空串时报错（C1 原来当作空白，C2 的做法更符合 §2.0）；`ColumnsNode` 要求列数 ≥ 2。

**不嫁接：** `Split<T>`、宏/`LimitedKind`、本刀的 `RuleNode`、`full_body_height` 参数、静态 `BandLine` 装饰、只画可见格线的 collapsed 边框（C1 每格画一个矩形更简单，像素结果相同）。

---

## 风险（以 C1 为底）

- **`Text` 重复排版。** `Text` 收到 `MoveToNextPage` 时如果原样把自己交回，换页后会重新 layout。`split` 里一旦排好，无论 tail 还是 move，都应该交回 `Paragraph`。
- **宽度守卫的浮点比较。** 守卫依赖逐 bit 比较。栏宽每次 split 都重新解，`Table::Measured.width` 也要比较，所以必须保证同一输入、同一函数得到同一 bit，不能有两条计算路径，并补测试锁定。
- **两种文本变体并存。** 刀 8 的 lower 应当只产出 `Text`。`Paragraph` 只作为续排形态，以及现有测试的入口；最好在 lower 侧加一条约束，让它无法直接构造 `Paragraph`。
- **装饰带内容过宽。** `Container::Band` 允许表、栏、keep_together，超出刀 7 的需要。建议先收紧到 Text/Paragraph/Spacer/Stack/List，否则装饰每页都要在带里跑表格测量。
- **`InvalidCellSpan` 不在 frame 列表里。** 需要先确认这个错误名，或者把非法合并并入 `InvalidCellContent`（C2 用 `kind: "span"` 的做法）。
- **`Ratio` 语义待确认。** C1 是「扣 gap 后内容宽的份额，取值 (0,1]」，C2 是「按权重分剩余」。提案书原文是「按剩余比例」，实现前要定下来，否则栏和表的列宽测试会推倒重写。
- **列表项内 keep_together。** C1 禁止，C2 也禁止。§2.8 的允许清单里没有它，但 §2.5.1 推荐用 keep_together 防止跨页，两条之间可能有张力，建议列为待确认条目。
- **页脚在带内的垂直对齐。** 两份都从带顶往下排，需要用户确认是否应当贴页底。
- **内存。** 大表在测量前每个单元格各持有一份 `String + Style`，限额 PR 应把单元格数纳入。
