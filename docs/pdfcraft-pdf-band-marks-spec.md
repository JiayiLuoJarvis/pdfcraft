# pdfcraft-pdf 页眉页脚全元素规格（band marks）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-02 |
| 状态 | 对话已定方向（方案 A）。尚未写入提案书。实现前先按 §9 改提案书与 `AGENTS.md`，再改代码 |
| 来源 | 需求方 hcsw-pdfcraft 的《需求书：页眉页脚 band 化（奇偶页 + 全元素表达）》（`/mnt/data/dev/hcsw-pdfcraft/.omo/upstream-band-header-footer-requirements.md`，只读引用） |
| 范围 | `pdfcraft-pdf` 的 `BandSpec` 及其 lowering、`layout::decorate` 的 band 测量。正文流、`behind` / `front`、`PageWhen`、forms、orgchart、post 不动 |
| 相关 | [`提案书.md`](提案书.md) §2.10、§2.12；[`pdfcraft-pdf-page-band-spec.md`](pdfcraft-pdf-page-band-spec.md)（`BandFn` 合同，本规格推翻其 §2.2 第二条）；[`pdfcraft-pdf-layer-rotation-spec.md`](pdfcraft-pdf-layer-rotation-spec.md) |
| 调用方 | 只改本仓库。不改 hcsw，不改 `/mnt/data/dev/hcsw-pdfcraft` |

## 1. 产品一句话

页眉页脚是页面上一块受限画布，不是只能放文字的槽。`LayerMark` 能画的东西（文字、图片、直线、矩形、椭圆、旋转组），`BandSpec` 都能画，坐标相对 band 左上，超出 band 报错。按页变化继续由 `BandFn::from_fn` 负责。

## 2. 选型

### 2.1 方案 A，不选方案 B

采用需求书的方案 A：表达力补在 band 层，`PageWhen` 不动。

方案 B（`PageWhen::{Odd, Even}`）不选，理由有三：

- B 让「按页变化」同时存在于 `BandFn::from_fn` 和 `PageWhen` 两套机制里。`from_fn` 已经能表达奇偶、首页、末页、任意页码条件，`PageWhen` 再长一档只是第二个入口。
- B 不回答总纲：band 仍只能放文字，页脚里的图片和边框只能留在 `front` 图层，带高校验、`Ok(None)` 跳过、按节页码这些 band 语义对它们都不生效。
- `PageWhen` 每加一档都是给图层补条件分支。条件在调用方函数里写成 `query.number.get() % 2` 更直接，也不限制组合。

### 2.2 R1、R2、R3 合并成一个字段

`BandSpec` 新增 `marks: Vec<BandMark<K>>`。一个字段同时满足：

| 需求 | 落点 |
| --- | --- |
| R1 图片槽 | `LayerShape::Image`，与 `LayerMark::Image` 共用 `ImageRef` 与同一份 `Resources` 注册表 |
| R2 形状 | 需求书选项 (b)。`LayerShape::{Line, Rect, Ellipse}` 与 `BandMark::Rotated` |
| R3 多行确定排布 | 需求书选项 (b)。每行一个 `LayerShape::Text`，各自 `at`、字体键、`RunStyle` |

不做需求书 R2 (a)（`BandDecoration` 整带边框与底色）。整带边框是一个 `Rect` 标记，矩形尺寸取 band 宽高即可。两种写法并存只会多一条要维护的路径。

不做需求书 R3 (a)（`Vec<BandLine>` 块级堆叠）。堆叠仍是流式排版，行高、基线来自字体度量，调用方无法直接命中 46.2 / 33.6 / 21.0pt 这类固定位置，还要另外定义行距语义。R3 (c) 的换算公式作为迁移文档交付（R6），不是 API。

## 3. 公共 API

### 3.1 `BandSpec`

```rust
#[derive(Debug, Clone)]
#[must_use]
pub struct BandSpec<K> {
    pub inset: BandInset,
    pub left: Option<BandSlot<K>>,
    pub right: Option<BandSlot<K>>,
    /// Absolute marks in band coordinates, painted under the slots in order.
    pub marks: Vec<BandMark<K>>,
}
```

`Default` 里 `marks` 为空。`BandSpec` 不加 `#[non_exhaustive]`，保留 `..BandSpec::default()` 写法。新增公共字段对结构体字面量是破坏性变更，见 §8。

### 3.2 `BandMark<K>`

```rust
/// One absolute mark inside a header or footer band.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub enum BandMark<K> {
    /// One unrotated shape, text line, or image.
    Shape(LayerShape<K>),
    /// Shapes rotated as one group.
    Rotated {
        /// Clockwise angle in degrees.
        degrees: f32,
        /// Rotation pivot in band coordinates.
        pivot: Point,
        /// Shapes in paint order.
        shapes: Vec<LayerShape<K>>,
    },
}
```

`BandMark` 与 `LayerMark` 的差别只有一处：没有 `when`。按页选择由 `BandFn::from_fn` 决定。形状本体直接复用 `LayerShape<K>`，band 与图层的形状集合因此由同一个类型保证对等，以后 `LayerShape` 加变体，band 自动获得。

`BandMark` 不加 `#[non_exhaustive]`：它描述的是既有本体（无旋转与有旋转两种），与 `LayerMark` 同一口径。`Clone` 是因为 `BandSpec` 是 `Clone`。`degrees` 是 `f32`，所以只派生 `PartialEq`，不派生 `Eq`。

## 4. 坐标与语义

### 4.1 band 坐标

原点是 band 左上角：`x` 等于页面 `x`，`y` 从 band 顶边量起，向下为正。

- 页眉：band 坐标 `(x, y)` 对应页面 `(x, y)`。
- 页脚：band 坐标 `(x, y)` 对应页面 `(x, y + page_height − margin.bottom)`。

`BandInset` 只作用于左右两个流式槽，不平移 `marks`。理由：`LayerMark` 的坐标本来就不受 margin 缩进影响，从 `front` 迁到 band 时只换算 `y`，不换算 `x`。

### 4.2 包含校验

每个标记在 band 内的包围盒必须落在 `[0, page_width] × [0, band_height]` 内，`band_height` 是该侧 margin。包围盒按下面的口径计算：

| 标记 | 包围盒 |
| --- | --- |
| `Line` | 两端点的外接矩形，四周各外扩 `pen.width / 2` |
| `Rect` / `Ellipse` | `rect`；`Ink` 含描边时四周各外扩 `pen.width / 2` |
| `Text` | 从 `at` 起的行盒：宽为排好的行宽，高为行盒高（与 `LayerMark::Text` 同一套 `single_line_run` 度量） |
| `Image` | `rect` |
| `Rotated` | 子形状包围盒的四角经同一个 `Affine` 变换后的外接矩形 |

越界报错，不裁切，不加高：

- 下边越界：`ErrorKind::DecorationTallerThanMargin { height, margin }`，`height` 是包围盒底边。这满足需求书 R2 的「装饰厚度计入高度」。
- 右边越界：`ErrorKind::DecorationWiderThanFrame { width, avail }`，`width` 是包围盒右边，`avail` 是页宽。
- 上边或左边越界（包围盒最小坐标为负）：新增 `GeometryKind::BandMarkOutside { x, y }`，字段是包围盒左上角。Display 写成 `header or footer mark starts outside its band at (x, y)`。

错误位置沿用 `measure_bands` 的 `ctx.located`：节、页、`Region::Header` 或 `Region::Footer`。

### 4.3 叠放顺序

同一 band 内先画 `marks`（按 `Vec` 顺序），再画 `left` 与 `right` 槽。底色矩形因此在槽文字之下。页面级叠层不变：`behind`、页眉、正文、页脚、`front`。

### 4.4 文字

`LayerShape::Text` 在 band 里与在图层里同义：单行、不断行、不对齐，`at` 是行盒左上。`TextRun` 按这一页填页码。样式从文档种子起步（12pt、黑、非粗），不继承任何 `Block::Styled`，与 `BandSlot` 和 `LayerMark::Text` 相同。

### 4.5 空 band

`left`、`right` 都是 `None` 且 `marks` 为空，等同今天的空槽：这一页这一侧不画，margin 保留。`Ok(None)` 语义不变。

## 5. 字体与图片解析时机

沿用 `pdfcraft-pdf-page-band-spec.md` §4.1、§4.2 的分工：

- `BandFn::fixed`：`marks` 里 `Text` 的字体键、粗斜体面在组装阶段解析一次，失败按节报告一次。`Image` 的 `ImageRef` 也在组装阶段校验：`validate_section_images` 扩到 `fixed` 页眉页脚的 `marks`，非本文档的图片报 `UnknownImage`。
- `BandFn::from_fn`：字体与图片按页解析。图片走 `image_for` 与 `DocToken` 比对，失败报 `UnknownImage`，位置含节、页、区域。

图片字节在 `layout_spec` 之前已经由 `Resources::add_image` 注册并解码，`from_fn` 只返回 `ImageRef`，不做 IO。这与需求书第七节第 1 条一致。

## 6. 库内落地

数据形状先定，再改函数。

1. `spec/lower.rs`。`OwnedBand` 增加 `marks: Vec<OwnedBandMark>`。`OwnedShape` 已存在，直接复用：`OwnedBandMark` 是 `Shape(OwnedShape)` 或 `Rotated { degrees, pivot, shapes: Vec<OwnedShape> }`。`own_band` 用现有 `own_shapes` 解析字体。`lower_band` 把标记降成 `Vec<LayerItem>`，逻辑与 `lower_layer_marks` 的非 `when` 部分共用（抽出 `lower_rotated` 与 `push_owned_shape` 的调用，不复制分支）。
2. `layout/decorate.rs`。`BandContent` 的返回值从 `Vec<FlowNode>` 改成一个具名结构 `BandNodes { flow: Vec<FlowNode>, marks: Vec<LayerItem> }`。`measure_band` 先按现状排流式槽，再把 `marks` 用现有 `append_layer_item` 译成 `FragmentCommand`，同时返回每项包围盒；命令按 §4.3 顺序拼进同一个 `Fragment`。包含校验在这里做，失败时不产生任何片段。
3. `geom.rs`。新增 crate 私有的包围盒计算（矩形外扩、`Affine` 作用于四角）。不新增模块依赖边，`scripts/check_pdfcraft_pdf_modules.py` 应保持通过。
4. `install_band` 需要拿到 `DocToken` 与 `LoweredImages`（今天只有 `layer_content` 拿到）。`lower_section` 把这两项传进去。

`nesting::check_content(Container::Band, …)` 不变。标记不进流，流内图片仍不允许出现在 band 里。

## 7. 合同锁定（需求书第三节与 R5）

需求书第三节五条，在 0.2.0 均已实现并有测试。本规格把下列测试标为合同测试，改名或删除须在 CHANGELOG 的 Breaking 里说明：

| 合同 | 现有测试（`tests/document.rs`） |
| --- | --- |
| 正文分页之后逐页调用，页眉先、页脚后，每页每侧恰一次 | `band_fn_calls_in_page_order_header_then_footer` |
| `Ok(None)` 不画，margin 保留，正文不动 | `header_fn_none_keeps_the_body_origin`、`footer_fn_none_skips_only_that_page` |
| `Error::band_fn` 的 kind、位置、`source()` | `band_fn_error_is_located` |
| 查询 `size` 是最终纸张尺寸 | `band_fn_query_size_is_the_final_page_size` |
| 字体错误：`fixed` 按节一次，`from_fn` 按页 | `fixed_band_font_error_reports_once_per_section`、`from_fn_band_font_error_reports_per_page` |

`band_fn_calls_in_page_order_header_then_footer` 补一条断言：调用总次数等于页数乘以存在的侧数，锁住「恰一次」。

## 8. 版本、CHANGELOG、文档

### 8.1 版本

`pdfcraft-pdf` 升到 `0.3.0`，按需求书 R7。crates.io 上当前只有 `0.1.0`（2026-11-02 查询），`0.2.0` 尚未发布。若决定 0.2.0 不单独发布，也可以把本规格并进 0.2.0，届时只改 CHANGELOG 归属，不改其他步骤。

`Cargo.toml`、已入库的 `Cargo.lock`、`crates/pdfcraft-pdf/README.md` 的状态行与依赖示例、根 `README.md` 的 `pdfcraft-pdf` 状态行同一提交更新。forms、orgchart、post 版本不动。

### 8.2 CHANGELOG

`crates/pdfcraft-pdf/CHANGELOG.md` 的 `## [Unreleased]`：

- `### Breaking`：`BandSpec` 新增公共字段 `marks`，结构体字面量须补 `marks: Vec::new()` 或 `..BandSpec::default()`。新增 `GeometryKind::BandMarkOutside`。
- `### Added`：`BandMark<K>`；页眉页脚可画图片、直线、矩形、椭圆、单行文字与旋转组，坐标相对 band 左上，越界报错。

根 `CHANGELOG.md` 加一行指向该条。

### 8.3 文档与例子

- `crates/pdfcraft-pdf/docs/guide.md` 的 “Header, footer, and page numbers” 追加两段：band 坐标与包含校验；奇偶菜谱（`from_fn` 按 `query.number.get() % 2` 返回不同 `BandSpec`，或返回 `Ok(None)`）。满足 R4。
- 新增 `crates/pdfcraft-pdf/docs/migrate-band-marks.md`，体例照 `migrate-inline.md`。内容：`front` 上的 `LayerMark` 页脚改成 footer band 的逐步写法；`y` 换算公式 `y_band = y_page − (page_height − margin.bottom)`；`at` 是行盒左上而不是基线，给出由基线位置反推 `at.y` 的方法（基线偏移等于该字号的行盒 ascent）；样式种子差异；`DecorationTallerThanMargin`、`DecorationWiderThanFrame`、`BandMarkOutside` 的排查清单。满足 R6。
- `examples/band_fn.rs` 新增 `BAND_FN_CASE=odd-even-marks`：奇数页页脚只有一张二维码图片，偶数页页脚是 180×65、1pt `#999999` 描边矩形，内含两行 9pt 文字、一行 8pt 灰色 `No.` 编号和一张 55pt 图片。图片用 `examples/assets` 里已有的资源。`examples/README.md` 同步。现有 `odd-even` 用例保留。
- rustdoc：`BandSpec::marks`、`BandMark` 及其变体写 `///`，`missing_docs` 已是 `deny`。

## 9. 提案书与 AGENTS 要改的句子

实现代码前先改。

- 提案书抬头状态行：加一项「页眉页脚可放图片与形状（band marks）已确认（2026-11-02）」。
- 提案书 §2.10「槽位」段后新增「标记」段：`BandSpec.marks` 是 `BandMark` 列表，形状同 `LayerShape`，可整组旋转；坐标相对 band 左上，`x` 同页面，`y` 从 band 顶边起；先画标记后画槽；包围盒越出 band 报错，不裁切，不加高。
- 提案书 §2.10 首段「`BandSpec` 只描述带里画什么」保留。
- 提案书总表 §2.10 行与 §1 行：页眉页脚补「可放文字、图片、形状」。
- `pdfcraft-pdf-page-band-spec.md` 的状态行加一句：§2.2「返回值只有左右槽」已由本规格取代。
- `AGENTS.md` 与提案书里 `pdfcraft-pdf` 的 `0.2.0` 改为 `0.3.0`（若按 §8.1 并入 0.2.0 则不改）。

核对命令：

```bash
rg -n "只有 \`BandSpec\` 的左右槽|只能放文字" docs/ AGENTS.md
```

## 10. 测试

排版断言一律用 `RecordingCanvas` 坐标，不断言压缩字节。新增于 `tests/document.rs`：

- `footer_image_mark_lands_in_band_coordinates`。页脚标记 `Image` 的页面矩形等于 band 矩形加 `(0, page_height − margin.bottom)`。
- `footer_rect_mark_strokes_at_band_offset`。180×65、1pt 描边矩形的描边路径与宽度按坐标逐位断言。
- `band_marks_paint_before_slots`。同一 band 内，标记命令在槽文字命令之前。
- `band_text_marks_keep_fixed_lines`。三行 `Text` 标记（9 / 9 / 8pt，不同颜色）各自落在给定 `at`，行间距不受行高影响。
- `band_mark_below_band_is_taller_than_margin`。描边外扩使包围盒底边超过 margin 0.5pt 时报 `DecorationTallerThanMargin`，`height` 含描边。恰好贴边时通过。
- `band_mark_past_page_width_is_wider_than_frame`。
- `band_mark_above_band_is_outside`。负 `y` 报 `GeometryKind::BandMarkOutside`，位置含页与区域。
- `rotated_band_mark_checks_rotated_bounds`。未旋转时在带内、旋转 45° 后越界的矩形报错。
- `from_fn_odd_even_marks`。奇数页页脚只有图片命令，偶数页页脚有矩形、三行文字和图片。
- `fixed_band_foreign_image_reports_once_per_section` 与 `from_fn_band_foreign_image_reports_per_page`。另一份 `Resources` 的 `ImageRef` 报 `UnknownImage`。
- `empty_marks_and_slots_skip_the_side`。三者皆空时这一侧没有片段，正文原点不变。

`src/layout/decorate_tests.rs` 补包围盒计算的单元测试（直线外扩、椭圆、旋转组四角）。

`BAND_FN_CASE=odd-even-marks` 的端到端检查按 `.cursor/skills/verify-pdfcraft` 的方式：跑例子，用 `pdftotext` 看偶数页的说明文字，用 `pdfimages -list` 看每页页脚的图片数。

## 11. 交付顺序

每步结束跑该包的 `cargo fmt --check`、`cargo clippy --all-targets --features test-utils -- -D warnings`、`cargo check --all-targets`，以及该步新增的测试。

1. 改提案书、AGENTS 与旧规格状态行（§9）。
2. `geom` 包围盒计算与单元测试。
3. `BandNodes` 与 `measure_band` 拼接标记、包含校验、`GeometryKind::BandMarkOutside`。先用库内 `BandContent` 闭包测（`decorate_tests.rs`）。
4. 公共 `BandMark`、`BandSpec.marks`，`lower.rs` 解析与降级，图片校验。仓库内所有 `BandSpec { … }` 字面量补字段。§10 的集成测试。
5. 合同测试补「恰一次」断言（§7）。
6. 例子、guide、迁移文档、CHANGELOG、版本号。
7. 完整门禁：`AGENTS.md`「门禁」全部命令。

## 12. 不做

- `PageWhen` 新档位，含 `Odd` / `Even`。
- band 内的流式图片、表格、列表、栏嵌套变化。`nesting` 规则不动。
- 标记上的链接。页眉页脚链接仍只有 `BandSlot::Link`。
- 由内容撑高 band、裁切越界内容、越界时回流正文。
- 需求书第八节的调用方迁移。那是 hcsw 侧在升级 pin 之后自己做的事。
- 页眉内容设计。机制与页脚同源，本规格落地即覆盖。
