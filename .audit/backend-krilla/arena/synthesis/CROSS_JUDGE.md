# 交叉评审：knife 3 backend-krilla 三份设计候选

只读评审。未改动 `src/`。评分对照 FRAME.md 的 6 条 rubric，逐条给证据，不按总体观感。

事实校准来源：`src/canvas.rs`（Canvas trait / RecordingCanvas 已发货）、`src/error.rs`（今日 `Error` 为 `Copy`，仅 4 变体）、`src/geom.rs`（branded `Length`，`Path::verbs()`，`Color::channels()`）、`Cargo.toml`（crate 名 `quire-pdf`，`publish = false`，计划钉 `krilla =0.8.2` 且注释写明 `default = ["backend-krilla"]`）、`docs/quire-design.md` §坐标转换 / 决定 7 / PR 4。

---

## 候选 1：plan-then-serialize（Plan 内存已转换的 krilla 值）

| 准则 | 分 | 一句话证据 |
| --- | --- | --- |
| 1 Canvas + 封死 krilla | 5 | `lib.rs` 明确不动，`backend` 保持 `pub(crate)`，`mod.rs` 只导出 `KrillaCanvas` 一个名字，无 krilla 重导出。 |
| 2 借用链无 unsafe | 5 | `Document`/`Page`/`Surface` 只作为 `Plan::serialize` 的局部量，surface 用块作用域结束后才 `add_annotation`；显式否掉 ouroboros/self_cell 与渲染线程两条路。 |
| 3 Error/Cargo 最小化 + 非 panic 延后 | 3 | 加 4 个公共变体，其中 `UnknownFont{id}` / `UnknownImage{id}` 用「未知」命名「本刀未做」，语义偏移；`default = []` 违背设计稿决定 7 与 `Cargo.toml` 计划注释（自陈可逆）；延后行为本身最好——调用点即返 `Err`，无 panic。 |
| 4 测试对齐设计 | 5 | 10 条测试覆盖 `%PDF`、identity convert、观察到的路径算子、观察到的 `/Annot` 矩形，明确不做整份 PDF 快照；额外有 `metadata` 迟到仍生效、与 `RecordingCanvas` 对账两条。 |
| 5 接口纵深 | 5 | 公共面只多 feature + 错误变体，`lib.rs` 零改动；内部吸收了 metadata 前置、注释时序、两支笔、serialize 档位、krilla 错误文本、`finish(self)` 失败语义。 |
| 6 后续扩展结构清晰度 | 3 | 在 Canvas 调用点就转成 krilla 值，意味着字形刀必须把 `FontStore` 提前挂到 `KrillaCanvas` 上（krilla `Font` 只有渲染期才自然可得）；`convert::to_annotation(link: super::krilla_canvas::Link)` 让 convert 反向依赖兄弟模块的私有类型。 |

**总分 26 / 30**

---

## 候选 2：buffered page plans（Plan 存 crate 原生类型，渲染期再转换）

| 准则 | 分 | 一句话证据 |
| --- | --- | --- |
| 1 Canvas + 封死 krilla | 5 | `KrillaCanvas` 保持 `pub(crate)`，模块图写明「Exports no public krilla type」，`Error::Backend(String)` 承接 krilla 诊断而不暴露其错误类型。 |
| 2 借用链无 unsafe | 5 | `render(plan)` 里 surface 取一次并在内嵌块中结束借用，之后才 `add_annotation` 再 `page.finish()`；`PagePlan` 只存 `Size`/`Mark`/`Link`，不含任何带生命周期的 krilla 值。 |
| 3 Error/Cargo 最小化 + 非 panic 延后 | 4 | 只加 3 个 backend-neutral 变体，`UnsupportedCanvasOperation{operation}` 精确命名「本刀延后」；Cargo 与设计稿一致（`default = ["backend-krilla"]`、`=0.8.2`、`default-features = false` + `raster-images`），且是唯一给出 base / backend 两套显式 CI 命令的候选。扣分点：只剩 `MoveTo` 的路径不受 brand 保护，`PathBuilder::finish` 返 `None` 时会被归成 `Error::Backend`（"backend contract mismatch"），误诊为后端问题而非调用方几何。 |
| 4 测试对齐设计 | 5 | 9 条测试正好覆盖四项硬要求，且是唯一断言「fill 后 stroke 只出现各自算子」的候选——这正对设计稿风险表里的「fill_path 留下 stroke」；末尾明确不快照整份 PDF、不从 PDF 空间数字反推 API 坐标。 |
| 5 接口纵深 | 5 | 公共面 = feature + 3 个变体；serialize 档位靠 `#[cfg(test)] new_observable_for_test` 留在内部，调用方不选 pretty/compress，也不碰 metadata 时序与注释时序。 |
| 6 后续扩展结构清晰度 | 5 | `Mark` 存 crate 类型、转换在渲染期发生，字形/图片注册表可以直接挂在 renderer 旁边而不侵入每个 `Canvas` 方法；open question 里已经点名问这件事。 |

**总分 29 / 30**

---

## 候选 3：split-tape session + emit-only，私有 `EmitError`

| 准则 | 分 | 一句话证据 |
| --- | --- | --- |
| 1 Canvas + 封死 krilla | 5 | 封印最强：`session.rs` 声明零 krilla import，于是「`Document` 字段」在类型层面无法出现；`lib.rs` 不动，公共类型表不增长。 |
| 2 借用链无 unsafe | 5 | `emit_page` 每页只取一次 `surface()`，用 `surface.finish()` 显式结束借用再进 link 循环；三份里唯一写出「`surface()` 二次调用会清页」「`Surface::finish` 就是 drop、push/pop 不平衡才 panic」两条 krilla 陷阱。 |
| 3 Error/Cargo 最小化 + 非 panic 延后 | 3 | 公共 `Error` 零增长、保住 `Copy`，这一半很好；另一半不行：`draw_glyph_run` / `draw_image` 返回 `Ok(())`，所有时序错误也返回 `Ok(())`，「不 panic」是靠假装成功换来的，调用点拿不到任何信号。私有 `EmitError` 10 个变体里 `RejectedRect` / `RejectedOpacity` / `RejectedPage` 按 brand 已不可达，与 subtract-before-you-add 相反。 |
| 4 测试对齐设计 | 4 | 四项硬要求都在（identity 用 `to_bits` 位比较，是三份里最严的），不快照整份文件；但缺「两支笔互清」测试，且 `probe_settings()` 既是测试档也是唯一生产档，测试与产物档位没有分离。 |
| 5 接口纵深 | 3 | 表面最薄（只多 feature），但代价是把成本推给下一把刀：`finish` 返 `Result<Vec<u8>, EmitError>` 而非 `crate::Result`，将来 `to_pdf` 必须写一层 10 变体 → 公共变体的映射，公共 `Error` 该长的还是要长；同时 `Ok(())`-always 的 trait 实现属于「看着深、实则不传信息」。 |
| 6 后续扩展结构清晰度 | 4 | `convert` / `session` / `emit` 三文件的阅读地图很干净（「字节从哪来」「canvas 调用能存什么」「有没有翻 y」各一处）；但 `deferred_glyphs` / `deferred_images` 两个计数器是纯脚手架，字形刀要先删它们，且唯一 serialize 档位届时必须拆开。 |

**总分 24 / 30**

---

## 总分

| 候选 | 1 | 2 | 3 | 4 | 5 | 6 | 总计 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| candidate-1 plan-then-serialize | 5 | 5 | 3 | 5 | 5 | 3 | **26** |
| candidate-2 buffered page plans | 5 | 5 | 4 | 5 | 5 | 5 | **29** |
| candidate-3 split-tape / EmitError | 5 | 5 | 3 | 4 | 3 | 4 | **24** |

三份在准则 1、2 上同分且都成立：本刀的硬分叉（`Document`/`Page`/`Surface` 不能共处一结构）三份都用「缓冲 + 词法重放」解决，没有 unsafe、没有自引用 crate、没有改 `Canvas` trait。差距全在 3、4、5、6。

---

## 推荐底座：candidate-2

理由，按权重排序：

1. **错误面的形状是三份里唯一站得住的。** 本刀真正的新错误只有两类——生命周期误用、以及「这把刀不做」。`InvalidCanvasState{operation, state}` 与 `UnsupportedCanvasOperation{operation}` 正好对上，并且这两个变体在字形刀、图片刀、DSL 刀里都还用得上，不是一次性命名。候选 1 用 `UnknownFont` 表示「未实现」，候选 3 把 10 个变体藏进私有类型再留一层映射债，两者都是把本刀的账记到别处。
2. **扩展缝对了。** Plan 存 crate 原生 `Mark`、转换发生在渲染期，这条决定让字形与图片的注册表能长在 renderer 旁边。候选 1 在 Canvas 调用点就转成 krilla 值，字形刀被迫把 `FontStore` 提前拉到 `KrillaCanvas` 的每个 `fill_path` / `stroke_path` 路径上。
3. **和已确认约定零冲突。** `default = ["backend-krilla"]`、`=0.8.2` + `default-features = false` + `raster-images`、从本 PR 起 CI 跑两套 feature 组合，全部照设计稿决定 7 与 `Cargo.toml` 的计划注释执行；候选 1 在这里自行改了 `default = []`。
4. **测试里有一条别人没有的。** 「fill 之后 stroke 只出现各自算子」直接钉设计稿风险表里的两支笔互清，这是实现期最容易悄悄回归的一项。

---

## 从落败者移植（graft）

### 从 candidate-1

1. **`Error::InvalidGeometry` 补回来，用于只含一个 `MoveTo` 的路径。** 这是 brand 唯一盖不住的几何失败（`Path::begin(p).finish()` 合法可构造），候选 2 把它归成 `Error::Backend` 是误诊。同时采纳候选 1 的减法：**不要**加 `InvalidPageGeometry`——`Size` 两轴都是 `Length<Positive>`，那条分支已不可达。
2. **`#[test] #[ignore] fn dump_experiment_pdf()` 这把「杠杆」。** 内容流算子与 `/Annot` 矩形的字面量必须能被下一个人重新推导，而不是变成口头传说；krilla 版本一动就重跑一次。配套抄走候选 1 的测试注释规则：数字与 `(10, 20)` 不符**不是**给 `to_krilla_point` 加翻转的理由。

### 从 candidate-3

1. **「`session` 模块零 krilla import」这条不变量。** 候选 2 的 `krilla_canvas.rs` 同时装计划构建与 krilla 渲染；把渲染搬进 `emit.rs`（或至少在文件头把该不变量写成注释 + 让 `render` 成为唯一 `use krilla` 的函数），可以让「`Document` 不许成为字段」由模块结构而不是纪律来保证。这是候选 3 唯一比候选 2 更强的结构点。
2. **krilla 陷阱清单落进文档注释。** `surface()` 二次调用会清掉本页绘制；`Surface::finish` 即 drop、push/pop 不平衡才 panic；`..Default::default()` 会把 `enable_tagging` 留在 `true`，必须显式关掉。候选 3 把三条都写出来了，候选 2 只覆盖了第三条的一半。

---

## 硬否决项

按候选列出，落地时不要带进来：

- **candidate-3：`draw_glyph_run` / `draw_image` 返回 `Ok(())`。** FRAME 要求「不用 panic stub 地正确延后」，返回 `Ok(())` 再丢弃入参是另一种 stub——语义上的静默成功。所有时序错误也一并返 `Ok(())`，同样否决：调用点必须拿到 `Err`。
- **candidate-3：`probe_settings()` 作为唯一 serialize 档位。** pretty + 不压缩是实验档，不能同时当产物档。保留候选 2 的 `production()` / `#[cfg(test)] observable()` 两档。
- **candidate-1：`default = []`。** 与设计稿决定 7 及 `Cargo.toml` 计划注释冲突，且会让 `cargo test` 默认跳过本刀唯一的实验测试。按候选 2 的 `default = ["backend-krilla"]`。
- **candidate-1：`convert::to_annotation(link: super::krilla_canvas::Link)`。** convert 反向引用兄弟模块的私有类型，方向错了；按候选 2 的 `to_annotation(rect, uri)` 传值。
- **candidate-1：`UnknownFont{id}` / `UnknownImage{id}` 承担「本刀延后」的语义。** 本刀所有 `FontId` 都「未知」是实现现状，不是调用方犯的错；用 `UnsupportedCanvasOperation{operation}`，等字体注册表落地再引入真正的 `UnknownFont`。

三份都没有触碰以下红线，记录备查：无 `unsafe`、无 self-referential crate、无 `Surface::draw_text`、无 `simple-text` / `rayon`、无第二 PDF 后端、无整份 PDF 快照、`publish = false` 保留、`Canvas` trait 未被改动。

---

## 落地前必须先核实的 krilla 0.8.2 事实（三份都在猜）

底座选定后第一步不是写 `KrillaCanvas`，而是钉住这几项——候选 1 明确标了 `// UNVERIFIED`，候选 2 没标但同样在猜：

- `PathBuilder` / `Path` 的模块路径：候选 1 写 `krilla::path::*`，候选 2、3 写 `krilla::geom::*`，二者不能都对。
- `PageSettings::from_wh` 返回 `PageSettings` 还是 `Option<PageSettings>`（决定 `to_page_settings` 是否可失败）。
- 链接注释构造式的确切形状：`LinkAnnotation::new(rect, Target::Action(LinkAction::new(uri).into()))`。
- `PathBuilder::finish` 是否拒绝只含一个 `MoveTo` 的路径——这一条决定移植进来的 `InvalidGeometry` 本刀有无可达的产生者。

三份候选都把内容流算子与 `/Annot` 矩形的字面量当作「先猜、跑绿后以文件为准」，方向一致；候选 3 从 `Transform::from_row(1, 0, 0, -1, 0, height)` 推出的 `10 180` / `40 180` / `40 150`、annot `10 126 90 170` 可以当第一次运行的起始假设，但按它自己的规则：不符就改字面量，不改 `convert.rs`。
