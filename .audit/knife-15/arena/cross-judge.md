# Knife 15 交叉评审

评分采用每项 0–5 分，六项等权，总分 30。

## 候选评分

| 候选 | 1. 提案忠实度 | 2. 调用方面 | 3. 排版诚实度 | 4. 接缝适配 | 5. 可测试性 | 6. 克制性 | 总分 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **1** | **5** — 范围、URI 原样写入、逐行矩形及 Band 都对齐 | **2** — `link -> Result<()>` 与现有 `paragraph` 的 `()` 闭包冲突；正文同时给出两套方案并留给实现 PR 二选一，usage 中的 `?` 实际不可用 | **5** — face+link 合并、link 边界 flush、最终 x、行盒 y/h、零宽省略均完整 | **5** — 正确复用 `LinkTarget`、`FragmentCommand::Link`、平移、Canvas 与 backend | **4** — 几何测试很全，但公开 DSL 的 `EmptyLink` 测试路径受未决 API 影响 | **5** — 没有扩张到 Cell、Layer、图片或视觉装饰 | **26** |
| **2** | **5** — 完整覆盖 §8，明确不 fetch、不验 scheme、不做 destination | **5** — 仅增加 `Paragraph::link` 与两个 Band 整槽方法；沿用 builder 首错记录，空 URI 不进入内部 run | **5** — 两层边界、对齐后 x、行盒顶与高度、bold/body 单矩形、分页局部坐标都讲清 | **5** — `FragmentCommand::Link` 贯穿 `with_offset` / `draw_fragment`，backend 与 canvas 无改动 | **5** — RecordingCanvas 覆盖单行、换行、字体切换、空 URI、偏移、Band、无装饰，并把 `%PDF` 留作冒烟 | **5** — 没有新增公开辅助类型或刀外能力 | **30** |
| **3** | **5** — 提案范围与字符串语义准确，Band 也可达 | **4** — `Linked` 限定只可写 `text`/`bold`，边界清楚，但新增并重导出一个公开类型及两方法；同一 link scope 内无法直接插 blank，只能退出后重入 | **5** — 数据流和最终矩形计算准确，明确处理跨页 y 重置 | **5** — 完整复用既有注解链路，无 krilla 越界 | **5** — 测试矩阵最细，尤其包含逐页 Band URI、分页和命令等价性 | **4** — 未做刀外功能，但公开 `Linked` 是完成同一目标并非必需的 API 扩张 | **28** |
| **4** | **5** — 功能边界和提案要求均正确 | **4** — DSL 可用且错误传播符合现状，但公开 `IntoLinkTarget` trait 加五组实现扩大了长期兼容面 | **5** — 合并、flush、最终 x/y、零宽及 bold/body 行矩形都正确 | **5** — 复用 Canvas/backend，片段平移链完整 | **4** — RecordingCanvas 覆盖充分；新增 PDF 明文 URI/结构断言没有必要，压缩输出也不稳定 | **4** — 功能范围克制，但通用转换 trait 和多种输入形态超出本刀所需 | **27** |

## 推荐基底

推荐 **candidate-2**。

它与 candidate-3 在内部排版主线几乎同样严谨，但公共面更小：无需新增 `Linked` 或转换 trait，直接沿用仓库已有的 builder 错误累计方式，因而不会出现 candidate-1 的 `Result`/闭包签名矛盾。`BandSlot { text, link }` 也把槽文本与链接保持为一个内聚状态，后续若扩展槽表示，迁移点清楚。

从维护角度看，candidate-2 的边界足够明确：URI 只在 `Paragraph::append` 进入 `TextRun`，只在 link 变化处切测量组，只在 `place_line`/`fragment_from_lines` 形成几何，最终只经 `FragmentCommand::Link` 到 Canvas。每层各承担一个职责，且没有新增公开抽象需要长期兼容。

## 可从落选方案移植的内容

- **candidate-1**
  - 移植其分阶段实现顺序：先打通内部 run/group，再接 fragment 命令，最后接 DSL；便于用内部测试隔离回归。
  - 保留其对组内 link 同构性的 `debug_assert`，可尽早发现未来改动绕过 flush。
- **candidate-3**
  - 移植 `header_uri_can_differ_per_page` 与 `link_across_pages_restarts_y` 两个测试，分别锁住 Band hook 的逐页求值和分页后的局部 y 重置。
  - 可采用内部 `TextRun::with_link` 辅助构造，但不要因此公开或重导出 `Linked`。
- **candidate-4**
  - 移植其对“为何不能让 `Paragraph::link` 返回 `Result`”的说明，作为 API 注释或实现说明，避免以后重新引入与 `FnOnce(&mut Paragraph) -> ()` 不兼容的签名。
  - 增加 `" "`、`mailto:`、`"not a uri"` 的原样保存测试，明确锁住“不 trim、不做 scheme 校验”。

## 评分时的歧义

- Rubric 没有指定分值范围或权重；本评审采用六项等权、每项 5 分。
- “构造期拒绝”可能被理解为方法立即返回 `Result`，也可能是 builder 当场记录错误、在 lower/paint 前返回。结合现有 `Paragraph::error`、Band hook 和“尽量早于 `start_page`”，后者应视为满足要求。
- “blank ends a linked group”没有完全说明 blank 是否必须能写在同一个 `link` 闭包内。frame 的默认示例更支持 candidate-2 的行为：blank 本身不带链接，但闭包中 blank 后的文字重新形成同 URI 组。
- “小 API”与“更严格的闭包能力边界”存在取舍：candidate-3 的 `Linked` 更能静态限制可调用方法，candidate-2 则少一个公开类型。本刀只涉及 `text`/`bold` 且已有明确运行时语义，因此以较小公共 API 为优先。
