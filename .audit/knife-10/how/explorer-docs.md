### Components Found

角度：纸张 / 边距 / 分节的提案权威，以及它们在现有组装路径里落在哪。不覆盖正文节点、图片、断行。

#### 几何（`src/geom.rs`）——没有纸张枚举

长度是带品牌的 newtype，不是纸张类型。

- `Length<C>`（`src/geom.rs:14-24`）：`repr(transparent)`，内部 `pt: f32`。品牌是空枚举 `Finite` / `NonNegative` / `Positive`（`src/geom.rs:65-75`）。放宽只走 `Positive → NonNegative → Finite`（`src/geom.rs:153-168`）。
- 构造：`Finite::pt/mm`、`NonNegative::pt/mm`（允许 0）、`Positive::pt/mm`（拒绝 ≤0）。`NaN`/±∞ → `Error::NonFiniteLength`；负数 → `NegativeLength`；非正 → `NonPositiveLength`（`src/geom.rs:77-150`，测试 `src/geom.rs:550-607`）。
- 单位：只有 pt 与 mm。`MM_TO_PT = 72.0 / 25.4`（`src/geom.rs:11-12`）。没有 `cm` / `in` 构造器。
- `from_trusted_pt`（`src/geom.rs:46-56`）只给编译期常量。运行时必须走品牌工厂。

`Size`（`src/geom.rs:209-249`）：宽高都是 `Length<Positive>`，所以空尺寸和零尺寸在类型上组不出来。`Size::new` / `try_pt` / `try_mm`。没有方向字段。

纸张常量，仅两个，都是 `Size`，不是枚举：

| 符号 | 定义 | 测试 |
| --- | --- | --- |
| `A4` | `Size::from_trusted_pt(210.0 * MM_TO_PT, 297.0 * MM_TO_PT)`（`src/geom.rs:251-252`） | 与 `f64` 的 `210 * 72 / 25.4`、`297 * 72 / 25.4` 绝对误差 `< 1e-3`；且 `A4 == Size::try_mm(210.0, 297.0)`（`src/geom.rs:530-540`） |
| `LETTER` | `Size::from_trusted_pt(612.0, 792.0)`（`src/geom.rs:254-255`） | 宽高 `to_bits` 分别等于 `612.0f32` / `792.0f32`（`src/geom.rs:544-546`） |

`src/` 里没有 `A3`、`A5`、`PageSize`、`Paper`、`Orientation`（对 `src/**/*.rs` 搜这些名字，纸张枚举零命中；`enum` 只出现在品牌、`PathVerb`、`Error`、`Numbering`、`ImageFormat`、`CanvasCommand`）。

`Margins`（`src/geom.rs:305-363`）：四边 `Length<NonNegative>`。`new(top, right, bottom, left)`，`uniform`。`Default` = 四边 `NonNegative::ZERO`（测试 `src/geom.rs:611-616`）。没有「72pt / 36pt」预置常量或边距枚举。

公共重导出（`src/lib.rs:12-15`）：`Length`、`Finite`、`NonNegative`、`Positive`、`Margins`、`Size`、`A4`、`LETTER`、`MM_TO_PT`。不导出纸张枚举。

#### 文档入口持有一份尺寸、一份边距

`document(page_size: Size, build) -> Result<Rendered>`（`src/document.rs:39-53`）。参数是 `Size`，不是 `PageSize`。闭包跑一次；然后 `paint` 进 `RecordingCanvas`。

`Document`（`src/document.rs:59-88`）字段与纸张相关的只有：

- `page_size: Size`（构造时写入，无 setter）
- `margins: Margins`，`new` 里 `Margins::default()`（四边 0）
- `header` / `footer` / `behind` / `front`：各一个 `Option` 闭包，后一次调用替换前一次（`src/document.rs:127-145`）

`Document::margins`（`src/document.rs:122-125`）整份替换，不跟 `page_size` 比较。没有按边 setter，没有 getter。`Document` `Deref`/`DerefMut` 到 `Flow`（`src/document.rs:275-287`），边距不在 `Flow` 上。没有 `section` 方法。

`paint`（`src/document.rs:147-171`）把这一对收成唯一的 `PaginateConfig { page_size, margins }`，再交给 `paginate_root`。装饰闭包是文档级的一份，不是按节存的。

#### 分页几何：整份文档一个框

`PaginateConfig`（`src/layout/paginate.rs:11-38`）`pub(crate)`：

- `body_width` = 页宽 − 左 − 右
- `body_height` = 页高 − 上 − 下
- `body_origin` = `(left, top)`
- `validate`：宽或高非有限或 `<= 0` → `Error::InvalidGeometry`（无字段）。不在 `margins()` 调用时跑，在 `plan_pages` 开头跑（`src/layout/paginate.rs:70-75`）。

每一页的 `PagePlan.size` 都抄 `config.page_size`（`ensure_content_page` `src/layout/paginate.rs:155-161`；空白页 `src/layout/paginate.rs:177-179`；空文档补一页 `src/layout/paginate.rs:198-204`）。规划器不会在中途换尺寸或边距。

正文原点加上边距：测试 `body_origin_adds_margins`（`src/layout/paginate.rs:660-685`），左边距 10、上边距 15 时字形原点 x=10、y=15+baseline。

#### 装饰：页尺寸已是按页数组，边距仍是一份

`PageInfo`（`src/layout/decorate.rs:36-43`）只有 `number: PageNumber`、`count: PageCount`、`size: Size`、`margins: Margins`。`PageNumber` / `PageCount` 是全文 `NonZeroU32`（`src/layout/decorate.rs:16-34`）。没有 section 计数类型。

`measure_bands(sizes: &[Size], margins: Margins, …)`（`src/layout/decorate.rs:91-137`）：

- `count` = `sizes.len()`（含空白正文页；0 页 → `InvalidGeometry`）
- 页号从 1 递增，扫整份 `sizes`
- 每一页的 `PageInfo.margins` 都是同一个 `margins` 参数
- `PageInfo.size` 来自该页的 `sizes[i]`（布局今天每页相同，但函数签名已经按页给尺寸）
- 页眉带 y=0，高=`margin.top`；页脚带 y=`page_h - bottom`，高=`margin.bottom`

`measure_band`（`src/layout/decorate.rs:180-206`）的框宽是**整页宽** `size.width()`，不是正文宽。行盒高 `> margin` → `Error::DecorationTallerThanMargin { height, margin }`。超宽 → `DecorationWiderThanFrame`，`avail` 是页宽。

零上边距再画页眉文字会失败：`tests/document.rs:259-270`，`margin == 0.0` 且 `height` 接近行盒高。这与提案「默认 0 = 无页眉/页脚带」一致：带高为 0 时装得下的只有空带。

DSL 的左右槽另做内缩（刀 8 决定，见下）：`Band::lower`（`src/document.rs:740-775`）在非零 `margin.left/right` 时插入 `ColumnWidth::Fixed` 空轨，文字轨夹在中间。边距 0 则丢掉该空轨。

`Layer`（`src/document.rs:791-824`）坐标不扣边距。`LayerItem::Image` 注释写明不按页边检查（`src/layout/decorate.rs:59-60`）。

#### 双页码今天只是抄全文

公共 `Page`（`src/document.rs:671-709`）`#[non_exhaustive]`：

- `number` / `count`：全文，从 1 起；`count` 含空白页（字段文档 `src/document.rs:677-680`）
- `section_number` / `section_count`
- `size: Size`
- `margins: Margins`
- `is_last`：`number == count`（全文末页，`src/document.rs:691-696`）。`Layer::is_last` 转发它（`src/document.rs:807-810`）

`From<&PageInfo> for Page`（`src/document.rs:699-709`）把 `section_number` / `section_count` **直接写成** `info.number` / `info.count`。注释：「The document is one implicit section, so section counters equal document totals.」（`src/document.rs:673`）。布局层不知道节。

集成测试 `footer_sees_section_counters_equal_to_document_totals`（`tests/document.rs:222-241`）：两页（中间一个 `page_break`），页脚格式 `section_number/section_count`，断言 `"1/2"` 与 `"2/2"`。页脚 y = `A4.height() - 20 + BASELINE`（下边距 20 的带顶 + 基线）。

#### 画布 / 后端已经按页接收 `Size`

`Canvas::start_page(&mut self, size: Size)`（`src/canvas.rs:187-188`）。`paint_pages` 每页 `canvas.start_page(page.size)`（`src/layout/paginate.rs:216-228`）。`RecordingCanvas` 把该 `Size` 记进 `StartPage`，`RecordedPage::size()` 读出来（`src/canvas.rs:213-224`、`312-314`）。

`KrillaCanvas::start_page` 把 `Size` 存在该页上（`src/backend/krilla_canvas.rs:220-230`）。`emit_page` 用**这一页**的宽高：`PageSettings::from_wh(width_pt, height_pt)`（`src/backend/emit.rs:89-96`）。`None` 变成 `Error::Backend("PageSettings::from_wh rejected a positive Size"…)`，不是设计稿里的 `InvalidPageGeometry`。

`to_krilla_size` 是恒等：`KSize::from_wh(width_pt, height_pt)`（`src/backend/convert.rs:20-23`）。不预翻转。注释：krilla 0.8.2 Surface 已是左上、y 向下（`src/backend/convert.rs:1-7`）。

后端测试 `two_pages_with_different_sizes`（`src/backend/emit.rs:301-309`）：先 `start_page(A4)` 再 `start_page(LETTER)`，`finish` 得到 `%PDF`。混合页尺寸在 Canvas/emit 上已经能走通。布局从不产出不同的 `PagePlan.size`。

#### 错误变体（与纸张相关的现状）

`src/error.rs` 有 `InvalidGeometry`（无字段，`src/error.rs:45-46`）、`DecorationTallerThanMargin`、`DecorationWiderThanFrame`。**没有** `InvalidPageGeometry`。设计稿枚举里的那个变体（`docs/quire-design.md:1109`）未落地。

---

### Flow

今天一条文档 = 一个隐式 section。尺寸和边距在组装时定死，分页和装饰不再改它们。

1. 调用方 `document(A4 或任意 Size, |doc| { … })`（`src/document.rs:39-44`）。`A4`/`LETTER` 只是 `Size` 常量。横向若要做，调用方自己交换宽高做成另一个 `Size`；库不记方向。
2. 闭包里可选 `doc.margins(Margins)`，默认四边 0。可选各注册一个 header/footer/behind/front。正文经 `Deref` 到 `Flow` 追加节点。这一刻没有页码。没有 `doc.section`。
3. `paint`（`src/document.rs:147-171`）把 body lower 成一个 `FlowNode::Stack`，把四个闭包包成一份 `Decorations`，构造**一个** `PaginateConfig`。
4. `paginate_root`（`src/layout/paginate.rs:57-68`）：先 `plan_pages`（校验正文框 > 0；每页 `size` 相同；正文框 = 页 − 四边），再 `measure_bands(&sizes, 同一个 margins, …)`，最后 `paint_pages`。
5. `measure_bands` 填 `PageInfo`（全文页号/总页数、该页 `Size`、同一份 `Margins`）。`document::Page::from` 把 section 计数抄成全文计数。页眉/页脚在带内测量；失败则画布还没 `start_page`（`measure_bands` 文档：`src/layout/decorate.rs:90`）。
6. `paint_pages` 叠层：behind → header → body → footer → front（`src/layout/paginate.rs:219-224`）。每页 `start_page(page.size)`。后端 `PageSettings::from_wh` 用该页 pt 宽高。

刀 10 要改的接缝（现状，不是方案）：

- 尺寸：`Document.page_size` 与 `PaginateConfig.page_size` 是标量；`PagePlan.size` 与 `measure_bands` 的 `&[Size]` 已经按页。后端 `start_page` / `emit_page` 已按页。
- 边距：`Document.margins`、`PaginateConfig.margins`、`measure_bands` 的 `margins` 参数都是整份文档一份。`Page.margins` 只是把这份抄进闭包。
- 装饰与页码：四个闭包是文档级 `Option`。`PageInfo` 没有 section 字段；双计数只存在于 `Page::from` 的拷贝。`is_last` 比的是全文。
- 强制新页：现有 `PageBreak` / `PageCursor` 是正文流里的断页（`src/layout/paginate.rs:41-46`、`164-184`），没有「新 section 即使末页未满也换物理页」的节点。

---

### Files Read

- `src/geom.rs`（长度品牌、`Size`、`A4`、`LETTER`、`Margins`、测试）
- `src/lib.rs`（重导出）
- `src/document.rs`（`document`、`Document`、`margins`、`paint`、`Page`、`Band::lower`、`Layer`）
- `src/layout/paginate.rs`（`PaginateConfig`、`plan_pages` 抄尺寸、`paint_pages`、`body_origin_adds_margins`）
- `src/layout/decorate.rs`（`PageInfo`、`measure_bands`、`measure_band`）
- `src/error.rs`（`InvalidGeometry`、装饰错误；确认无 `InvalidPageGeometry`）
- `src/canvas.rs`（`start_page(Size)`、`RecordedPage::size`）
- `src/backend/krilla_canvas.rs`（按页保存 `Size`）
- `src/backend/emit.rs`（`PageSettings::from_wh`、`two_pages_with_different_sizes`）
- `src/backend/convert.rs`（恒等 `to_krilla_size`）
- `tests/document.rs:222-270`（section 计数糖、零边距页眉报错）
- `docs/提案书.md`：抬头与已定项；§1 示例里的 `PageSize::A4`；§2 清单里的 `section` 与 `pageSize`；§2.0 长度；§2.10–§2.12；§5；§7；确认总表与「实现前可再钉」
- `docs/quire-design.md:257-290`、`290`、`316`、`957-969`、`1104-1110`（设计稿纸张；标为未确认产品）
- `AGENTS.md` 文档优先级、概念三分、Agent 工作方式、明确不要做
- `.audit/handoff/knives-8.md`（刀 8 开工前锁定：`geom::A4`，不做 `PageSize`）
- `.audit/knife-8/frame.md`、`.audit/knife-8/arena/synthesis/design.md`、`.audit/knife-8/arena/synthesis/cross-judge.md`（G4 与否决 `Paper` 枚举）、`.audit/knife-8/implement-status.md`
- `.audit/handoff/knives-done.md`（刀 7–9 完成后仍推迟 `PageSize` 与 multi-section）
- `.audit/knife-9/implement-status.md`（全文无 `PageSize` / section / 纸张）
- `.audit/knife-9/arena/frame.md`（本刀范围外：`PageSize` enum、Multi-section）

未把刀 8 candidate-2 的 `PageSize` API 当作现状或已确认产品。交叉评审明确不嫁接它。

---

### Boundaries

#### 提案书已确认、对刀 10 有约束的原文要点

权威顺序：`docs/提案书.md` 抬头（`docs/提案书.md:1-8`）写明设计稿里未经本文件确认的决定先不算数。`AGENTS.md:5-15` 重复：提案书 > 本文件 > 设计稿未确认部分；§1–§9 主线已确认；「预置边距枚举、分隔线最终方法名等小糖可后补」；搁置项不要提前做进公共 API。

§1 示例写成 `document(PageSize::A4, …)`（`docs/提案书.md:45`、`78`）。那是调用形状示意。实现是 `document(Size, …)`。示例里的 `p.bold`、`image_at` 坐标用裸 `f32` 等也未按字面落地（刀 8/9 另有取舍）；纸张这条以 §2.11 / §7 为准，不以示例里的类型名已存在为准。

§2 清单（讨论用，不是签名）：`section`「从这里起换纸张、方向、边距、页眉、页脚、水印、背景」（`docs/提案书.md:148`）。文档级 `pageSize` / `pageOrientation` / `pageMargins`（`docs/提案书.md:175`）。`AGENTS.md:32-44`：这三层不要混成一种节点；「未取舍前不要在 `document` 里发明 API」。§2 主线现已在总表里确认，所以 section 本身是已取舍项，不是发明项。A 系列具体名单仍未钉死（见下）。

§2.0（`docs/提案书.md:208-228`）：内部只有 pt；调用方经长度类型，默认可写 pt，也支持 mm；「后续若需要」才提 cm/in。非负（margin、spacer）与正数（字号）用 newtype，禁止用 `u32` 当 pt。`NaN`/±∞ 构造期拒绝。换算集中，禁止元素里各自乘。合法枚举纸张/方向优先类型化，不要在 layout 各处 `if` 再报 `InvalidGeometry`。跨字段关系（例如边距之和相对页宽）允许在组装/lower 边界查一次。

§2.10（`docs/提案书.md:486-500`），状态「已定」：

- 闭包在绘制前能读页码（含 §2.11 全文与 section 内）和**当页四边 margin**。闭包不改带高、不改正文流。
- 带高 = 上下 margin，不另设高度。正文高 = 页高 − top − bottom；正文宽 = 页宽 − left − right。装不下就报错，不裁切、不加高、不回流。
- 水平：画 header/footer **无视左右 margin**，横跨 `x = 0 … page_width`。与正文左右对齐要调用方用传入的 `margin.left/right` 自己缩进。

§2.11（`docs/提案书.md:502-525`），状态「已定方向」：

| 项 | 约定 |
| --- | --- |
| 名字 | 对外 `section`，不用 `page` |
| 新 section | 强制新物理页，即使上一节末页未满 |
| 文档默认纸张 | `document` 上有默认 page size（含方向）；节未覆盖则用它 |
| 节覆盖 | 节可单独设纸张，优先于文档默认 |
| 尺寸 | **枚举**常用规格（至少含 A 系列如 A3/A4/A5 等，**具体清单实现时列全**）+ **Custom(宽, 高)**（§2.0 → pt） |
| 方向 | 枚举横向/竖向，**只与尺寸枚举搭配**。`Orientation + SizeEnum` → 最终宽高（A4 竖 = 210×297 mm 的 pt，A4 横则对调）。**Custom 已是最终尺寸，不再套方向** |
| margin / header / footer | 随 section；闭包画在该节生效的 margin 带上 |
| 页码 | 两套，都从 1 起。全文当前页/总页数；本节从本节第一页起的序号与节内总页数。字段名可再定。页数在分页完成后填入，带高不依赖这些字符串 |
| 单节糖 | `document` 闭包可糖成隐式唯一 section。全文页码仍可用；节内页码与全文相同 |

§2.12（`docs/提案书.md:527-531`）：主挂在 section；单节时 `doc.behind` / `doc.front` 糖到隐式节。坐标相对当前物理页左上，y 向下；不因左右 margin 自动缩进。可用全文/节页码、`is_last` 等决定本页是否画。叠层：behind → header → 正文 → footer → front。

§5（`docs/提案书.md:596-598`）：与 §2.10、§2.11 对齐。某页关 header/footer **仍搁置**。

§7（`docs/提案书.md:604-610`），状态「已定」：与 §2.11 对齐（枚举 + 方向、Custom、节覆盖文档默认；内部 pt）。并排栏是内容 `columns`，不是整页分栏。**默认页边距 = 0（四边）。** 上下 margin 同时是 header/footer 带高：默认 0 表示无带，正文可顶满页。**预置枚举（可选）**：常见边距套装（例：四边 72pt、四边 36pt），名字与数值实现时列；预置只是糖，语义仍是四个长度。

确认总表（`docs/提案书.md:632-664`），日期 2026-09-30：

- §2.1–2.13 要点含：「header/footer=上下 margin；section 强制新页+双页码+纸张枚举/方向」
- §7：「默认 margin **0**；预置边距枚举可选」
- 搁置：某页关闭 header/footer
- **实现前可再钉、非阻塞**：预置边距枚举具体有哪几档；**A 系列枚举完整列表（A3/A4/A5…）**

`AGENTS.md:131-134`：先读提案书已确认条，设计稿只取仍有效的技术核实。主线确认后 DSL 以提案书为准；预置与搁置项不提前做。`AGENTS.md:140-148`：不暴露 `Flowable`；不做/延后项不抢跑；不并行开页或画页。纸张枚举不在「不要做」列表里；它在提案 §2.11/§7 已定，完整 A 名单在「可再钉」。

#### 设计稿（未确认产品，不得当刀 10 规格）

`docs/quire-design.md:257-267` 的技术数字与现码一致、可当换算事实：`MM_TO_PT = 72/25.4`；A4 真值是 `210*72/25.4` 与 `297*72/25.4`，不要用「595.28 × 841.89」当第二来源；测试绝对误差 `< 1e-3`。现码测试就是这条（`src/geom.rs:530-540`）。`LETTER` 612×792 与现码一致。

下列是设计稿产品，**提案已改掉或未实现**，标为未确认：

- 默认边距四边 **72 pt**（`docs/quire-design.md:267`）。提案 §7 定为 0。现码 `Margins::default` 是 0。
- 边距违反时 `Error::InvalidPageGeometry`（`docs/quire-design.md:267`、`1110`）。现码正文框 ≤0 用无字段的 `InvalidGeometry`（`src/layout/paginate.rs:31-37`）。
- 页眉带 x 从边距起、宽=正文宽（`docs/quire-design.md:271-278` 表：页眉带 x=72, width=451.27…）。提案 §2.10 是横跨整页。现码 `measure_band` 用整页宽。
- `PageSize { A4, Letter, Custom(Size) }`（`docs/quire-design.md:957-962`）没有方向、没有 A3/A5。与 §2.11「枚举 + 方向，Custom 不再套方向」不是同一形状。
- `PageSettings::from_wh` 的 `None` 映射为 `InvalidPageGeometry`（`docs/quire-design.md:290`、`316`）。现码页面这条是 `Error::Backend(String)`（`src/backend/emit.rs:90-96`）。`to_krilla_size` 的 `None` 才是 `InvalidGeometry`，用于路径/矩形而不是开页。
- 「v1 没有旋转页面」（`docs/quire-design.md:290`）与 §2.11 的横向枚举冲突；以提案为准。适配器事实仍在：开页就是 `from_wh(width, height)`，横向在布局侧变成对调后的宽高，不另传旋转标志。这一点现码 `emit_page` 已是这样。

krilla 坐标恒等、不预翻转：`AGENTS.md:26` 与 `src/backend/convert.rs:1-7` 一致，当作技术事实。提案「你已经定过的」第 7 条写「在画布出口再转换」（`docs/提案书.md:22`），实现已按设计稿核实改成不翻转。页尺寸数字因此原样进入 `PageSettings`。

#### 刀 8 / 刀 9 交接里和纸张、分节有关的推迟

刀 8 **开工前**（`.audit/handoff/knives-8.md:34`）：锁定 `geom::A4`，不是 `PageSize` 枚举；页眉左右槽按边距内缩（`full-bleed-slots` 会反过来）；不发明 Paper builder。

刀 8 frame（`.audit/knife-8/frame.md:22-24`、`44-50`）：单隐式节，多节可以等。页尺寸复用 `geom::Size` / `A4`（「enum PageSize can thin-wrap A4/LETTER + Custom」是 frame 的可选说法，不是提案的 A 系列清单）。默认边距 0。header/footer 收到全文页码；单节时 section 计数等于全文。

刀 8 synthesis（`.audit/knife-8/arena/synthesis/design.md:11-12`、`24`）：嫁接「`Page` 带 section 计数，单节时等于全文」。纸张一行：「Reuse `geom::Size` / `A4` / `LETTER`. No `PageSize` enum until A-series list is pinned.」反向词 `page-size-enum`。

交叉评审（`.audit/knife-8/arena/synthesis/cross-judge.md:35`、`82`、`92`）：candidate-2 的 `Paper::{A3,A4,A5,Letter}` 被扣分，因为提案把 A 系列清单标成「实现前可再钉」。**不嫁接**该 `Paper` 枚举。G4 要求加上 `section_number` / `section_count`，单节等于全文，并保留 `#[non_exhaustive]`。未决第 1 条：§2.10 说调用方自己用 margin 缩进，但当时两个候选的页眉 API 都给不出缩进手段。

刀 8 实现状态（`.audit/knife-8/implement-status.md:40-44`、`53`）：采用了内缩槽（synthesis 的 `full-bleed-slots` 取反）：`[Fixed(left)?, 左文, 右文, Fixed(right)?]`，边距 0 丢掉空轨。`Page` 有 `number`、`count`、`section_number`、`section_count`、`size`、`margins`，`#[non_exhaustive]`。仍推迟：`PageSize` 枚举（以及 `p.bold`、style builders、当时的 `image_at`、带中槽）。`image_at` 已由刀 9 补上；`PageSize` 没有。

刀 9 frame 范围外（`.audit/knife-9/arena/frame.md:39-45`）：`PageSize` enum、Multi-section。`.audit/knife-9/implement-status.md` **没有**写纸张或 section；它只记图片路径。推迟清单在完成后的交接：`.audit/handoff/knives-done.md:22` 仍列 `PageSize` enum；`:30` 下一步含「section multi」。

---

### Non-Obvious Things

- 双页码字段已经在公共 `Page` 上，测试锁的是「永远等于全文」。布局 `PageInfo` 没有对应字段。刀 10 若让两套计数分叉，必须改 `Page::from` 的拷贝（`src/document.rs:704-705`），并给 `measure_bands` 的全文编号（`src/layout/decorate.rs:97-109`）旁边加节内编号。只改 DSL 不改 `PageInfo` 做不到分叉。
- `measure_bands` 已经接收 `&[Size]`，后端 `start_page` 已经按页。缺的是规划器始终写入同一个 `config.page_size`，以及边距/装饰/页码不是按页或按节。混合尺寸的 `%PDF` 冒烟在 `emit.rs` 里已有，但是手写两次 `start_page`，不是从 `document()` 来的。
- `Document::margins` 不校验是否吃光页面。失败发生在 `plan_pages` → `PaginateConfig::validate`，错误是无字段的 `InvalidGeometry`。§2.0 允许这种跨字段检查放在边界，但没规定变体名。设计稿的 `InvalidPageGeometry` 不存在，不要当成现在的错误。
- 页眉测量框是整页宽（符合 §2.10），左右**文字槽**却被刀 8 用 Fixed 空列内缩到正文缘（`.audit/knife-8/implement-status.md:40`）。这是对交叉评审「未决第 1 条」的实现选择，不是设计稿那张「页眉带 x=边距」表。节级边距会同时改变正文框和这两条空列。带的 y/高仍只看上下边距。
- `is_last` 只比较全文 `number == count`。§2.12 允许闭包用它决定画不画，但没写「节的最后一页算不算 last」。现状没有节末标志。
- 字段名：提案说「名可再定」。代码已公开 `section_number`、`section_count`、`number`、`count`，并且 `Page` 是 `#[non_exhaustive]`。集成测试依赖这些名字的值，不依赖 Display 格式。
- 预置边距套装（72/36）在 §7 是可选糖，总表「可再钉」，`AGENTS.md:13` 写明可后补。现码没有。默认 0 已实现，不是可选。
- A4 的 const 用 `from_trusted_pt(210 * MM_TO_PT, …)`，跳过 `Positive::pt`。运行时等价路径是 `Size::try_mm(210, 297)`，测试断言两者相等。自定义纸张今天就是调用方传入的 `Size`（`try_pt` / `try_mm` / `new`），没有 `PageSize::Custom` 变体。
- 刀 8 candidate-2 写过 `PageSize::Paper(Paper, Orientation)` 与 `Custom`，横向 = `Size::new(height, width)`，Custom 不再套方向（`.audit/knife-8/arena/candidate-2/design.md` 约 108、582-610 行）。交叉评审不嫁接。那不是代码，也不是被确认的 API 形状；和提案语义同向的只有「方向只配枚举、Custom 是最终尺寸」这句产品规则本身。
- `src/` 搜不到 `fn section`。单节糖是「根本没有 section 构建器」，不是「有一个默认 section 对象」。

---

### Open Questions

产品规则里写明留给实现、代码也还没定的，只列与纸张/边距/节计数有关的：

1. A 系列枚举的完整名单。提案只举例 A3/A4/A5，总表写「实现时列全 / 可再钉」。刀 8 因此整份 `PageSize` 都没做。刀 10 要做节覆盖纸张时，名单仍没有确认文本。
2. 预置边距枚举有哪几档、叫什么。§7 举例 72pt 与 36pt，但是可选，且「实现时列」。与默认 0 不是一回事。
3. `is_last` 在多节时是全文末页，还是也包括本节末页。现状是全文。提案只点名 `is_last`，没定义节语义。
4. 节内页码字段的最终名字。提案说可再定；公共 API 已经用了 `section_number` / `section_count`。
5. 页眉左右槽内缩是刀 8 的实现选择（对 §2.10「调用方自己缩进」的缺口）。多节换边距时这条会跟着变。提案正文仍写横跨全宽、缩进由闭包自己做。没有更新过的确认条文把内缩改成产品规则。
6. 边距之和 ≥ 页宽/页高时的错误类型。现码是 `InvalidGeometry`。设计稿的 `InvalidPageGeometry` 未采用。提案只要求边界上拒绝，没指定变体。
