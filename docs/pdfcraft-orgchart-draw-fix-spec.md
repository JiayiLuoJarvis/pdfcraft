# pdfcraft-orgchart 绘制修复规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-30 |
| 状态 | 已实现，随 `pdfcraft-orgchart` `0.1.1` 发布。本文仍是该次绘制合同，不含实现步骤 |
| 对照 | Python `pdfcraft-equity`，提交 `50852663e2e03c8636400a75505ea0af838ca647` |
| 上位 | [`pdfcraft-orgchart-spec.md`](pdfcraft-orgchart-spec.md) 的协议、页角色、文案、错误码。[`pdfcraft-orgchart-structure-spec.md`](pdfcraft-orgchart-structure-spec.md) 的模块方向和 `count_units` 上限 |

调用方拿到的 PDF 在比例位置、续页箭头、投资侧比例高度、描边颜色、标题位置和虚框外观上要与该提交的 Python 包一致。公共函数签名不变。`validate_request`、`generate_pdf`、`document_title` 的参数和 `ErrorCode::as_str` 不改。

页眉主标题和副标题的字面已经对齐，不在本次范围。整图和股东首页是 `股权架构图: {主体}`。投资首页是 `对外投资: {主体}`。同侧续页主标题是 `续页{n}: {主体}`。副标题继续用现有 `captions` 函数。

冲突时按下面的顺序。

1. 协议、错误码、页眉字面，以上位规格为准。
2. 本文写明的绘制数值和求和，以本文为准。上位规格第 12 节没有列出的颜色和标题间隙，以本文补上的常量为准。
3. `max_cols` 的 64 上限，以结构规格为准。不把上限改成 Python 的无上限 `int`。

## 0. 条目

必须改的行为如下。

| 条目 | 现在 | 改完 |
| --- | --- | --- |
| 比例横坐标 | `draw_ratios` 用框心加 1.2 mm，没有 stub 也画 | 贴 stub 右侧 1.2 mm。没有贴框 stub 不画 |
| 投资侧比例纵坐标 | 用整段行高，多了 `LINE_GAP_PT` 的 2 pt | 框上沿外，只留 descent 和 0.4 mm |
| 左右续页 | `needs_leader` 写了，绘制不读 | 左右放置时画指向锚点的虚线箭头 |
| 描边颜色 | 节点、边、续页框都是 `rgb(20, 20, 20)` | 节点框和续页框黑色。边和引线是 0.2 灰 |
| 标题位置 | 主标题在 `margin + 1` mm，副标题紧贴主标题行高 | 顶间隙 0。主副标题之间再加 1.5 mm |
| 超长标题 | 按全宽居中，不截断 | 超出可印宽度时截断并加 `…` |
| 续页虚框 | 0.8 pt，圆角 2.2 mm，文字靠左内边距 | 1.0 pt，圆角 1.0 mm，槽内水平居中 |
| 折叠求和 | 左折叠 `sum` | 与 CPython `math.fsum` 相同 |
| 稀疏分桶 | 按 0.05 累加 | `int(gap / 0.05)`，向零截断 |

保持不动的行为如下。

| 条目 | 决定 |
| --- | --- |
| `max_cols` 上限 64 | 保留。先用测试钉住竖版 4 列、横版 6 列，以及原始列数大于 64 时结果仍是 64。循环可以换成向零截断后再 `min(64)`。禁止四舍五入，禁止拿掉上限 |
| 边箭头的张角 | 不改。现有半宽系数 0.45 不是本次合同。续页引线箭头另按第 4 节绘制 |
| 页眉字面、错误码、字体只收字节、不写 `/Info` | 不改 |
| 公共 API | 不新增必须由调用方使用的类型 |

坐标都是版面毫米，原点在左上，y 向下。基线是字形基线，不是槽顶。`draw_text` 若仍接收槽顶，放置结果里的基线要在绘制前换算，禁止再套一次整段行高。

## 1. 比例标签

放置结果是一个私有结构，绘制只消费它。

```text
PlacedRatioLabel
  node_id
  text
  x_mm          贴框 stub 的 x，再加 RATIO_LABEL_OFFSET_X_MM
  baseline_y_mm 字形基线
```

`render` 可以新增只依赖 `page`、`measure`、`units`、`error` 的模块来产生这个结构。该模块不依赖 `layout` 模块，也不依赖 `paging`。`render` 绘制这个结构。禁止在绘制函数里再用框心计算 x。

规则与 `annotations.py` 的 `plan_ratio_labels` 相同。

1. 同一 `node_id` 只留第一条 fact。
2. 页主体，以及升格后 `is_subject` 的框，不画。
3. 节点不在本页放置结果里，不画。
4. 股东贴边 `LabelHug::Bottom`。候选边是 `from_id` 等于该节点的边。投资贴边 `LabelHug::Top`。候选边是 `to_id` 等于该节点的边。
5. `preferred_parent` 有值时，该父边排在其余候选边之前。
6. 先找竖向 stub。股东看折线起点。起点贴在框底中心，并且第一段竖直，x 取起点 x。投资看折线终点。终点贴在框顶中心，并且最后一段竖直，x 取终点 x。
7. 竖向 stub 都不存在时，只有股东侧再找侧向 stub。起点在框左缘或右缘，y 在框垂直中线，并且第一段水平，x 取起点 x。投资侧没有侧向 stub。
8. 上面都没有，则该 fact 不产生标签。
9. 贴边容差是 0.05 mm。竖直指 x 差不超过容差且 y 差超过容差。水平相反。

基线如下。`ascent_mm` 和 `descent_mm` 取比例字号 9 pt。`descent_mm` 是基线以下的正距离。

- 股东。`baseline_y = node.y + node.height + ascent_mm + RATIO_LABEL_GAP_Y_MM`。
- 投资。`baseline_y = node.y - descent_mm - RATIO_LABEL_GAP_Y_MM`。

`RATIO_LABEL_GAP_Y_MM` 保持 0.4。投资侧禁止使用 `line_height_mm`。行高含 2 pt 行隙，用它会把标签整段抬高。

验收使用提交 `50852663` 的 `tests/test_annotations.py` 里这些观察结果。误差不超过 `1e-4` mm。

| 观察 | 值 |
| --- | --- |
| 一条竖边的两端 | 股东 `40.00%` 在 x `11.2`、基线 `14.4`。投资 `100.00%` 在 x `11.2`、基线 `58.6`。页主体无标签 |
| 两个股东共用主干 | x 为 `11.2` 和 `51.2`，不是主干的 x |
| 两个投资子节点 | x 为 `11.2` 和 `51.2`，基线都是 `38.6` |
| 没有贴框 stub | 标签集合为空 |
| 重复 fact | 只留一个标签 |

侧向出口的 x 是该段起点 x 加 1.2。基线仍用股东公式，不把测试字体上的 `4.4` 写成常量。

`pdftotext` 在没有贴框 stub 的页上抽不到该百分比。有竖向 stub 的页上抽得到 `40.00%` 这类字面。

## 2. 续页虚框和引线

槽位几何保持 `cues.rs` 的现有规则。股东优先在锚点上方，投资优先在下方。挡道时先撑行距，再改到左右。左右放置时 `needs_leader` 为真。

绘制读 `needs_leader`。为真时画一条从虚框指向锚点的引线，再画箭头。为假时不画引线。

引线端点是两个矩形相向边的中点。虚框是矩形 A，锚点节点框是矩形 B。比较中心的 dx 和 dy。`abs(dx) >= abs(dy)` 时走左右边，否则走上下边。方向指向 B。

引线样式如下。

| 项 | 值 |
| --- | --- |
| 颜色 | 0.2 灰，字节 `51, 51, 51` |
| 线宽 | `CONT_CUE_LINE_WIDTH_PT`，1.0 pt |
| 虚线 | 4 pt 实、3 pt 空 |
| 箭头长度 | `ARROW_SIZE_PT * 0.75`，即 4.5 pt |
| 箭头张角 | 轴线两侧各 30 度 |
| 箭头填充 | 与引线相同的 0.2 灰，不描边 |

先画虚线，再画实心箭头。箭头不被虚线样式带上。

虚框本身如下。

| 项 | 值 |
| --- | --- |
| 描边颜色 | 黑色 `0, 0, 0` |
| 线宽 | 1.0 pt |
| 圆角 | `CONT_CUE_CORNER_MM`，1.0 mm |
| 虚线 | 4 pt 实、3 pt 空 |
| 填充 | 无 |
| 文字 | 黑色，6 pt，在槽宽内水平居中 |
| 第一行基线 | `slot.y + CONT_CUE_PAD_MM + ascent_mm(6 pt)` |
| 第二行 | 再下移 `line_height_mm(6 pt) + CONT_CUE_LINE_GAP_MM` |

禁止再用节点圆角 2.2 mm 和普通框线宽 0.8 pt 画续页虚框。禁止把文字画在 `slot.x + CONT_CUE_PAD_MM`。

链接矩形仍是虚框外接矩形，目标页不变。

## 3. 标题位置和截断

常量如下。放在 `units.rs`，`pub(crate)`。

| 名 | 值 |
| --- | --- |
| `MAIN_TITLE_TOP_GAP_MM` | 0 |
| `TITLE_LINE_GAP_MM` | 1.5 |
| `EDGE_GRAY` | 0.2。绘制字节用 `51` |
| `CONT_CUE_LINE_WIDTH_PT` | 1.0 |
| `CONT_CUE_CORNER_MM` | 1.0 |

主标题槽顶 y 等于 `margin_mm + MAIN_TITLE_TOP_GAP_MM`。副标题槽顶 y 等于主标题槽顶加主标题 `line_height_mm`，再加 `TITLE_LINE_GAP_MM`。两者都在可印宽度内水平居中。可印宽度是 `printable_width_mm`，不是页宽。

截断只作用于页眉主标题和副标题。宽度不超过可印宽度时原文绘制。超过时用字符二分，保留前缀并追加 `…`（U+2026）。比较和切片按 Unicode 标量，不按字节。省略号本身宽于可印宽度时画空字符串。框内文字、比例、续页虚框文字、页脚不截断。

短于可印宽度的标题，水平中线仍在页面中线。

## 4. 描边颜色

一种描边颜色画所有线的现状废止。按对象分开。

| 对象 | 颜色 |
| --- | --- |
| 节点框描边，含主体和 overflow | 黑色 |
| 框内文字、标题、页脚、比例、续页文字 | 黑色 |
| 公司填充 | 保持 `COMPANY_SHADE` `247/255` |
| 人物填充 | 白色 |
| 边线 | 0.2 灰 |
| 边箭头填充 | 0.2 灰。现有填充已是 `rgb(51, 51, 51)`，保持该字节，并与边线使用同一常量 |
| 续页虚框描边 | 黑色 |
| 续页引线及其箭头 | 0.2 灰 |

节点线宽保持主体 1.5 pt，普通节点 0.8 pt。overflow 仍只把节点描边画成虚线。边线宽保持 0.7 pt。

## 5. 折叠求和

`prepare` 里被折叠子节点的比例和，结果与 CPython `math.fsum` 相同。禁止用 `Iterator::sum` 作为该和。

实现放在 `prepare` 内部，不新增依赖。输入在进入求和前已经是有限值。求和结果仍须落在 `0..=100`，否则 `INVALID_PROTOCOL`。

验收用六笔相同比例。每笔是 `100.0 / 6.0`。左折叠和是 `100.00000000000001`，会拒绝。`math.fsum` 是 `100.0`，必须接受。这组数来自 2026-10-30 在 CPython 3.14.7 上的实测。

另保留一笔和明显大于 100 的用例，结果仍是 `INVALID_PROTOCOL`。

`top_n` 为 `None` 或 `0` 时不求和。投资侧不折叠，因此不走这条和。

## 6. 稀疏分桶

`family.rs` 的 `sparse_buckets` 改为向零截断。

```text
gap <= 0        → 0
否则            → trunc(gap / 0.05) 的非负整数
```

`SPARSE_UTILIZATION` 保持 0.35。`UTILIZATION_BUCKET` 保持 0.05。禁止再按 0.05 累加。

实测差在这两个间隙上。累加结果比截断大 1，会改变分页分数。

| gap | 现累加 | 截断后 |
| --- | --- | --- |
| 0.35 | 7 | 6 |
| 0.30 | 6 | 5 |

`gap <= 0` 时两边都是 0，测试要锁住。

## 7. 列数上限

`PageGeometry::max_cols` 的业务结果保持结构规格的决定。

- 默认竖版，节点宽 38 mm，结果是 4。
- 默认横版，节点宽 38 mm，结果是 6。
- 原始截断值大于 64 时，结果是 64。
- 节距小于等于 0 时，结果是 1。

允许把 `count_units` 的逐 1 累加改成向零截断，再与 64 取较小值。改之前先提交上述四个断言。禁止改成四舍五入。禁止放宽到 Python 的无上限 `int`。A4 默认页到不了 64，放宽会变成另一条装箱合同。

## 8. 测试

断言调用方或页计划能观察到的值。页计划上的毫米、页成员、`needs_leader`、标签基线属于观察结果。私有函数名、模块文件名、调用顺序不是观察结果。

禁止断言压缩后的内容流字节。需要看虚线和链接时，测试把 PDF 解开再读操作数和注释数量。`lopdf =0.45.0` 且 `default-features = false` 只放在 `pdfcraft-orgchart` 的 dev-dependency。库依赖仍是 skrifa 和 krilla。生产路径不 `use lopdf`。

每个修复至少一条会在旧行为下失败的测试。

| 修复 | 失败条件 |
| --- | --- |
| 比例贴 stub | x 仍是框心加 1.2，或没有 stub 仍产出标签 |
| 投资基线 | 基线仍比 `node.y - descent - 0.4` 高一个行隙 |
| 续页引线 | `needs_leader` 为真但没有指向锚点的线段和 4.5 pt 箭头 |
| 虚框 | 线宽不是 1.0 pt，圆角不是 1.0 mm，或文字左边落在槽左加 2 mm |
| 颜色 | 节点描边不是黑色，或边线不是 `51, 51, 51` |
| 标题 | 主标题槽顶不是页边距，或超长标题不以 `…` 结尾 |
| 求和 | 六笔 `100/6` 被拒绝 |
| 分桶 | `gap = 0.35` 得到 7 |

页结构和 PDF 文本按上位规格 §13.4 与 §13.5 补锁。搬这些观察，不搬 Python 函数名。几何截图仍然不做。

必须补上的页结构观察如下。旧实现若与 Python 提交不一致，改引擎去对齐期望，不改期望去迎合当前 Rust。

1. 高度落在舒适带和弹性带之间时，整图是一页横版。超过弹性带时拆开，且每页放下。
2. 直接层溢出留下的孩子、overflow id、续页利用率不低于 0.35。
3. 浅对浅的间隙是 `FAMILY_GAP_MM` 12。深邻或超宽退回后是 `NODE_SPACING_MM` 6。子列中心与父列中心对齐。
4. 同一行多个股东进主体只有一个箭头。bus 的 y 不低于源行底加 `SHAREHOLDER_STUB_MIN_MM`。
5. 股东侧外圈在叶子之上。平衡折行的行长与 Python `test_layout.py` 里已有的行长断言一致。
6. 续页槽在锚点上方或下方的坐标，以及左右放置时 `needs_leader` 为真。槽不与节点、边、其他槽相交。
7. 八个和十四个直接股东的总览页数与朝向。纯高链保持竖版。深链页数小于 8，相邻页有重叠节点。
8. `pdftotext` 抽到主体名、角色页眉、`详见附件`、页脚 `第 1 / 1 頁`。一页图没有「详见」和「附件」。`top_n` 合成节点的名字出现，被折掉的名字不出现。
9. 有续页或 overflow 的页，解开后的注释数量与链接目标页一致。虚线节距是 4 和 3。无续页的单页注释数量是 0。
10. 单页 MediaBox 宽高是 210 mm 和 297 mm 换成 pt，误差小于 0.5 pt。

`examples/equity_chart.rs` 仍只生成文件，不承担断言。

## 9. Rust 写法

只改 `crates/pdfcraft-orgchart`。不改 hcsw 树。不改 `pdfcraft-pdf`、`pdfcraft-forms`、`pdfcraft-post`。

模块依赖保持结构规格。`render` 不依赖 `layout` 和 `paging`。新的比例放置模块若存在，依赖方向是 `page`、`measure`、`units`、`error` 指向它，它不反向依赖 `render`。

生产路径，即 `src/` 里不在 `#[cfg(test)]` 中的代码，遵守下列限制。

- `unsafe` 保持 crate 级 forbid。
- 不写 `.unwrap()`，不写 `.expect()`。失败返回 `Error`，码用现有 `ErrorCode`。
- 不新增 `panic`。新的 `pub` 与 `pub(crate)` 函数若返回 `Result`，文档写 `Errors`。若确实不会 panic，文档写 `Panics` 且正文是不会 panic。
- 新的 `pub` 与 `pub(crate)` 项有 `///`。`missing_docs`、`missing_errors_doc`、`missing_panics_doc` 保持 deny。
- 重要的放置函数加 `#[must_use]`。
- 不新增调用方可见的公共类型。放置结构是 `pub(crate)` 或私有。
- 生产代码的 `use` 不写 glob。类型导入后直接用名字。跨模块函数用模块路径调用。
- 颜色、间隙、线宽进 `units.rs` 常量。禁止在 `render.rs` 再写一套裸的 `20`、`1.0`、`2.2` 来表示这些合同值。
- 布尔参数不增加。贴边继续用 `LabelHug`。引线是否绘制用已有的 `needs_leader`。
- 求和、分桶、列数各只有一处实现。

`#[cfg(test)]` 和 `tests/` 里可以 `unwrap`。测试名写观察结果，例如投资基线不含行隙，而不是写内部步骤。

格式和检查命令如下。小步实现时不要每步全量跑 `cargo test`。按 [`AGENTS.md`](../AGENTS.md) 门禁里的小步检查。几步合并后，以及做完第 10 节最后一步时，再跑下面四条。

```bash
cargo fmt -p pdfcraft-orgchart -- --check
cargo clippy -p pdfcraft-orgchart --all-targets -- -D warnings
cargo test -p pdfcraft-orgchart
cargo doc -p pdfcraft-orgchart --no-deps
```

`cargo doc` 在 `RUSTDOCFLAGS=-D warnings` 下通过。不放宽 clippy。不为了测试给生产函数加 `#[allow]`。

## 10. 实现顺序

每步结束跑 [`AGENTS.md`](../AGENTS.md) 的小步检查，再进入下一步。完整 `cargo test` 按小步检查合并执行。

1. 补常量，拆开描边颜色，加上第 4 节的颜色测试。
2. 改标题槽顶、1.5 mm 间隙和省略号，加上截断测试。
3. 改续页虚框的线宽、圆角和居中，加上引线。槽位测试先锁 `needs_leader`，绘制测试再锁线段和箭头。
4. 引入 `PlacedRatioLabel`，删掉框心 x。补上第 1 节的坐标测试和一条 `pdftotext`。
5. 换折叠求和。先看六笔 `100/6` 的测试变红，再改求和让它变绿。
6. 换稀疏分桶。先锁 `0.35 → 6`，再改实现。
7. 给 `max_cols` 钉第 7 节的四个断言。实现若改成截断，结果必须与钉住的值相同。
8. 补第 8 节的页结构和 PDF 观察。某一条失败时，只修导致该观察偏离 Python 提交的那一处。

第 7 步不得和第 6 步混在同一次行为变更里。列数上限不是分桶缺陷。
