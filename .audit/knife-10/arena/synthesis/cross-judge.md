# Knife 10 cross-judge

只读评分。没有改写两份设计，没有改 `src/`。基线仍是 `feat/section` @ `a12b610`。对照了提案书 §2.10 / §2.11 / §2.12 / §7，以及今天的 `document(Size, …)`、`PagePlan { size, placed }`、`plan_pages` 的三个只给页盖 `config.page_size` 的辅助函数、`measure_bands` 的「一份边距 + 一份装饰」。

C1：`Document` 持有 `lead` + `Vec<Section>`，`PagePlan` 打上 `SectionIndex`，`document(impl Into<PageSize>)`。
C2：`DocumentPaginator` + `PageStream` / `SectionSpan`，`PagePlan` 不加字段，`document(PageSize)`。

## 分数

| # | 标准 | C1 | C2 |
| --- | --- | ---: | ---: |
| 1 | 提案贴合 | 4 | 5 |
| 2 | 兼容 | 5 | 4 |
| 3 | 领域模型 | 4 | 4 |
| 4 | 分页接缝 | 4 | 5 |
| 5 | 公共面深浅 | 4 | 4 |
| 6 | 延期纪律 | 5 | 5 |
| | **合计** | **26** | **27** |

## 逐条

### 1. 提案贴合

两边都写全了 §2.11 / §7 的行为：对外名 `section`；新节另起物理页，靠每节各跑一次现有 `plan_pages`，不往游标里塞 `PageBreak`；文档默认纸、节可覆盖；A0–A6 + Letter + Legal，外加 `Custom`；方向只和规格枚举配对；margin / header / footer / behind / front 挂在节上；单节时 `doc.*` 糖到隐式节；两套页码从 1 起，全部节排完再填；默认边距 0；`is_last` 仍是全文末页；节末用 `section_number == section_count`。预置边距枚举两边都没做（总表标成可选，标准 6 也要求不做）。

C2 多一分，因为文档默认纸和节覆盖是同一种 `PageSize`。横向只从 `StandardSize::landscape` 来，`PageSize` 上没有会在 `Custom` 上失败的 `landscape()`。这和「默认纸张含方向、Custom 已是最终尺寸」是同一道门。

C1 扣一分，因为 `From<Size> for PageSize` 把 `document(A4)` 收成 `Custom`。`Paper::A4` 竖向和 `geom::A4` 点值逐位相同，变体却不同。提案要的默认纸是「规格 + 方向」或 Custom，不是两条都能编译、以后要认规格名就会分叉的入口。行为上仍能画出对的页，所以不是 3 分。

### 2. 兼容

C1 是 5。`margins` / `header` / `footer` / `behind` / `front` 签名不动，转调隐式首节。`document(A4)` 和 `document(page)`（`page: Size`）继续能编过。`tests/document.rs` 里 24 处 `document(` 不用改参数，`footer_sees_section_counters_equal_to_document_totals` 仍测同一件事。`paginate_root` 留着，layout 测试继续从一个入口进来。

C2 是 4，不是 5。单节时那五个方法仍写在隐式节上，节计数等于全文是因为唯一的 span 盖住全部页，别名分支删掉之后断言仍然成立。对 `RecordedPage` 尺寸继续拿 `geom::A4` 比，测试没有被掏空。但入口不再收 `Size`。现有调用里不只有 `document(A4)`，还有 `Size::try_pt(...)` 和 `document(page)`。这些要改成 `PageSize::A4` 或 `PageSize::custom(...)` 才编得过。断言还在，套件不能原样留下。

### 3. 领域模型

两边都是 4。`Custom` 加方向在类型里都写不出来。节最后都是「一份配置 + 一条流」。都没有 5，因为都不是从一开始就只存一条 `Vec`。

C1 的名词最干净：一个 `Section` 带纸张覆盖、边距、四个钩子、正文。交给分页时收成一列。弱点是 `lead` 和 `sections` 可以同时有内容，非法混写要等到 `document` 返回前才报；`margins: Option` 既表示「四边 0」，又充当首节有没有被写过的脏标记。

C2 的 `BodyCursor` 用枚举把隐式草稿和已封口的节列表拆开，混写状态不能同时存在。`PageSize::Custom(Size)` 没有方向字段，方向也不做成会在运行期失败的方法。弱点是同一节要经过 `SectionDraft` → `SectionJob` → `SectionSpan` 三份记录，封口后还要一条只负责拒收的 sink `Flow`。清单清楚，份数偏多。

### 4. 分页接缝

今天 `plan_pages` 用一份 `PaginateConfig` 算出正文宽、高、原点，三个辅助函数只把 `config.page_size` 写进 `PagePlan.size`。`measure_bands` 对所有页共用一份边距和一份装饰。`paint_pages` 已经按页 `start_page(page.size)`。换节若只改页尺寸、不换那一节的正文框，页脚位置会对、断行框仍是上一节的。

C2 是 5。`plan_pages` 和 `PagePlan` 的字段不动。驱动在外面：每节一个 `SectionJob`，各带自己的 `PaginateConfig`，所以正文框和页尺寸一起换。节的范围、边距、装饰只放在 `SectionSpan` 上。`PageStream::new` 检查区间首尾相接并盖满页流。两套页码在 `plan_all` 之后由 `measure_flat` 填写，钩子才跑。`paint_pages` 仍只 zip 页计划和带，不读 span。`measure_bands` 收成单 span 适配，公式只有一处。失败仍在 `start_page` 之前。

C1 是 4。算法是对的：每节一份 `SectionInput`，游标从 `Fresh` 起，节末的 `PendingBlank` 由该节自己的 `finalize_eof` 丢掉，计数在全部 `PagePlan` 追加完之后用连续段算，`paint_pages` 不改。扣分是因为 `SectionIndex` 被加进 `plan_pages` 和那三个只盖页尺寸的辅助函数，标签对断行没有用。节长靠 `chunk_by` 相等下标恢复，中间没有覆盖检查；以后若有人把页插进别的节中间，节计数会静默裂开。`paginate_root` 今天接 `&Decorations`，`SectionInput` 按值持有装饰，借用怎么收，设计自己留成未决项。

### 5. 公共面深浅

两边都是 4。krilla 不出 backend，`Flowable` / `FlowNode` / `PaginateConfig` 不进公共重导出。分页复杂度都在 `paginate` / `decorate` 后面。

C1 的调用面更小：`Paper`、`Orientation`、`PageSize`、`section`、一个错误变体。扣分是因为签名和用法对不上。`section` 的闭包是 `FnOnce(&mut Section)`，不返回 `Result`，用法示例里的 `NonNegative::pt(...)?` 编不过。今天的 `document` 闭包是返回 `Result` 的；节闭包把可失败的长度构造关在外面，调用方得先在节外把边距算好。这是公共面的洞，不是内部细节。

C2 的调用面更大：`StandardSize` 与 `PageSize` 上各有一排竖向名字，另外还要公开 `A0`–`A3`、`A5`、`A6`、`LEGAL` 这些 `Size` 常量，理由是给断言用。`DocumentPaginator`、`SectionSpan`、`PageStream` 保持 `pub(crate)`，所以多出来的是纸张名字，不是分页器。节闭包返回 `Result`，和 `document` 一致，示例里的 `?` 能用。两边的洞份量相当，所以同分，不因为 C1 名字更少就再加一分。

### 6. 延期纪律

两边都是 5。都没有样式 builder、没有把页眉改成横跨全宽、没有奇偶换页、没有 path 层、没有按页关掉页眉、没有 fit/cover、没有具名图片字典。`Band::lower` 的左右槽内缩保持刀 8，并说明内缩随该页所属节的边距走，不用改槽的实现。

## 推荐底本

**用 C2 当底。** 合计 27 对 26。多出来的那一分落在以后改代码时最容易把不变量改破的两处：纸张类型（标准 1）和 plan/measure 的切分（标准 4）。

维护者往后加的是「这一节的框、边距、装饰、页数」，不是「这一页再带一个节号」。C2 把这些放在 `SectionJob` / `SectionSpan` 上，`plan_pages` 仍然是「一份 config、一棵树」。区间盖不住页流会在 `PageStream::new` 报 `InvalidGeometry`，而不是靠连续段猜测。计数只有 `measure_flat` 一处，且在全部节排完之后。`Size` 只表示已经 resolve 的页框，规格名和方向不会和 Custom 混成同一个变体。

C1 的兼容领先是真实的，也是它只输一分的原因。`impl Into<PageSize>` 让今天的测试不用动，代价是 `document(A4)` 和 `Paper::A4` 从此是两种值。以后若要按规格名分支，这道口子关不上，除非再破一次调用方。`chunk_by` 同样是一道靠约定站住的口子。这两处都不适合当以后往上加节功能的地基。

若把标准 2 读成「现有测试必须原样编译」，C2 的兼容会落到 3，合计打成 26 平。平局时题目要求在更干净的边界和更小的 API 之间选。这里选边界：不变量住在纸张类型和 span 上，更小的 API 可以从输家摘过来，不必改用 C1 的打标签方案。

## 值得从 C1 摘来的

摘的是约束和测试，不是 C1 的数据结构。

1. **规格枚举标 `#[non_exhaustive]`。** 总表把 A 系列完整名单留在实现前可再钉。C2 把它写成闭集，下一次加规格就是破坏性改动。
2. **不要为断言新增公共 `Size` 常量。** 保留现有 `A4` / `LETTER`，用 `PageSize::resolve` 的 `to_bits` 对它们。C2 提议的 `A0`–`A3`、`A5`、`A6`、`LEGAL` 是调用面膨胀，标准 5 已经为此压分。
3. **带上 C1 第 4 步的集成测试清单。** 两节 2 页 + 1 页时页脚依次是 `1/3·1/2`、`2/3·2/2`、`3/3·1/1`；第二节左边距改变时，页眉左槽 x 等于这一节的左边距；空节仍是 1 页且装饰照画；节末单独一个 `page_break` 不多页；组装错误时 `pages()` 为空。C2 的下一步停在几何 `to_bits`，这些断言还没有落点。
4. **五个糖方法转调隐式节，不复制逻辑。** C2 已经是这个语义；C1 把「不复制」写成了实现约束，值得留下。

不要摘这两样：

- `impl Into<PageSize>` / `From<Size>`。它会把 C2 故意分开的「规格」和「最终页框」重新合成两个变体。
- `PagePlan.section` 和 `chunk_by`。节身份已经在 span 上，再打一份标签就是 C2 自己否决过的双份身份。

## 分歧与标准里含糊的地方

- **标准 2 的「现有测试仍然有意义」可以读成两件事。** 一是断言还在测同一条排版事实（本评分：C2 得 4）。二是测试文件不用改还能编过（那样 C2 得 3，总分打平）。`document` 的第一个参数不在标准点名的五个方法里。本评分不把 `document(Size)` 的源码兼容算进提案贴合，只算进兼容，避免同一件事扣两次。
- **「最小改动」指少改 `plan_pages`，还是少造类型。** 本评分按少改现有规划函数计。C1 的新类型更少，但改的是不需要节号的辅助函数。若把「少造类型」当成最小，标准 4 会打成平局，C1 合计反超。题目把推荐系在「以后扩展还不把不变量改破」上，所以没有按类型个数改分。
- **`doc` 上写了正文再调用 `section`，提案没有钉。** 糖只写了单节文档。C1 整份文档二选一，混写是 `SectionMixedWithDocumentContent`。C2 把写过的钩子和正文留在第 0 节，哪怕没有正文也占一页；封口之后再写才是 `ContentOutsideSection`。两边都满足「不调用 `section` 时五个方法仍可用」。空白前言那一页是 C2 的锐边，不是提案违背。本评分没有为这件事单独加减分。
- **C1 的用法和签名不一致。** 这是设计缺陷，算在标准 5，不把它当成 §2.11 行为缺失。若改用 C1 当底，第一刀应先把节闭包改成返回 `Result`，否则示例本身不能当验收。
- **`is_last` 要不要表示节末。** 两边都不加谓词。§2.12 点了 `is_last`，没有点节末。这里没有分歧。