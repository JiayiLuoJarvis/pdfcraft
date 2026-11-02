# pdfcraft-pdf 按页页眉页脚规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-02 |
| 状态 | 对话已定（2026-11-02 审查后补定：新增 `ErrorKind::BandFn`、`fixed` 字体只解析一次、`K: 'static`、函数要求 `Sync`）。§2.2“返回值只有左右槽”已由 [`pdfcraft-pdf-band-marks-spec.md`](pdfcraft-pdf-band-marks-spec.md) 取代 |
| 范围 | `pdfcraft-pdf` 的 `SectionSpec.header` / `footer`。正文块、`behind` / `front`、forms、orgchart、post 不动 |
| 相关 | [`提案书.md`](提案书.md) §1、§2.10、§2.11、§4.1、§5、§12 与文末总表；`spec::{SectionSpec, BandSpec, BandSlot, TextRun}`；`layout::decorate::{PageInfo, measure_bands, measure_band}` |
| 对照 | [pdfmake 0.1 Headers and footers](https://pdfmake.github.io/docs/0.1/document-definition-object/headers-footers/)。页眉页脚可以是固定内容，也可以是按当前页、总页数、纸张尺寸现算的函数 |
| 调用方 | 只改本仓库。不改 hcsw，不改 `/mnt/data/dev/hcsw-pdfcraft` |

本文是实现合同。与现提案书「调用方不写闭包」和「某页关 header/footer 仍搁置」冲突的部分，以本文为准，并在实现前写回提案书。其余已确认条（带高等于上下 margin、不回流正文、`TextRun` 填页码、`PageWhen` 只用于图层）仍然有效。

## 1. 产品一句话

一节的页眉和页脚各是一个可选函数。正文分页完成之后，库按页调用它，传入这一页的页码和最终纸张尺寸。函数返回这一页的 `BandSpec`，或者返回空表示这一页不画。带高仍是这一节的上下 margin。正文框不因返回值改变。

## 2. 范围

### 2.1 做

- `PageBandQuery`。库构造，调用方只读。
- `BandFn<K>`。`fixed` 表示每一页同一份 `BandSpec`。`from_fn` 表示按页计算。
- `SectionSpec.header` 与 `footer` 的类型从 `Option<BandSpec<K>>` 改为 `Option<BandFn<K>>`。
- 页眉和页脚收到同一个查询类型。查询含全文页码、全文页数、本节页码、本节页数、这一页的 `Size`。
- `Ok(None)` 这一页不画该侧。margin 保留。正文起点不变。
- 函数返回的 `BandSpec` 仍走现有的 `lower_band` 与 `measure_band`。`TextRun` 仍由库按这一页填数。缺字、空链接、超高、超宽仍失败。
- 新增 `ErrorKind::BandFn` 与 `Error::band_fn`，供函数报告自己的失败，原错误经 `source()` 取出。见 §6。
- `layout_spec` 的 `K` 增加 `'static` 约束。见 §3.3。
- `BandSpec` 与 `BandSlot` 补 `Clone`。
- 破坏性变更。`pdfcraft-pdf` 从 `0.1.0` 升到 `0.2.0`。仓库内现有页眉页脚初始化改为 `BandFn::fixed`。
- `crates/pdfcraft-pdf/CHANGELOG.md`、rustdoc、`examples/band_fn.rs`。

### 2.2 不做

- 正文块、样式、表格、列表上的函数。`Flowable` 仍不向调用方暴露。
- 函数返回任意正文块、栏、表或流式图片。返回值是 `BandSpec` 的左右槽与绝对定位标记，标记合同见 [`pdfcraft-pdf-band-marks-spec.md`](pdfcraft-pdf-band-marks-spec.md)。
- 查询里放 margin、正文框、已排好的片段。调用方自己写在 `SectionSpec.margins` 上的边距，函数不能改。
- 函数改变纸张、边距、页数或正文断点。
- 按页关掉页眉时把这块高度还给正文。
- `PageWhen` 扩到页眉页脚。图层仍只有 `All` 与 `Last`。
- 除 `BandFn` 外的新 `ErrorKind`。
- `catch_unwind`。
- 第二 PDF 后端。不在 `pdfcraft-pdf` 里依赖 pdf-writer、lopdf、printpdf。
- 改 `pdfcraft-forms`、`pdfcraft-orgchart`、`pdfcraft-post` 的版本或行为。

## 3. 公共 API

### 3.1 `PageBandQuery`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
#[non_exhaustive]
pub struct PageBandQuery {
    pub number: NonZeroU32,
    pub count: NonZeroU32,
    pub section_number: NonZeroU32,
    pub section_count: NonZeroU32,
    pub size: Size,
}
```

字段都从 1 起算。`number` 与 `count` 是整份 PDF 的物理页。`section_number` 与 `section_count` 是当前节。`size` 是这一页套用方向之后的最终宽高，即该节 `page_size.unwrap_or(文档默认).resolve()` 的结果。`Custom` 已经是最终尺寸，不再套方向。

库是唯一构造方。字段公开，便于读取。`#[non_exhaustive]` 让以后可以加只读字段。本规格不加 margin。

`size` 与 pdfmake 的 `pageSize` 对应。pdfmake 文档里的页眉函数写成 `function(currentPage, pageCount, pageSize)`。本库把这三项和本节页码放进同一个结构，页眉和页脚都收它。

### 3.2 `BandFn<K>`

```rust
#[must_use]
pub struct BandFn<K> { /* 私有：Fixed(BandSpec<K>) 或 boxed FnMut */ }

impl<K> BandFn<K> {
    pub fn fixed(spec: BandSpec<K>) -> Self;

    pub fn from_fn<F>(f: F) -> Self
    where
        F: FnMut(&PageBandQuery) -> Result<Option<BandSpec<K>>, Error>
            + Send
            + Sync
            + 'static;
}
```

`Debug` 只打印 `BandFn`，不打印函数体或 `BandSpec`。不实现 `Clone`。

`fixed` 忽略查询，等价于每一页返回 `Ok(Some(spec))`。这是 pdfmake 里直接给字符串或内容对象的写法。它的 `BandSpec` 在 `layout_spec` 组装阶段解析一次，与今天的 `Option<BandSpec<K>>` 相同，见 §4.1。

`from_fn` 的函数由文档持有，不借用调用方栈上的临时值。它可以改自己捕获的状态。库保证调用顺序，见 §4.2。`Send + Sync` 让 `DocumentSpec` 保持今天的 `Send` 与 `Sync`。`FnMut` 改自己的捕获不需要 `Cell` / `RefCell`；捕获 `Rc`、`Cell` 一类单线程类型的函数不被接受。

`SectionSpec` 改为：

```rust
pub header: Option<BandFn<K>>,
pub footer: Option<BandFn<K>>,
```

`None` 表示这一侧整节不存在，函数不会被调用。`Some` 里的函数再决定每一页画不画。`Default` 两侧都是 `None`。

`DocumentSpec` 与 `SectionSpec` 不加生命周期参数。

### 3.3 `K: 'static`

`layout_spec` 的约束从 `K: Copy + Eq + Hash + Debug` 改为再加 `'static`。按页现算的页眉要在分页之后解析字体键，库内装饰持有调用方的 `K`。

枚举、`()`、整数、`&'static str` 都满足。只有借用运行时数据的键（例如对局部 `String` 取 `&str`）不再接受；提案书 §4.1 已禁止用裸字符串作字体名。CHANGELOG 写明这一约束。

### 3.4 `ErrorKind::BandFn` 与 `Error::band_fn`

```rust
// ErrorKind 新增：
/// A header or footer function returned an error.
///
/// The caller's error is the [`source`](StdError::source).
BandFn,

impl Error {
    pub fn band_fn(source: impl Into<Box<dyn StdError + Send + Sync>>) -> Self;
}
```

`Display` 是 `header or footer function failed`，后接库补上的位置。调用方的错误不拼进消息，经 `source()` 取出，与 `Font`、`Backend` 的依赖失败同一写法。`Into<Box<…>>` 接受任意 `Error + Send + Sync` 类型，也接受 `&str` 与 `String`。

函数也可以返回其他已有 kind 的 `Error`，例如自己调用 `LinkTarget::uri` 得到的 `EmptyLink`。库不改写 kind。

## 4. 调用时机

正文先在 `plan_pages` 分页。页数、每页纸张、节边界在此之后才知道。然后 `measure_bands` 按文档页序逐页调用，整份文档只调用一次。

### 4.1 `fixed`

组装阶段（`lower_section`）照今天解析一次 `BandSpec`：字体键、粗斜体面、装饰线度量在此检查。失败每节报一次，位置是该节，与今天相同。分页后每页只做 `lower_band` 填页码。

### 4.2 `from_fn`

同一页先页眉、后页脚。某一侧是 `None` 则跳过该侧。下一页才继续。函数因此可以用捕获的状态累计次数。函数在某页返回 `Err` 后，后续页不再调用。正文分页失败时函数一次也不调用。

查询从现有的 `PageInfo` 取出。`number`、`count`、`section_number`、`section_count`、`size` 原样公开。不把 `PageInfo` 本身公开。

`Ok(Some(spec))` 先按页解析字体键与样式，再交给现有 `lower_band`。字体类错误（`UnregisteredFont`、`BoldFaceRequired`、`ItalicFaceRequired`、`BoldItalicFaceRequired`、`MissingDecorationMetrics`）因此按页报告，位置含节、页与 `header` 或 `footer`。同一错误在多页出现时逐页记录，受 `Errors` 的 64 条上限约束。

`TextRun::PageNumber`、`PageCount`、`SectionNumber`、`SectionCount` 仍按这一页替换。函数自己写成字面量的页码，库不再改写。

`Ok(None)` 或左右槽都空，得到空节点列表。`measure_band` 对空列表已经返回 `Ok(None)`，这一页该侧没有 `PlacedFragment`。不要改这条空列表行为。

`Err(error)` 从库内 `BandContent` 闭包返回。`measure_bands` 里的 `ctx.located` 用 `Error::fill_missing` 补上尚未填写的节、页、以及 `header` 或 `footer` 区域。调用方已经填进 `Error.at` 的字段保留。

### 4.3 库内落地约束

库内 `BandContent` 仍是 `Fn`，`decorate` 与 `paginate` 仍只借出 `&Decorations`。`from_fn` 的 `FnMut` 由 `spec/lower.rs` 的闭包经 `RefCell` 持有。`measure_bands` 单线程、不重入，`borrow_mut` 不会冲突。不把 `&mut` 穿过 `decorate` 与 `paginate`。

## 5. 行为

带高仍等于 `margins.top` 与 `margins.bottom`。函数不能把带撑高，也不能在返回空时缩小 margin。正文框在分页前已经按 margin 定死（`body_origin` 只取左、上 margin）。

装不下仍失败。行盒高于带高是现有的「decoration height … exceeds margin band」。宽过页面是现有的超宽错误。不裁切，不加高，不重排正文。

空节仍是一页，仍会调用该节的函数。新节仍强制从新物理页开始。该页的 `size` 用该节生效的纸张。

叠层顺序不变。`behind`，页眉，正文，页脚，`front`。

页眉页脚链接仍是 `BandSlot::Link`。URI 仍由 `TextRun` 在这一页拼成字符串。空 URI 仍失败。本规格不给页眉页脚加文内锚点。

## 6. 错误与 panic

函数返回的 `Error` 使整份 `layout_spec` 失败。调用方自己的失败用 `Error::band_fn` 包装，kind 是 `ErrorKind::BandFn`。

函数 panic 不捕获。panic 从 `layout_spec` 向调用方展开，`layout_spec` 不返回结果；进程是否退出取决于调用方。

`fixed` 与 `from_fn` 不在构造时排版，也不在构造时查字体。`fixed` 的字体问题在 `layout_spec` 组装阶段按节报告；`from_fn` 的字体问题与缺字在 `layout_spec` 里按该页该侧报告。

## 7. 版本与迁移

`crates/pdfcraft-pdf/Cargo.toml` 改为 `0.2.0`，同一提交更新已入库的 `Cargo.lock`（CI 跑 `cargo check --workspace --locked`）。`crates/pdfcraft-pdf/README.md`（状态行与两处依赖示例）与根 `README.md` 里 `pdfcraft-pdf` 一段的 `**Status:** \`0.1.0\`` 一并改；根 README 里 forms 一段的状态行不动。

`crates/pdfcraft-pdf/CHANGELOG.md` 的 `## [Unreleased]` 下 `### Breaking` 写明：`header` 与 `footer` 的类型变化；某一页可以不画；新增 `ErrorKind::BandFn` 与 `Error::band_fn`；`layout_spec` 要求 `K: 'static`。根 `CHANGELOG.md` 只加一行指向该条。

`AGENTS.md` 与提案书里 `pdfcraft-pdf` 的 `0.1.0` 句子改成 `0.2.0`。原句把 pdf 与 forms 写在一起，要拆开。forms 保持 `0.1.0`，orgchart 保持 `0.1.1`，post 保持 `0.2.0`。

仓库内 `SectionSpec` 的页眉页脚写法共 29 处、10 个文件（2026-11-02 统计）：`examples/hello.rs`、`bill.rs`、`anchors.rs`，`tests/document.rs`、`batch_baseline.rs`、`letter_layout.rs`、`golden_docs.rs`、`anchors.rs`、`spec_parity.rs`（赋值写法 `section.header = Some(...)`），`src/spec/tests.rs`。每处 `Some(BandSpec { ... })` 改成 `Some(BandFn::fixed(BandSpec { ... }))`。`None` 保持 `None`。`src/layout/decorate_tests.rs` 里的是库内 `BandContent` 闭包，不是 `SectionSpec`，不改。

## 8. 提案书与 AGENTS 要改的句子

实现代码前先改这些句子，使提案书与本文一致。行号按 2026-11-02 的 `docs/提案书.md`。

- 抬头状态行（第 6 行）：加一项「页眉页脚按页函数已确认（2026-11-02）」。
- §1 第 29 行：「调用方不写闭包」改为：正文组装仍用值，调用方不给正文块写函数；页眉和页脚可以是 `BandFn`。
- §1 第 38 行：「每一页绘制时」一条补上：除 `TextRun` 与 `PageWhen` 外，库按页调用页眉页脚函数。
- §1 完整模板（第 97–107 行）：`header: Some(BandSpec { … })` 改为 `Some(BandFn::fixed(BandSpec { … }))`。
- §2.10 第 503 行：字段类型写成 `Option<BandFn<K>>`。写明查询字段、`fixed`、`Ok(None)` 不画且 margin 不变。带高与不回流正文的句子保留。
- §4.1 第 624 行：「与 §1『调用方给值、不写闭包、不实现库内 trait』一致」改为不给正文块写函数、不实现库内 trait；页眉页脚函数见 §2.10。
- §5 第 632 行：删掉「某页关 header/footer 仍搁置」。写成由函数对该页返回空，margin 保留。
- §12 第 781 行：同日仍搁置项里删掉「某页关 header/footer（见 §5）」。
- 文末第 834 行：「章跟签署栏、某页关 header/footer 继续搁置」删掉后一项。
- 总表 §1 行（第 842 行）：「页眉页脚是 `BandSpec`」改为 `BandFn`（`fixed` 或按页函数）。
- 搁置表第 863 行：删掉「某页关闭 header/footer」一行。拆节仍是换纸张、换 margin 的办法。
- `AGENTS.md`：「调用方不提供闭包」改为不给正文块写函数、页眉页脚可为 `BandFn`；「搁置项（章跟流走、某页关 header/footer）」删掉后一项。

改完后用下面的命令核对，除新写的说明外不应再有命中：

```bash
rg -n "某页关|不写闭包|不提供闭包|Option<BandSpec>|页眉页脚是 \`BandSpec\`" docs/提案书.md AGENTS.md
```

本文件随 `proposal` 一并入库，作为实现合同。

## 9. 测试

`tests/document.rs` 至少覆盖：

- `header_fn_changes_page_one_only`。第 1 页的字面量不出现在第 2 页。
- `header_fn_none_keeps_the_body_origin`。第 2 页返回 `Ok(None)` 时，正文 y 与固定页眉的同页相同（`RecordingCanvas` 坐标逐位相等）。
- `footer_fn_none_skips_only_that_page`。页脚一侧同样成立。
- `band_fn_calls_in_page_order_header_then_footer`。函数把查询推进捕获的 `Vec`，断言顺序为 (1, header)、(1, footer)、(2, header)…；某侧为 `None` 时不出现。
- `band_fn_error_is_located`。页眉返回 `Error::band_fn("boom")`：kind 是 `BandFn`，`at` 含节、页与 `Region::Header`，`source()` 的展示是 `boom`。页脚一侧同样断言 `Region::Footer`。调用方预填的 `at.page` 保留。
- `band_fn_query_size_is_the_final_page_size`。第二节 `Paper` 横向、第三节 `Custom`，查询的 `size` 与各节 resolve 后尺寸相等，且页眉字面量能区分这些页。
- `fixed_band_font_error_reports_once_per_section`。`fixed` 用未注册的键：只有一条 `UnregisteredFont`，位置是该节，无页码。
- `from_fn_band_font_error_reports_per_page`。`from_fn` 用未注册的键：位置含页与 `header`。
- 现有 `header_numbers_every_page_and_layers_stack_behind_header_body_footer_front` 改为 `BandFn::fixed` 后仍通过。`TextRun` 页码仍被替换。
- `src/spec/tests.rs` 的 `an_empty_band_uri_is_located_in_the_header` 与 `a_missing_header_glyph_is_located_in_the_header` 改为 `BandFn::fixed` 后断言不变，并各加一份 `from_fn` 版本。

另加编译期断言：`DocumentSpec<()>` 仍是 `Send + Sync`。

`examples/band_fn.rs` 用 `BAND_FN_CASE` 选择 `first`、`last`、`odd-even`、`skip`、`count`、`sections`、`link`、`too-tall`、`fail`、`footer-skip`，写出 `out/band_fn.pdf`。`sections` 的第二节用横向纸张，页眉字面量印出 `query.size` 的宽高。`fail` 打印错误展示与 `source()` 后以非零退出。

性能：在进程内对同一份多页文档计时 `layout_spec`，比较 trunk 的 `Option<BandSpec>`、head 的 `BandFn::fixed`、head 的 `from_fn`（返回同样内容）三种写法。不用 `cargo run` 墙钟。

小步检查用该包的 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo check --all-targets`，并按测试名跑。合并前跑 `pdfcraft-pdf` 的完整 clippy 与 `cargo test --features test-utils`。
