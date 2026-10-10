# Cross-judge：quire-pdf Slice 1（geom + error）

## 评分

### C1 — `Length<K>` 密封阶梯

| 标准 | 分数 | 理由 |
|---|---:|---|
| 1. Typed lengths | 2 | `Length<Finite/NonNegative/Positive>` 集中执行有限、非负、正值检查，mm→pt 也集中在长度构造器。 |
| 2. Illegal states unrepresentable | 2 | `Point`、`Size`、`Rect`、`Margins`、`Stroke` 都持有私有的已验证长度，不能塞入裸非法 `f32`。 |
| 3. Scope discipline | 2 | 只纳入构造错误、零默认边距、A4/LETTER 和最低限度绘图原语，没有越界到 DSL、limits 或 backend。 |
| 4. Interface depth | 1 | 验证被隐藏，但 `Point::new`、`Size::new`、`Rect::from_origin_size`、`Stroke::new` 仍对已验证参数返回冗余 `Result`，`Margins::try_new` 更是恒为 `Ok`。 |
| 5. Downstream fit | 2 | crate 内可用 `pt()`、`pt_xywh()`、`pt_trbl()` 直接取 pt，且具备所需的三种 `PathVerb`。 |
| 6. Distinctiveness + coherence | 1 | 阶梯本身鲜明，但示例调用了外部不可见的 `pub(crate) pt()`，使用 `Size::A4` 而签名实际给出自由常量 `A4`，且缺少安全的向宽约束 `From` 转换。 |
| **总分** | **10/12** | |

### C2 — branded `Length<C>` + 非空 Path builder

| 标准 | 分数 | 理由 |
|---|---:|---|
| 1. Typed lengths | 2 | 单一 `Length<C>` 配合三个 brand 完整覆盖有限、非负、正值；所有 mm 工厂复用同一换算边界并要求检查换算后溢出。 |
| 2. Illegal states unrepresentable | 2 | 聚合类型只接收对应 proof；`Stroke` 强制 `Positive`，`Path::begin` 还排除了空路径、首个 `LineTo` 和 `Close` 后追加。 |
| 3. Scope discipline | 2 | `Error` 只有三个标量构造错误，边距默认零，并严格停在 A4/LETTER 与最小绘图词汇。 |
| 4. Interface depth | 2 | `Finite::pt` 等工厂隐藏校验策略；已验证值组合成 `Point`、`Size`、`Rect`、`Margins`、`Stroke` 时均不可失败。 |
| 5. Downstream fit | 2 | `as_pt()` 可直接供 layout/canvas 使用，proof 的 `From` 宽化明确，路径暴露稳定的 verb slice。 |
| 6. Distinctiveness + coherence | 2 | brand 同时充当工厂和类型证明，proof lattice、聚合构造器、错误面与 usage 相互一致。 |
| **总分** | **12/12** | |

### C4 — `Coord` / `Length` / `PosLength`

| 标准 | 分数 | 理由 |
|---|---:|---|
| 1. Typed lengths | 2 | 三个具名 newtype 分别保证有限、非负和正值，`from_mm` 集中换算，未使用整数 pt。 |
| 2. Illegal states unrepresentable | 2 | `Size`、`Rect`、`Margins` 和 `Stroke` 均持有已验证标量；公开的 margin 字段仍是受约束的 `Length`。 |
| 3. Scope discipline | 1 | `Rect::deflate/contains_pt`、路径快捷工厂、具名颜色、归一化后端颜色、`InvalidPath` 等明显超过本 slice 的必要边界。 |
| 4. Interface depth | 1 | 校验集中，但公共模块、公开字段、成组的 typed/raw 双访问器和大量快捷方法形成了偏宽且较早冻结的 API。 |
| 5. Downstream fit | 2 | `to_pt()` 及聚合 pt 访问器便于消费，路径也含 `MoveTo/LineTo/Close`。 |
| 6. Distinctiveness + coherence | 1 | 三具名类型很清楚，但 `InvalidPath` 没有对应的 fallible path API，空路径和 `LineTo` 起步仍可表达；零宽 `Stroke` 的语义也未钉死。 |
| **总分** | **9/12** | |

## 推荐基线

推荐 **C2**。

它在相同类型安全水平下提供了最干净的构造边界：原始数值只在 brand 工厂处失败，拿到 proof 后的聚合完全不可失败；`Positive -> NonNegative -> Finite` 的安全宽化也比 C1 的“收紧后缺少宽化”更适合后续 layout 组合。相比 C4，C2 没有提前加入正文框运算、后端颜色转换或第二层错误类型，公共 API 更小，未来维护者可按真实需求增加能力。

实现时需要核实泛型派生是否给 marker 引入多余 trait bound；如有，应手写 `Length<C>` 的相关 trait impl。这是实现细节，不改变设计选择。

## 可从落选方案移植的内容

### 从 C1

1. 采用 `length.rs`、`point.rs`、`size.rs`、`path.rs` 等内部文件拆分；保持 C2 的统一重导出，不必把整个实现堆进单个 `geom.rs`。
2. 可增加少量 **`pub(crate)`** 聚合拆包器，如 `Rect::pt_xywh()`、`Margins::pt_trbl()`，供 canvas/backend 边界使用；不要把 C4 那套重复 raw getter 全部公开。

### 从 C4

1. 增加 `Point::ZERO`；它常用、无歧义且不扩大验证策略。
2. 保留“错误应带非法值”的诊断意识；C2 已携带 `value`，若以后真实报错显示字段上下文必要，再在现有错误中扩展，不要现在引入独立 `GeomError` 层。

## 拒绝的想法

- 拒绝 C1 对已验证组件继续返回 `Result`：`Size::new(PosLen, PosLen)`、`Rect::from_origin_size(Point, Size)` 和 `Stroke::new(...)` 应当不可失败。
- 拒绝 C1 的 `Margins::try_new` 恒成功占位，以及与实际可见性/常量位置不一致的 usage；这些会让调用方误判 API。
- 拒绝 C4 的 `Error::Geom(GeomError)` 双层错误体系：当前只有一个构造域，分层只增加匹配和重导出成本。
- 拒绝 C4 在 Slice 1 加入 `Rect::deflate`、`contains_pt`、命名色板、`to_rgba_f32`、`Margins::symmetric` 等便利层；应等 layout/backend 的真实调用形状出现。
- 拒绝允许零宽 `Stroke` 代表“不描边”：PDF 的零宽可能是 hairline，C2 的 `Positive` 宽度配合“没有 stroke 即不画”语义更确定。
- 拒绝可公开构造空路径或以 `LineTo` 开头的 mutable `Path`；C2 的 `Path::begin` 边界更可靠。复合闭合子路径若以后确有需求，再扩展 builder。
