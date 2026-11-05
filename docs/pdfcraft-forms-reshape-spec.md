# pdfcraft-forms 0.3 公共面与结构重塑规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-05 |
| 状态 | **已确认（2026-11-05）。** D1 至 D24 与检查清单 1–9 已写入提案书 §14。§15 不在本次。公共签名按该确认实施 |
| 范围 | 只改 `crates/pdfcraft-forms`：公共面、错误形状、模块与文件划分、`font` 与 `subset` 的环，以及字体构造和测字里两处不改输出的多余开销（D23、D24）。折行、缩字、内边距、勾的形状、子集字节、flatten 规则不改。单故障的 `ErrorKind` 名字保留，线段页号越界除外（§6）。`Display` 整句不再是合同 |
| 版本 | `pdfcraft-forms` `0.3.0`。全部 breaking 改动在这一版完成（VER-3） |
| 相关 | [`pdfcraft-forms-spec.md`](pdfcraft-forms-spec.md)（行为合同）；[`pdfcraft-forms-font-subset-spec.md`](pdfcraft-forms-font-subset-spec.md)；[`pdfcraft-forms-repair.md`](pdfcraft-forms-repair.md)；[`pdfcraft-orgchart-reshape-spec.md`](pdfcraft-orgchart-reshape-spec.md)（同层先例）；[`pdfcraft-pdf-shape-refactor-spec.md`](pdfcraft-pdf-shape-refactor-spec.md)；[`rust-engineering-standard.md`](rust-engineering-standard.md) API-2、API-8、API-9、API-11、API-12、ARC-2、ARC-3、ERR-2、ERR-3、ERR-7、DET-1、VER-1 至 VER-3、TOOL-5 |
| 行号 | 以 2026-11-05 工作区 HEAD `36f5d33` 为准。动手前重新核对 |
| 冲突 | 提案书 §14 已确认本文。与 §14 冲突时以提案书为准。与 `pdfcraft-forms-spec.md` 冲突时，以本文的公共面为准，行为合同仍以该规格 §4 为准。`pdfcraft-forms-spec.md` 的签名句在阶段 4 改写。实现中发现与代码事实冲突，先停下来改本文，不要自行裁决 |

本文是已确认的实现合同，不重开「填哪些字段、要不要 OTF」这类产品讨论。

## 1. 产品一句话

调用方先 `Form::open` 读一份已有 PDF，再 `Fill::new` 用请求和可选的 `Font` 编译出已校验、已测字的 `Fill`，最后 `to_pdf` 嵌入字体、画出并 flatten。请求与数据问题在 `Fill::new` 失败；`to_pdf` 只剩字体子集与嵌入、叠绘和 PDF 写出阶段的失败。单个 TrueType 用 `Font::new`，字体集合用 `Font::from_collection`。文档默认外观在 `FillStyle`，单个字段的覆盖在 `FieldStyle`，框底横线是单独的 `FieldRule`。折行、缩字、勾、子集、flatten 与 `0.2.0` 相同。

## 2. 对标范式

| 层 | 范式 | 在本 crate 的落点 |
| --- | --- | --- |
| 公共面 | 先编译、再使用（`regex::Regex::new` 加 `find`；股权图是 `Chart::new` 加 `to_pdf`） | `Form::open` 只读 PDF。`Fill::new` 做全部校验，并完成测字、折行与缩字。`to_pdf` 只子集、叠绘、剥离、写出 |
| 请求形状 | 提案书 §12.7 的字段三分，股权图 §13 的落法 | 可复用外观是 `FillStyle`、`FieldStyle`、`StrokeStyle`。每次请求的策略 `missing` 平放在 `FillRequest` 上。描述性数据是 `FieldFill`、`FieldRule`、`AbsoluteLine`。前两者以字段名为 `BTreeMap` 的键 |
| 字体 | ARC-3：外部字节在边界解析一次。股权图把 `FontBytes` 收成 `Font` | 单面 `Font::new`，集合 `Font::from_collection`。类型留在本 crate，不与 `pdfcraft-pdf`、`pdfcraft-orgchart` 共享 |
| 错误 | 种类与位置分开。pdf 用 `ErrorKind` + `Location`，股权图用 `ErrorCode` + `ErrorSubject` | 种类留在 `ErrorKind`。字段名、页号、缺字从变体载荷里拿出来，改走 `field()`、`page()`、`scalar()`。不另做一套与 kind 重复的 subject |
| 内部 | 打开之后只用快照，不回到「再扫一遍注释才知道矩形」 | `Form::open` 生成 `WidgetSnap` 列表。填写计划引用这份快照 |
| 模块 | ARC-2：边沿 DAG 向下 | 拆掉今天的 `font` ↔ `subset` 环。`lopdf` 只出现在 `pdf` 与 `paint`。`skrifa` 只出现在 `font` |

不采用的方向，以及原因：

- **把填写收成一棵 `children` 树。** 输入是按名字对齐的 widget，不是排版树。§12.7 的 `children` 命名不搬。
- **builder、serde 主接口、可插拔 PDF 后端。** `lopdf` 是本 crate 唯一的 PDF 栈。协议记录用结构体字面量构造。
- **`Pt` 或 `FontSize` 新类型。** 股权图规格否决了 `Mm`。非法字号在 `Fill::new` 拒绝。再包一层新类型删不掉这条校验。
- **再做一套 `ErrorSubject`，把字段名在 kind 里存一份、在 subject 里再存一份。** 位置只放在 `Error` 的访问器上，kind 不再携带。
- **和兄弟 crate 共享 `Font` 或 `Error`。** 包边界禁止这条边。
- **把 `FieldValue::CheckBox(bool)` 改成枚举，或删掉 `CheckBox(false)` 与空字符串。** 省略名字表示「这个名字不参与本次填写」。`CheckBox(false)` 和 `Text("")` 仍会做类型检查（以及选项是否在 `/Opt` 里），然后不画。两件事不一样，所以值留着。`bool` 的含义在 rustdoc 里写明。确认提案书时改掉 Google disposition t5 里「先提修正案才许动」的那句，因为本轮就是那次修正；`bool` 本身不改。
- **为了兼容 `0.2` 的多故障优先级，把样式、值和横线捆回一条全是 `Option` 的记录。** 没有值的对齐或字号覆盖，在合法时不改变输出字节。横线会画出来，所以单独成 `FieldRule`。
- **保留 `list_fields`。** 它与 `Form::open` 再 `fields()` 是同一个入口。VER-3 不留第二个名字。
- **`fields` 与 `rules` 用带 `name` 的 `Vec`。** `0.2` 的 `HashMap` 写不出重复名。换成 `Vec` 会凭空多出一个 `DuplicateName` 错误和一步检查。名字留作 `BTreeMap` 的键，重复由类型排除。
- **`Fill::new` 不接收字体，绘制时再分 `to_pdf()` 与 `to_pdf_with(&Font)`。** 这样编译阶段拦不住调用方最关心的缺字和放不下，数据错误与写出错误都从绘制返回，批跑时分不开。
- **把字体放进 `FillRequest`。** `Font` 不实现 `PartialEq`（D12），放进去之后 `FillRequest` 就不能派生 `PartialEq`。字体是 `Fill::new` 的单独参数。

## 3. 拟定的决定

这些决定待提案书确认。确认前它们不是产品合同。

| # | 决定 |
| --- | --- |
| D1 | 新增 `Form` 与 `Fill`。删除 `list_fields`、`fill_and_flatten`、`FillOptions`、`FontBytes`，不留过渡（VER-3）。读取只走 `Form::open` 与 `Form::fields` |
| D2 | `Fill::new(&Form, FillRequest, Option<&Font>)` 接收已经解析的字体。有非空文本要画而 `font` 为 `None` 时返回 `FontNotConfigured { Missing }`。没有文本要画时不读 `font`。`Fill::to_pdf(&self)` 不接收参数 |
| D3 | `Form` 不持有字体。有文本要画时，`Fill` 持有字体的一个克隆（只加 `Arc` 引用计数），供 `to_pdf` 子集与嵌入 |
| D4 | 单面用 `Font::new` / `Font::from_shared`。集合用 `Font::from_collection` / `Font::from_shared_collection(data, index)`。`new` 遇到 `ttcf` 失败；`from_collection` 遇到非 `ttcf` 失败。两者都是今天的 `FontReason::Collection`。不依赖具体字形的检查在这些构造函数里做完 |
| D5 | `FillStyle { font_size, h_align, v_align }`，`Default` 为 12 pt、左、底。`FieldStyle { font_size, h_align, v_align }` 三个字段都是 `Option`，`Default` 全是 `None`，表示沿用 `FillStyle`。`FillRequest` 平放 `missing`，并带 `style`、`fields`、`rules`、`lines`。`fields` 与 `rules` 是以字段名为键的 `BTreeMap` |
| D6 | `FieldFill { value, style }`，名字是 `fields` 的键。`value` 必填。框底横线是 `rules` 里的 `FieldRule { offset }`，名字同样是键，可以没有对应的 `FieldFill`。`AbsoluteLine` 的线宽和颜色收进 `StrokeStyle`，几何与页号留在记录上。不给 `FieldRule` 加颜色或线宽 |
| D7 | 名字唯一由 `BTreeMap` 的键保证，不设 `DuplicateName`。同一个名字同时出现在 `fields` 与 `rules` 是正常的（既填值又画线）。不存在「五槽全空」这种记录 |
| D8 | `Fill::new` 拒绝全部与请求和数据有关的失败：协议校验、线段上的非有限数与页号越界、缺字体、缺字、缺 advance、放不进框。`to_pdf` 只剩子集、清零子集的 `head` 与写出失败 |
| D9 | `FieldInfo` 改为私有字段加访问器 |
| D10 | `FieldType` 与 `FieldValue` 去掉 `#[non_exhaustive]`。`ErrorKind`、`ArgumentReason`、`FlattenReason`、`FontReason` 维持 `#[non_exhaustive]` |
| D11 | `Error` 字段私有。`kind()` 返回不含字段名、页号、字符的种类。位置用 `field()`、`page()`、`scalar()`。删除 `PartialEq`。`Display` 供人读，整句不是合同，且不得重复 `source()` 的文本 |
| D12 | `Form`、`Fill`、`Font` 字段私有。`Font` 与 `Form` 的 `Debug` 不输出字节。`Font` 不实现 `PartialEq` |
| D13 | 上限与版式常量保持私有或成为样式默认值，不在 crate 根导出 |
| D14 | API-9：`FillStyle`、`FieldStyle`、`StrokeStyle` 归入第四类。`FieldFill`、`FieldRule`、`FillRequest`、`AbsoluteLine` 归入第五类。`FillOptions` 从第三类举例里删掉。`FieldType` 与 `FieldValue` 归入第一类闭集 |
| D15 | 模块按阶段重组。`lopdf` 只在 `pdf` 与 `paint`。`skrifa` 只在 `font`。测试放在同目录的 `*_tests.rs` 或留在短文件里，见 §8 |
| D16 | 拆掉 `font` ↔ `subset` 环：`checksum` 移到新叶子 `sfnt`。`font` 不再调用 `subset_sfnt`，也不再 `use` `PdfDoc` |
| D17 | 不在类型定义模块之外写 inherent `impl`。今天 `content.rs` 对 `inspect::PdfDoc` 的 `impl` 收成 `paint` 的自由函数 |
| D18 | `to_pdf` 不修改 `Form` 与 `Fill`。同一 `Fill` 两次输出字节相同（DET-1）。没有文本要画时，`Fill::new` 传 `None` 与传 `Some(&font)` 的输出字节相同 |
| D19 | widget 身份在内部用打开顺序的下标。字段名只出现在 `Error::field()`，不放进 kind。不把矩形放进 `FieldInfo` |
| D20 | 工作区里的 `src/cff_subset.rs` 是未跟踪文件，属于 OTF 草案（`pdfcraft-forms-otf-spec.md`），不在 HEAD `36f5d33`。本系列任何 PR 都不提交、不挂进模块树、不删除它。搬文件时不得 `git add` 它 |
| D21 | 只有一种故障时，`ErrorKind` 与 `0.2.0` 的种类相同（载荷位置按 D11 搬迁）。唯一例外是线段页号越界，从 `FlattenFailed` 改为 `InvalidArgument`（§6）。两种以上故障同时存在时，返回哪一个只遵循 §5 的新顺序，不再承诺与 `0.2.0` 的组间优先级相同 |
| D22 | 按 §13 迁完之后，成功路径的绘制、子集、剥离与 `0.2.0` 字节级相同。不改算法、常量、页面上的文字。错误 `Display` 可以改写 |
| D23 | 构造 `Font` 时不清零 `head`，只做只读检查：表目录能读、有 `head`、长度不小于 54、不越界。失败集合与今天 `zero_head_times` 相同，仍是 `FontReason::Head`。清零只对 `to_pdf` 里生成的子集做一次。单面字体直接持有调用方的字节，`from_shared` 零拷贝 |
| D24 | 测字只做一次。`Face` 构造时建好 `Charmap` 与 `GlyphMetrics`，之后每个字符只查表。`LaidText::fit` 对每个字段先查矩形、再分词一次，然后才进入字号循环；循环里只折行与摆放。输出与失败种类都不变 |

## 4. 公共 API 终态

crate 根只导出下面这些条目，全部加 `#[doc(inline)]`。

```rust
pub use error::{ArgumentReason, Error, ErrorKind, FlattenReason, FontReason};
pub use fill::{Fill, Form};
pub use font::Font;
pub use request::{
    AbsoluteLine, FieldFill, FieldInfo, FieldRule, FieldStyle, FieldType, FieldValue, FillRequest,
    FillStyle, HAlign, MissingName, StrokeStyle, VAlign,
};

pub type Result<T> = std::result::Result<T, Error>;
```

不导出 `list_fields`、`fill_and_flatten`、`FillOptions`、`FontBytes`。

### 4.1 读取

```rust
#[derive(Clone)]
#[must_use]
pub struct Form { /* 私有 */ }

impl Form {
    /// 打开 PDF，列出 widget，并拒绝加密与 `/XFA`。
    ///
    /// 失败种类与今天的 `list_fields` 相同：[`ErrorKind::NotAPdf`]、
    /// [`ErrorKind::Encrypted`]、[`ErrorKind::HasXfa`]。
    pub fn open(pdf: &[u8]) -> Result<Self, Error>;

    /// 每个 widget 一条，顺序与今天的 `list_fields` 相同。
    #[must_use]
    pub fn fields(&self) -> &[FieldInfo];
}
```

`Form::open` 的拒绝条件就是今天 `PdfDoc::open`（`inspect.rs` 第 241 行起）：`Document::load_mem` 失败映射 `NotAPdf` 或解密类 `Encrypted`；`is_encrypted()` 再报 `Encrypted`；`has_xfa` 为真报 `HasXfa`，含空的 `/XFA` 数组。不在 `open` 里读字体，不在 `open` 里看 `FillRequest`。

`Form::open` 按今天 `WidgetSnap::info`（`inspect.rs` 第 77 行）的映射一次建好字段快照：`ListBox` 与 `ComboBox` 带 `options`，其余种类的 `options` 为 `None`。`fields()` 借用这份不可变切片，不在每次调用时克隆字段名或选项。缺少 `/T` 或 `/Rect` 的注释继续跳过，不出现在快照里。

`Debug` 手写，只输出页数与 widget 数，以 `finish_non_exhaustive()` 结束，不打印 PDF 字节。`Form` 可以 `Clone`：克隆只共享已解析文档的 `Arc`（§7.2），不复制对象图。`Form` 不实现 `PartialEq`。

### 4.2 请求

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillStyle {
    /// 起始字号，单位 pt。拟合时仍可降到 7 pt，步长 0.5 pt。
    pub font_size: f32,
    pub h_align: HAlign,
    pub v_align: VAlign,
}

impl Default for FillStyle {
    fn default() -> Self {
        Self {
            font_size: 12.0,
            h_align: HAlign::Left,
            v_align: VAlign::Bottom,
        }
    }
}

/// 单个字段相对 [`FillStyle`] 的覆盖。`None` 表示沿用文档样式，不是「改回左对齐 / 12 pt」。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FieldStyle {
    pub font_size: Option<f32>,
    pub h_align: Option<HAlign>,
    pub v_align: Option<VAlign>,
}

/// 一个名字要画的值。名字是 [`FillRequest::fields`] 的键。
#[derive(Debug, Clone, PartialEq)]
pub struct FieldFill {
    pub value: FieldValue,
    pub style: FieldStyle,
}

impl FieldFill {
    /// `style` 为 [`FieldStyle::default`]。
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self;
    /// `style` 为 [`FieldStyle::default`]。
    #[must_use]
    pub fn check(checked: bool) -> Self;
}

/// 框底下方一条与框同宽的黑线，线宽 1 pt。名字是 [`FillRequest::rules`] 的键。
/// 不需要字体，也可以没有对应的 [`FieldFill`]。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldRule {
    /// 相对框底的偏移，单位 pt。有限的负数仍然合法。
    pub offset: f32,
}

impl FieldRule {
    #[must_use]
    pub fn new(offset: f32) -> Self;
}

/// 线段的线宽和颜色。`Default` 是红色 0.8 pt，与今天 [`AbsoluteLine::new`] 的笔触相同。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    pub width: f32,
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self { width: 0.8, red: 1.0, green: 0.0, blue: 0.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbsoluteLine {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub page_index: usize,
    pub style: StrokeStyle,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FillRequest {
    pub style: FillStyle,
    pub missing: MissingName,
    /// 键是字段名，按字节比较。
    pub fields: BTreeMap<String, FieldFill>,
    /// 键是字段名。可以与 `fields` 的键重叠。
    pub rules: BTreeMap<String, FieldRule>,
    pub lines: Vec<AbsoluteLine>,
}

impl FillRequest {
    /// `style` 为 [`FillStyle::default`]，`missing` 为 [`MissingName::Error`]，
    /// `rules` 与 `lines` 为空。
    #[must_use]
    pub fn new(fields: BTreeMap<String, FieldFill>) -> Self;
}

impl Default for FillRequest {
    fn default() -> Self {
        Self::new(BTreeMap::new())
    }
}
```

`BTreeMap` 而不是 `HashMap`：键的迭代顺序就是 §5 的字典序，校验顺序不依赖哈希器，`FillRequest` 也不带哈希器泛型。

`FieldFill` 与 `FieldRule` 不实现 `Default`。`AbsoluteLine::new(x1, y1, x2, y2)` 仍是页 0、`StrokeStyle::default()`。`AbsoluteLine::default` 仍是原点上的这条线段。

颜色通道只要求有限，不要求落在 `0.0..=1.0`。本轮不增加范围检查。`FieldRule` 不使用 `StrokeStyle`：它永远是 1 pt 黑色。给横线加样式是新能力，不在本轮。

`FillStyle`、`FieldStyle`、`StrokeStyle`、`FillRequest`、`FieldFill`、`FieldRule`、`AbsoluteLine` 保持公有字段与结构体字面量，不标 `#[non_exhaustive]`。0.x 增加字段按 breaking 处理。

只有值、没有单独样式覆盖的字段，用 `FieldFill::text` / `check`。只改其中一个外观槽时，写 `FieldFill { style: FieldStyle { h_align: Some(HAlign::Center), ..FieldStyle::default() }, ..FieldFill::text("Ada") }`。只画横线、不填值时，名字只放进 `rules`，不放进 `fields`。

`FieldStyle` 挂在勾选值上时，字号覆盖仍按 §5.2 校验，但字号与对齐都不影响勾的绘制。rustdoc 写明这一点，不为它改数据形状。

旧 API 里「只有字号或对齐、没有值」的条目，合法时不改变输出。新 API 写不出来。非法的孤立字号同样写不出来；要让非法字号失败，必须把它放在一条真正的 `FieldFill` 上。

关联常量：不把 `12.0`、`7.0`、`0.5`、`1.2`、内边距提到 crate 根。`12.0` 只出现在 `FillStyle::default`。最小字号、步长、行高系数、内边距留在 `layout` 的私有常量里，数值与今天 `fields.rs` 第 6 至 18 行、`layout.rs` 第 8 至 10 行相同（7 pt = `7_000` 毫点，步长 `500` 毫点，起始字号上限 `256_000` 毫点）。

### 4.3 字段值与快照

`FieldType` 七个变体不变：`Text`、`CheckBox`、`ListBox`、`ComboBox`、`Signature`、`RadioButton`、`PushButton`。去掉 `#[non_exhaustive]`。无 `/FT` 仍视为 `Text`。`/Btn` 与 `/Ch` 的位判定不变（行为规格 §4.1）。

`FieldValue` 仍是 `Text(String)` 与 `CheckBox(bool)`。去掉 `#[non_exhaustive]`。`CheckBox(true)` 画勾，`CheckBox(false)` 不画。ListBox 与 ComboBox 仍用 `Text`。画出的字符串不改写成显示值。

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct FieldInfo { /* 私有 */ }

impl FieldInfo {
    #[must_use]
    pub fn name(&self) -> &str;
    #[must_use]
    pub fn field_type(&self) -> FieldType;
    #[must_use]
    pub fn page_index(&self) -> usize;
    /// ListBox / ComboBox 为 `Some`，其余为 `None`。
    #[must_use]
    pub fn options(&self) -> Option<&[String]>;
}
```

矩形、checkbox 的 on-state、`/AP` 仍不进入 `FieldInfo`。

### 4.4 编译与绘制

```rust
#[must_use]
pub struct Fill { /* 私有 */ }

impl Fill {
    /// 按值接收请求。失败时请求随之丢弃；要重试须调用方先克隆。
    ///
    /// `font` 只在有非空文本要画时使用。有文本而 `font` 为 `None` 时返回
    /// [`ErrorKind::FontNotConfigured`]，[`FontReason::Missing`]。
    pub fn new(form: &Form, request: FillRequest, font: Option<&Font>) -> Result<Self, Error>;

    /// 嵌入字体、叠绘、剥离并写出。可能因字体子集或度量换算、内容流与
    /// 页面资源更新、剥离或 PDF 写出失败；不再产生请求校验、缺字或放不下。
    pub fn to_pdf(&self) -> Result<Vec<u8>, Error>;
}

impl fmt::Debug for Fill { /* 绘制指令数、线段数、是否持有字体；不输出文档与字节 */ }
```

- `Fill::new` 做 §5.2 的全部检查，并完成测字、折行与缩字。它从字段快照读取名字、类型与选项；绘制计划只复制要用的矩形、页号与已经解析好的样式，拥有文本字符串与排好的行，并记录用到的 glyph id。它不把名字或选项复制进计划，不子集，不写 PDF。
- `to_pdf` 不改变 `Fill` 或 `Form`。它在 `Form` 文档的一份克隆上绘制，见 §7.2。
- `Debug` 手写，以 `finish_non_exhaustive()` 结束。`Fill` 不实现 `Clone` 与 `PartialEq`。
- 成功时的绘制规则与今天 `fill_and_flatten` 相同：墨迹画在页面内容之上；去掉全部 Widget 注释和 catalog `/AcroForm`；不写字段 `/V`，不写外观流；空字段表也剥离；未绘制控件的原外观不保留；页面上预先印好的内容保留。
- 同名多个 widget 仍使用同一条 `FieldFill` 和同一条 `FieldRule`。类型检查和 `/Opt` 检查仍以打开顺序里的第一个同名 widget 为准。其余同名 widget 照画，即使它们的类型和第一个不同。横线画在每一个同名 widget 的框下。这一条是今天 `plan` 里 `first.entry(name).or_insert`，以及 `line_below` 按每个 snap 各画一条的行为，保持不变。
- 名字比较是字节级的字符串相等。不规范化 Unicode，不拼接点分全名。`/T`、`/FT`、`/Ff`、`/Opt`、`/AP` 仍沿 `/Parent` 取第一个已经存在的键。`HAlign`、`VAlign`、`MissingName` 的变体与 `Default` 不变。

### 4.5 字体

```rust
#[derive(Clone)]
pub struct Font { /* 私有 */ }

impl Font {
    /// 一个 `sfnt`。字节以 `ttcf` 开头时失败。
    pub fn new(data: impl Into<Vec<u8>>) -> Result<Self, Error>;

    /// 不修改 `data`。见 [`Self::new`]。
    pub fn from_shared(data: std::sync::Arc<Vec<u8>>) -> Result<Self, Error>;

    /// 从 `ttcf` 抽出 `index` 这一面。字节不是 `ttcf` 时失败。
    pub fn from_collection(data: impl Into<Vec<u8>>, index: u32) -> Result<Self, Error>;

    /// 不修改 `data`。见 [`Self::from_collection`]。
    pub fn from_shared_collection(
        data: std::sync::Arc<Vec<u8>>,
        index: u32,
    ) -> Result<Self, Error>;
}

impl fmt::Debug for Font { /* 字节长度、集合 index（单面没有）、units per em；不输出字节 */ }
```

`new` 与 `from_shared` 走同一条路径，`from_collection` 与 `from_shared_collection` 走另一条。构造只读不写（D23）。`from_shared` 零拷贝：`Font` 直接持有调用方的 `Arc`。`new` 把传入的 `Vec` 包进 `Arc`，不另复制。`from_collection` 与 `from_shared_collection` 仍要拆 TTC，会写出一份新的面字节，放进 `Font` 内部的 `Arc<Vec<u8>>`。两次调用同一个 `Arc` 得到两个 `Font`；单面时两者共享同一份字节。调用方的 `Arc` 不被修改。

不在构造时清零 `head` 的理由：今天构造时清零一次，`embed` 对子集还会再清零一次（`font.rs` 第 185 行 `FontFile::from_source(&subset, 0)`）。子集复制源字体的 `head`，第二次清零把时间戳与 `checkSumAdjustment` 置零后重算，`head` 的目录校验和也一并重写。所以第一次清零不影响嵌入的字节，只换来一份整面字体的复制。CJK 单面常见十几 MB。

构造函数依次做今天这些检查，失败种类不变：

| 顺序 | 今天的位置 | 失败 |
| --- | --- | --- |
| `new` 见到 `ttcf`，或 `from_collection` 见到非 `ttcf` | 新的分界。今天两种字节都从 `FontBytes.index` 进来 | `FontReason::Collection` |
| `from_collection` 按 `index` 抽出一个 `sfnt` | `font.rs` `extract_collection_face`（第 325 行起） | `Collection`；目录越界时今天会报 `Head`，保持这个区分 |
| 只读检查 `head`：目录能读、表存在、长度不小于 54、不越界。不写字节（D23） | `FontFile::zero_head_times`（第 118 行起）里写字节之前的那些检查 | `FontReason::Head` |
| `FontRef::from_index(..., 0)`。抽出之后的面和单面文件都用 0 | `Face::load`（第 38 行起） | `FontReason::Unreadable`，`source()` 为 skrifa 错误 |
| 没有 `glyf` 表 | `Face::load` | `FontReason::NoGlyf` |
| `units_per_em` 为 0 | `Face::load` | `FontReason::UpemZero` |
| ascent 或 descent 非有限 | `Face::load` | `FontReason::Width` |

单面文件今天把调用方的 `index` 交给 `FontRef::from_index`。新的 `Font::new` 固定用 0。`index != 0` 的单面文件是新的拒绝差异：以前可能在 `from_index` 报 `Unreadable`，现在 `new` 不接收 index。集合必须走 `from_collection`。这是 API 分界，不是新的字体合法性规则。

构造函数不在这里做：cmap 缺字、字形没有 advance、某个宽度换算进不了 `i32`、子集之后清零 `head`。

字形缓存不放进 `Font`。每个 `Fill` 在 `Fill::new` 里自己记录用到的 glyph id。因此 `Font` 不可变，`Clone` 只增加 `Arc` 的引用计数，可以跨次填写、跨线程复用。

`Debug` 手写，以 `finish_non_exhaustive()` 结束。单面不打印 index。集合打印调用方传入的 `index`。不实现 `PartialEq` / `Eq`。公共签名里不出现 skrifa 类型。

今天「传了坏字体但本次没有文本要画」不会解析字体（`fill.rs` 第 52 至 60 行）。新 API 里，调用方不去构造 `Font` 就不会碰到这个失败。`Fill::new` 在没有文本要画时不得读 `font` 的字体表，`Fill` 也不持有它，输出字节不因传了 `Some(&font)` 而改变（D18）。

### 4.6 错误

种类与位置分开。种类是 `Copy`，因为字符串已经不在变体里。

```rust
#[derive(Debug, Clone)]
pub struct Error { /* 私有 */ }

impl Error {
    #[must_use]
    pub fn kind(&self) -> ErrorKind;
    /// 这条失败点名的 widget 名字。没有名字时为 `None`。
    #[must_use]
    pub fn field(&self) -> Option<&str>;
    /// 页号越界时的页下标。其余失败为 `None`。
    #[must_use]
    pub fn page(&self) -> Option<usize>;
    /// [`ErrorKind::MissingGlyph`] 时缺的那个字符。
    #[must_use]
    pub fn scalar(&self) -> Option<char>;
}

impl std::error::Error for Error { /* source() 行为不变 */ }
```

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    NotAPdf,
    Encrypted,
    HasXfa,
    UnknownField,
    IllegalChoice,
    InvalidCheckbox,
    InvalidArgument(ArgumentReason),
    MissingGlyph,
    BoxOverflow,
    FontNotConfigured(FontReason),
    SignatureNotWritable,
    WidgetUnsupported,
    FlattenFailed(FlattenReason),
}
```

- 不提供公共构造函数，也删除 `From<ErrorKind>`。位置和种类是两截，crate 外用填表函数拿到 `Error`。crate 内用 `pub(crate)` 构造。
- 删除 `PartialEq`。`ErrorKind` 本身保持 `PartialEq`，调用方比较 `kind()`。
- `field()` 有值的种类：`UnknownField`、`IllegalChoice`、`InvalidCheckbox`、`SignatureNotWritable`、`WidgetUnsupported`、`MissingGlyph`、`BoxOverflow`，以及落在某个字段或横线上的 `InvalidArgument`。默认字号和线段上的非法值，`field()` 为 `None`。
- `page()` 只在 `InvalidArgument(ArgumentReason::PageOutOfRange)` 时有值，即 `lines` 里某条线段的 `page_index` 不小于文档页数。`ArgumentReason` 新增单元变体 `PageOutOfRange`。`FlattenReason::PageOutOfRange` 删除：widget 的页号读自文档本身，线段页号已在 `Fill::new` 拒绝，`paint` 取不到页面是内部不变量，不是对外的失败。
- `scalar()` 只在 `MissingGlyph` 时有值。
- `ArgumentReason::FontSize { value }` 与 `NonFinite { value }` 保留被拒绝的数。`FlattenReason::NestingTooDeep { depth }` 保留深度。这些是原因，不是位置。
- `ErrorKind` 要 `Copy`，所以 `ArgumentReason`（今天只有 `Clone`）与 `FlattenReason` 都加上 `Copy`。`ArgumentReason` 含 `f32`，仍只派生 `PartialEq`。
- `FontReason` 的变体不变。
- `Display` 供日志。它应当让人看出种类、`field()`、`page()`、`scalar()` 里已经有的事实。整句与 `0.2.0` 不必相同，测试不断言整句。`Display` 不得包含 `source()` 的文本（ERR-3）。
- 不提供 `to_dict`。不另做 `ErrorSubject`。
- `Error::with_source` 继续是 `pub(crate)`。

### 4.7 名字唯一

`fields` 与 `rules` 是 `BTreeMap`，同一张表里写不出重复名。`ArgumentReason` 不增加 `DuplicateName`。同一个名字一边在 `fields`、一边在 `rules`，是正常用法。

`ArgumentReason` 只增加 `PageOutOfRange`（§4.6）。它已经是 `#[non_exhaustive]`。

不增加 `EmptyField`。`FieldFill` 必须有值，`FieldRule` 必须有偏移，空记录写不出来。

### 4.8 删除或改形的公共条目

| 删除或改形 | 替代 |
| --- | --- |
| `list_fields` | `Form::open` 与 `Form::fields` |
| `fill_and_flatten` | `Fill::new` 与 `to_pdf` |
| `FillOptions` 及其 `'a` | `FillRequest` + `Option<&Font>` |
| `FontBytes { bytes, index }` | `Font::new` 或 `Font::from_collection` |
| `HashMap<String, FieldValue>`、四张借用的覆盖 `HashMap`，以及没有值的覆盖 | `BTreeMap<String, FieldFill>`，覆盖在 `FieldFill.style`；横线用 `BTreeMap<String, FieldRule>` |
| `FlattenFailed(PageOutOfRange { page_index })` | `InvalidArgument(PageOutOfRange)`，页号走 `page()` |
| `AbsoluteLine` 上平铺的线宽和颜色 | `style: StrokeStyle` |
| `Error.kind` 公有字段，以及 kind 里的 `field_name` / `scalar` / `page_index` | `kind()`、`field()`、`page()`、`scalar()` |
| `Error: PartialEq`、公共 `Error::new`、`From<ErrorKind>` | 比较 `kind()`；错误只从库函数返回 |
| `FieldInfo` 公有字段 | 访问器 |
| `FieldType`、`FieldValue` 的 `#[non_exhaustive]` | 穷尽匹配 |

不提供 `#[deprecated]` 包装、类型别名或隐藏的重导出。

## 5. 失败顺序

这一节是实现顺序，用来让同一次调用的结果确定。它不是与 `0.2.0` 多故障优先级的兼容承诺（D21）。只有一种故障时，种类与 `0.2.0` 相同。

### 5.1 `Form::open` 与 `Font` 的构造函数

两者互不调用。谁先被调用，谁的错误先返回。

`Form::open` 只产生 `NotAPdf`、`Encrypted`、`HasXfa`。

`Font::new`、`from_shared`、`from_collection`、`from_shared_collection` 只产生 §4.5 表里的 `FontNotConfigured`。

### 5.2 `Fill::new`

前提：`Form::open` 已经成功。顺序固定：

名字的「字典序」就是 `BTreeMap` 键的迭代顺序。

1. **默认字号。** `style.font_size` 走今天 `check_font_size`（`fill.rs` 第 281 行）：非有限、不大于 0、毫点不是有限值或放不进 `i64`、或大于 256 pt。`InvalidArgument(FontSize { value })`，`field()` 为 `None`。
2. **未知名字。** 仅当 `missing == Error`。先 `fields` 的键，再 `rules` 的键。组内取字典序最小的、不在快照里的名字，`UnknownField`，`field()` 为该名字。
3. **字段上的字号覆盖。** `missing == Ignore` 时跳过未知名字，不检查它们的字号。对快照里存在的名字，若 `style.font_size` 为 `Some`，按字典序做与步骤 1 相同的检查，`field()` 为该名字。
4. **横线偏移。** 同样跳过未知名字。对存在的名字，`offset` 非有限时按字典序报 `InvalidArgument(NonFinite { value })`，`field()` 为该名字。负的有限偏移仍然合法。
5. **类型与选项。** 名字存在于快照的 `fields` 条目，按字典序，用该名字的第一个 widget，规则与今天 `WidgetSnap::accept`（`inspect.rs` 第 130 行）相同：
   - `Signature` → `SignatureNotWritable`
   - `RadioButton` 或 `PushButton` → `WidgetUnsupported`
   - `CheckBox` 配 `Text` → `InvalidCheckbox`
   - `CheckBox(false)` → 不绘制，不是错误
   - `CheckBox(true)` → 稍后画勾
   - `Text` 配 `CheckBox` → `InvalidArgument(CheckboxOnText)`
   - `Text("")` → 不绘制，不是错误；若它是选项，仍须先通过 `/Opt` 检查
   - `ListBox` 或 `ComboBox` 配 `CheckBox` → `InvalidArgument(CheckboxOnChoice)`
   - 选项字符串不在 export 与 display 列表里 → `IllegalChoice`
   - 其余非空文本 → 稍后绘制

   以上凡是失败，`field()` 都是该名字。
6. **线段数值。** 按 `lines` 的向量顺序。任一坐标、线宽或颜色通道非有限：`InvalidArgument(NonFinite { value })`，`field()` 为 `None`。这一步早于测字，因此缺字与非有限线段同时存在时返回参数错误。
7. **线段页号。** 第 6 步对全部线段做完之后，按 `lines` 的向量顺序。`page_index` 不小于 `Form` 的页数：`InvalidArgument(PageOutOfRange)`，`page()` 为该下标，`field()` 为 `None`。
8. **字体。** 计划里有非空文本要画而 `font` 为 `None`：`FontNotConfigured(FontReason::Missing)`，`field()` 为 `None`。没有文本要画时不读 `font`。
9. **测字与缩字。** 按打开顺序遍历要画文本的 widget，做今天 `LaidText::fit` 的测字与缩字，同时记录用字表。第一个失败返回：`MissingGlyph`（`field()` 与 `scalar()`）、`FontNotConfigured(NoAdvance)`，或 `BoxOverflow`（`field()`）。空文本与未勾选不进入这一步。

`missing == Ignore` 时，未知名字不进入步骤 3、4、5。

### 5.3 `to_pdf`

前提：`Fill::new` 已经成功。顺序固定：

1. 有文本时嵌入字体：子集、清零子集的 `head`、写字体字典。失败仍是 `FontNotConfigured`（`Head` 来自今天 `subset.rs` 的 `head_error`，`Width` 来自 `em1000`），不改记成 `FlattenFailed`。先嵌入、再逐页叠绘，与今天 `content.rs` 第 168 行的顺序相同，对象编号因此不变。今天 `content.rs` 第 203 行与 `fill.rs` 第 82 行的 `Missing` 在新结构里不可达，按内部不变量处理，不再是对外的失败路径。
2. 叠绘与写出。内容流数字非有限仍是 `FlattenFailed(NonFinite)`。其余 `FlattenReason` 的种类不变。

勾的路径、clip、`FieldRule` 的 1 pt 黑线，都在步骤 2，不产生新的 kind。

## 6. 行为变化

按 §13 迁完之后，成功的输出字节与 `0.2.0` 相同。只有一种故障时，`ErrorKind` 的名字与 `0.2.0` 相同，第 8 条除外。下面这些是故意改掉的：

1. **字体失败的时机。** 今天坏字体在字段协议检查之后、测字之前才解析（`fill.rs` 第 52 至 74 行）。`0.3` 里坏字体在调用方构造 `Font` 时失败，缺字体在 `Fill::new` 第 8 步失败。
2. **打开 PDF 早于默认字号，如果调用方先 `Form::open`。** 今天 `check_font_size` 在 `PdfDoc::open` 之前（`fill.rs` 第 48 至 49 行）：垃圾 PDF 加上非法默认字号，得到 `InvalidArgument`。先 `open` 时，垃圾 PDF 得到 `NotAPdf`、`Encrypted` 或 `HasXfa`。
3. **不需要字形就能发现的非法线段，在 `Fill::new` 返回。** 今天非有限线段在测字之后才查（`fill.rs` 第 116 行）。缺字加上非有限线段时，`0.2.0` 报缺字，`0.3` 在 `Fill::new` 报 `InvalidArgument`。
4. **没有值的字号或对齐覆盖写不出来。** 合法时它们不改变 PDF。非法的孤立字号也不再有一条独立的失败路径。
5. **单面字体不再接收 index，集合必须走 `from_collection`。** 把 `ttcf` 交给 `Font::new` 得到 `Collection`。
6. **多种故障同时存在时，谁先返回只按 §5。** 不再复现「先扫值表、再扫四张覆盖表」的组间顺序。
7. **`Display` 整句可以与 `0.2.0` 不同。** 位置改从访问器读。
8. **线段页号越界改为参数错误。** `0.2.0` 在叠绘时报 `FlattenFailed(PageOutOfRange { page_index })`。`0.3` 在 `Fill::new` 报 `InvalidArgument(PageOutOfRange)`，页号走 `page()`。这是调用方传错了数，不是写出失败。
9. **缺字、缺 advance、放不进框在 `Fill::new` 返回。** 种类不变，`to_pdf` 不再产生它们。调用方据此区分：`Fill::new` 的失败是请求或数据的问题，`to_pdf` 的失败是文件或库的问题。

不改变的行为，实现时禁止顺手「修正」：

- 同名 widget 以打开顺序的第一个做类型和 `/Opt` 判断，其余照画。
- `missing == Ignore` 时不检查未知名字的字号和偏移；已知名字上的非法字号和非法偏移仍然失败。
- `CheckBox(false)` 与 `Text("")` 做类型检查（选项还包括 `/Opt`）然后不画。省略名字则连检查都不做。
- 没有文本要画时，不嵌入字体。
- 勾的几何常数（`content.rs` 第 11 至 19 行）与 ZapfDingbats `"4"` 的对照关系。
- 最小 7 pt、步长 0.5 pt、行高 1.2、内边距左/右 1、底 1、顶 0.5。
- 末行只剩一个字形时最多再降 2 pt。
- ASCII 按词断、超宽词按字符断、码位大于 U+007F 按字符断、显式换行分段。
- 成功路径嵌入保留原 glyph id 的 `glyf` 子集；`FontFile2` 不压缩；两次成功填写字节相同。
- 资源名 `QuireForms` 至 `QuireForms64` 用尽时仍是 `FontNameTaken`。
- 横线仍是 1 pt 黑色。线段的默认笔触仍是红色 0.8 pt。

## 7. 内部结构

### 7.1 快照

`Form::open` 在校验加密与 XFA 之后走今天的 `widgets()`（`inspect.rs` 第 253 行）：页顺序、页内注释顺序。每条内部记录：

```rust
pub(crate) struct WidgetSnap {
    pub(crate) rect: PdfRect,
}
```

`Form` 另存一份与 `WidgetSnap` 等长、同顺序的 `Vec<FieldInfo>`。不变量是 `widgets[i]` 与 `fields[i]` 描述同一个 widget；内部遍历时按下标或 `zip` 配对。名字、类型、页号和选项只存于 `FieldInfo`，不复制第二份字符串或选项。§5.2 的类型与选项检查改为接收配对的 `&FieldInfo`，不再让 `WidgetSnap::accept` 从自身读取这些字段。折叠或覆盖不产生新 widget。

`Fill` 保存：

- `Form` 文档的 `Arc`，见 §7.2。
- 计划：每个要画的 widget 一条绘制指令，内容是勾，或已经折行缩字的文本行加对齐；每条横线一条指令。
- 有文本时：用字表与字体的克隆。没有文本时两者都不存在。
- `lines` 的拥有副本。非有限数与越界页号已在 `Fill::new` 拒绝。

文本从请求移进 `Fill` 后，`to_pdf` 不再借用 `FillRequest`。

### 7.2 文档所有权

合同只要求：`to_pdf` 不修改 `Form` 和 `Fill`；同一输入两次字节相同。

`lopdf 0.45` 的 `Document` 派生 `Clone`（`document.rs` 第 18 行）。它的 `encryption_state` 里的 `CryptFilter` 约束为 `Send + Sync`，所以 `Document` 是 `Send + Sync`。定为一种方案，写进 `Form` 的模块文档：

- `Form` 持有 `Arc<PdfDoc>`、`Vec<WidgetSnap>` 与等长同序的 `Vec<FieldInfo>`。`fields()` 返回后者的切片。
- `Fill::new` 克隆这个 `Arc`，因此 `Fill` 的生命周期不绑在 `&Form` 上。`to_pdf` 不再接收 `&Form`。
- `to_pdf` 克隆一次 `Document`，在副本上画并写出。

不按原始字节重新打开，不保留第二套方案。几何只用快照，不在 `to_pdf` 里再扫一遍注释。

### 7.3 字体内部

`Font` 内部保存：面字节（`Arc<Vec<u8>>`）、集合的 `index`（单面没有）、`upem`、`ascent`、`descent`。单面时面字节就是调用方的字节，时间戳原样保留（D23）。集合时是拆出来的那一面。两种情况后续 `FontRef::from_index` 都使用 `0`。

`Fill::new` 用 `Font` 的字节建一个 `Face`，`Face` 借用这份字节，生命周期不超出 `Fill::new`。`Face` 在构造时建好 `skrifa` 的 `Charmap` 与 `GlyphMetrics`（`Size::unscaled()`、默认位置），之后每个字符只查表（D24）。今天 `Face::glyph` 每查一个字符都新建这两个对象（`font.rs` 第 74、78 行）。

`LaidText::fit` 的内部顺序（D24），失败种类与今天相同：

1. 矩形检查：`max_w` 或 `inner_h` 不大于 0 时直接得到 `BoxOverflow`，不分词。今天这条检查在每个字号的 `lay_at` 开头、分词之前，与字号无关，结果相同。
2. 按 `\n` 分段并分词一次，产生 `MissingGlyph` 或 `NoAdvance`。今天 `lay_at` 在每个字号重新分词（`layout.rs` 第 140 行），而字宽是 font units，不随字号变化。
3. 字号循环与末行缩字（`shrink_orphan`）只做折行与摆放，复用第 2 步的词表。空格字形仍按需查：只有某个字号真的画出词间空隙时才查，查到的结果在各字号之间复用。缺空格字形仍是 `MissingGlyph`，出现时机与今天相同。

D24 的收益是推断，未测量。字号循环从调用方字号以 0.5 pt 步长降到 7 pt，起始字号上限 256 pt。

`Fill::new` 里建的用字表仍是 `BTreeMap<u16, (char, f32)>`，按 glyph id 排序，保证子集和 `/W` 的顺序稳定。不要改回 `HashMap`。

嵌入对象的构造顺序保持字体子集规格与 `pdfcraft-forms-repair.md`：子集字节、`with_compression(false)`、FontDescriptor、CIDFontType2、`/CIDToGIDMap /Identity`、ToUnicode、Type0。`BaseFont` 仍是 `QuireForms`。子集生成仍调用今天的 `subset_sfnt`，只是调用点从 `PreparedFont::embed` 挪到 `paint`。

### 7.4 不在此轮做的内部清理

- 不合并 `subset.rs` 的 `read_u16` 与 `font.rs` 的 `u32be` 这类大端读取函数。三份手写的 sfnt 目录解析改用 `skrifa::raw` 是后续项（§15）。
- 不碰工作区里未跟踪的 `cff_subset.rs`（D20）。OTF 仍未做。
- 不把 widget 快照暴露成公共类型。
- 不引入 `WidgetId` 新类型包装，除非 PR 2 发现裸 `usize` 会和页下标混用。若引入，它是 `pub(crate)` 的元组结构体，包 `usize`，不进公共导出。这是 PAN-4 的内部加强，不是公共 breaking。默认先用字段名 `index` 对 `page_index`，不新建类型。

## 8. 模块与文件

### 8.1 今天的边

生产代码的 `use crate::` 构成下面的图。`font` 与 `subset` 互相引用，这是 ARC-2 要拆的环。

```text
error
fields                         无内部依赖
subset  → error, font          subset.rs 第 6 行 use font::checksum
font    → error, inspect, subset
           font.rs 第 7 行 use inspect::PdfDoc
           font.rs 第 184 行 subset::subset_sfnt
layout  → error, fields, font, inspect
inspect → error, fields        PdfDoc、list_fields、WidgetSnap
content → error, fields, font, inspect, layout
           content.rs 第 161 行 inherent impl PdfDoc
fill    → error, fields, font, inspect, layout, content
lib     → 上述模块的重导出
```

工作区里的 `src/cff_subset.rs` 是未跟踪文件，不在 HEAD，不在这张图里（D20）。

`lopdf` 今天出现在 `inspect`、`font`（嵌入字典）、`content`（经由 `PdfDoc` 的方法）。`skrifa` 出现在 `font`。

### 8.2 目标模块图

目录模块用 `<name>/mod.rs`，不写成 `<name>.rs` 加同名目录（`clippy::self_named_module_files` 为 deny）。

```text
sfnt      无内部依赖                         checksum
error     无内部依赖
geom      无内部依赖                         PdfRect
request   无内部依赖                         公共值与 FillStyle::default
subset    → error, sfnt                      glyf 子集。不 use font
font      → error, sfnt                      Font、Face、测量。不 use subset::subset_sfnt，不 use pdf
pdf       → error, geom, request             打开、快照、写出原语。唯一持有 Document 的模块
layout    → error, font, geom, request       折行与缩字。不 use pdf，不 use lopdf
paint     → error, pdf, font, subset, layout, geom, request
fill      → error, pdf, layout, paint, request, font
lib       → fill, error, font, request       只做重导出
```

节点内部文件互相引用，不记成边，与 `docs/module-dependency.md` 对 orgchart 的处理相同。

允许的 `lopdf`：`pdf`、`paint`。允许的 `skrifa`：`font`。`layout` 只通过 `font` 的 `Face` 测字。

**禁止：**

- `font` → `subset`、`pdf`、`paint`、`layout`、`fill`（`font` → `sfnt` 允许）
- `subset` → `font`、`pdf`、`paint`
- `pdf` → `font`、`paint`、`layout`、`fill`、`subset`
- `layout` → `pdf`、`paint`、`fill`、`subset`
- `paint` → `fill`
- `request` → 任何内部模块
- 除 `pdf` 与 `paint` 以外 `use lopdf`
- 除 `font` 以外 `use skrifa`
- 在 `pdf` 以外为 `PdfDoc` 写 inherent `impl`

`paint` 需要改文档时，只调用 §8.4 列出的 `pub(crate)` 原语，不把 `Document` 字段暴露到 `pdf` 目录之外。

### 8.3 文件去向

PR 1 只搬，不改控制流。下表的行号是 HEAD `36f5d33`。

| 现有 | 去向 |
| --- | --- |
| `error.rs` | `error.rs`，原地 |
| `fields.rs` 的公共类型与 `Default` | `request.rs` |
| `fields.rs` 的 `DEFAULT_FONT_SIZE` | 删除常量，数值写进 `FillStyle::default`。PR 1 可以先把常量留在 `request` 作 `pub(crate)`，PR 3 再收进 `Default` |
| `fields.rs` 的 `LINE_HEIGHT_FACTOR`、`PAD_*` | `layout` 的私有常量。PR 1 搬走后 `layout` 不再 `use fields` 的这些常量 |
| `inspect.rs` 的 `PdfRect`（第 16 行起） | `geom.rs` |
| `inspect.rs` 的 `open_error`、`PdfDoc::open`、`widgets`、`has_xfa`、`lookup`、`inherited`、`page_annots`、`read_options`、`field_type_of` | `pdf/read.rs` |
| `inspect.rs` 的 `save`、`add_object`、`add_page_contents`、`attach_font`、`strip_form`、`prune`、`reachable`、`copied_resources`、`filter_annots`、`fresh_font_name` | `pdf/write.rs` |
| `inspect.rs` 的 `WidgetSnap`、`Draw` | `pdf/snapshot.rs`。`Draw` 若只被 `fill` 使用，PR 1 可以随 `fill` 留下，不要为了对称强行下沉 |
| `inspect.rs` 的 `list_fields` | PR 1 与 PR 2 保持函数签名。PR 3 删除，调用方改用 `Form::open` 与 `fields` |
| `font.rs` 的 `checksum`（第 402 行） | `sfnt.rs` |
| `font.rs` 的 `Glyph`、`Face`、`GlyphFault`、`FontFile`、`extract_collection_face`、`zero_head_times` | `font/mod.rs` |
| `font.rs` 的 `PreparedFont::embed`、`descriptor`、`cid_font`、`to_unicode`、`em1000`（第 183 至 321 行） | PR 1 先随 `font` 搬走以保持能编译。PR 2 再挪到 `paint/embed.rs`，届时 `font` 去掉对 `PdfDoc` 和 `subset_sfnt` 的调用 |
| `subset.rs` | `subset/mod.rs`。`use crate::font::checksum` 在 PR 2 改为 `crate::sfnt::checksum` |
| `layout.rs` | `layout.rs`，单文件。约 400 行生产代码，不拆 |
| `content.rs` 的 `Mark`、`Stamp`、`Stroke`、内容流与勾的常数 | `paint/stream.rs` |
| `content.rs` 的 `impl PdfDoc::paint`（第 161 行） | PR 2 改为 `paint` 里的自由函数 `paint(doc: &mut PdfDoc, ...)`。PR 1 若必须暂时留下 inherent impl，只允许写在 `pdf/` 目录内，不允许继续留在 `paint` |
| `fill.rs` | `fill.rs`。PR 3 才把 `fill_and_flatten` 换成 `Form` / `Fill` |
| `lib.rs` | 只保留模块声明与 §4 的重导出 |

`pdf/mod.rs` 定义 `PdfDoc`。字段对子模块 `pub(super)`。`read.rs` 与 `write.rs` 是 `pdf` 的子模块，可以写这些字段。

### 8.4 `pdf` 暴露给 `paint` 的原语

PR 2 结束时，`paint` 只能通过这些 `pub(crate)` 函数碰文档。名字可以沿用今天的方法名。签名里允许出现 `lopdf::Object` 与 `lopdf::ObjectId`，因为 `paint` 本来就要组装字体字典。

- `open`、`widgets`（`fill` 也用）
- `page_ids`
- `add_object`
- `add_page_contents`
- `attach_font`
- `strip_form`
- `prune`
- `save`

`lookup`、`inherited`、`reachable`、`has_xfa` 留在 `pdf` 内部，不升到 `pub(crate)`，除非 `paint` 缺了它们就无法保持今天的剥离行为。缺了就先改本文再加，不要预防性加宽。

### 8.5 可见性

与股权图规格 §8.2 末段相同，并加一条 forms 专有的限制：

- 被拆类型的定义放在该目录的 `mod.rs`。`PdfDoc` 在 `pdf/mod.rs`。
- 同一目录的兄弟文件要读写私有字段时，字段提到 `pub(super)`，不提到 `pub(crate)`。
- 跨目录只走 §8.4 的原语。
- 不用 `include!`。
- 不为了搬文件把条目提到 `pub`。

### 8.6 测试放置

- 单元测试留在 `src/`。集成测试留在 `tests/`，只使用公共 API。
- `layout.rs` 自第 402 行起的内联测试、`subset.rs` 自第 537 行起的内联测试、`font.rs` 自第 417 行起的内联测试、`inspect.rs` 自第 740 行起的内联测试、`content.rs` 自第 260 行起的内联测试：随生产代码搬。生产代码仍长于测试时，可以继续内联。
- `tests/fill_and_flatten.rs`（949 行）、`tests/list_fields.rs`（299 行）、`tests/fuzz_parser.rs`（236 行）在 PR 3 改到新公共面。PR 1 与 PR 2 不改断言。

## 9. 测试

- **PR 1。** 测试数量与断言不变。`use` 路径随搬运更新。
- **PR 2。** `fill_and_flatten` 的集成测试断言一行不改，包括两次调用字节相同、未知名字的字典序、`missing` 的两种模式。
- **PR 2b（D24）。** 既有测试一行不改。新增两条 `layout` 单元测试，用 `subset` 测试里那份只映射 `A`、`B`、`C` 的合成字体，锁住失败时机：矩形宽度为 0 且文本是 `"Z"` 时仍是 `BoxOverflow`；`"AB"` 成功，`"A B"` 报 `MissingGlyph`，缺的是空格。
- **字节基线。** PR 2b 与 PR 3 开工前，用 `PDFCRAFT_FORMS_FONT` 跑 `fill_tax_refund_guarantee` 示例（含中文字段）与 `fill_basic`，记下输出的 SHA-256。两个 PR 合入后重跑，哈希必须相同。这是 D22、D23、D24 的直接证据。
- **PR 3。** 集成测试改调用 `Form`、`Fill`、`Font`。期望的 PDF 字节与 `pdftotext` 不变。错误断言改为 `kind()`、`field()`、`page()`、`scalar()`，不再比较 `Display` 整句。保留一条断言：`Display` 不含 `source()` 的文本。
- **新增测试，只在 PR 3：**
  - `Font::new` 拒绝 `ttcf`。`Font::from_collection` 拒绝单个 `sfnt`。空字节、坏集合下标、无 `glyf`、`upem == 0` 返回 `FontNotConfigured`。`Unreadable` 的 `source()` 不为空。
  - `Font` 的 `Debug` 输出不含字体里一段长度为 16 的原始字节（用夹具里一段独特字节做断言）。单面的 `Debug` 不打印 index。
  - D23：`from_shared` 之后，`Font` 内部的 `Arc` 与调用方的是同一个（`Arc::ptr_eq`，`font` 模块的单元测试）。`head` 截短到 54 字节以下时，构造返回 `FontNotConfigured(Head)`。嵌入的子集 `head` 时间戳为 0。
  - 同一个 `Font` 填两份不同的 `Fill`，与各自新建 `Font` 的结果字节相同。
  - 同一个 `Fill` 调用两次 `to_pdf`，字节相同。`Form::fields()` 在绘制前后相同。
  - 只有勾选时，`Fill::new` 传 `None` 与传 `Some(&font)`，`to_pdf` 字节相同。有文本而传 `None` 时，`Fill::new` 返回 `Missing`。
  - 同一名字既在 `fields` 又在 `rules` 时成功，值与横线都画出。
  - 缺字与非有限线段同时存在时，`Fill::new` 返回 `InvalidArgument(NonFinite)`。只缺字时 `Fill::new` 返回 `MissingGlyph`；文字放不下时 `Fill::new` 返回 `BoxOverflow`。两者都不等到 `to_pdf`。
  - 线段 `page_index` 越界时，`Fill::new` 返回 `InvalidArgument(PageOutOfRange)`，`page()` 为该下标。
  - 非法默认字号与损坏 PDF：先 `Form::open` 得到 `NotAPdf`。合法 PDF 加非法字号，`Fill::new` 得到 `InvalidArgument`。
  - `Form`、`Fill`、`Font`、`Error` 满足 `Send + Sync`。
- 不断言压缩内容流里既有测试没有锁过的细节。既有测试已经断言的字节级相等继续保留。
- 中文夹具仍要求 `PDFCRAFT_FORMS_FONT`。未设置时相关测试跳过。

## 10. PR 切分

顺序固定。每个 PR 都能单独通过 TOOL-5 门禁。公共 API 的 breaking 只出现在 PR 3。

| PR | 内容 | 版本与 CHANGELOG |
| --- | --- | --- |
| 1 | 按 §8.3 搬文件。允许暂时保留 `font` ↔ `subset` 环和「embed 仍在 font 里」。`paint` 若还不能独立，PR 1 可以把 `content.rs` 原样挪进 `paint/stream.rs`，但 inherent `impl PdfDoc` 必须改到 `pdf/` 目录内或改成自由函数。不改逻辑。在 `docs/module-dependency.md` 加一节，写明环要到 PR 2 才断。提交前确认未跟踪的 `cff_subset.rs` 不在暂存区（D20） | 不升版本，不记用户可见变更 |
| 2 | 断环：`checksum` 移到 `sfnt`；`embed` 移到 `paint`；`font` 不再依赖 `pdf` 与 `subset_sfnt`。`PdfDoc` 的跨目录入口收成 §8.4。公共函数签名不变 | 不升版本，不记 |
| 2b | D24：`Face` 一次建好 `Charmap` 与 `GlyphMetrics`；`LaidText::fit` 按 §7.3 先查矩形、再分词一次、再进字号循环。只动 `font` 与 `layout`，公共函数签名不变 | 不升版本，不记 |
| 3 | §4 与 §5：`Form`、`Fill`、`Font`（构造按 D23 只读不写）、`FillStyle`、`FieldStyle`、`StrokeStyle`、`FieldFill`、`FieldRule`、`FieldInfo` 访问器、错误的种类与位置分离。同步示例、`docs/guide.md`、crate README、`tests/`。按 D14 改工程规范 API-9 的举例、Google disposition t10（三个 `Style` 按名字归入）和 t11（点名 `FieldFill`、`FieldRule`、`FillRequest`、`AbsoluteLine`）。t5 改为：`CheckBox(bool)` 保留，`true` 画勾、`false` 不画，本轮不引入 `CheckboxState`。**开工前必须先由提案书新增的 forms reshape 条目确认本文** | 升到 `0.3.0`。`Breaking` 写 §4.8 与 §6。合入后可发 `0.3.0` |
| 4 | 文档终态：改写 `pdfcraft-forms-spec.md` 的 §3、§4.5、§5 里被新公共面替换的句子；`workspace-crates-spec.md` 的 forms 状态行；根 `README` 与 `AGENTS.md` 的版本句；`module-dependency.md` 的目标图改为 §8.2 且注明环已断开。行为规格 §4.1 至 §4.4 的发现与绘制规则保持 | 不升版本 |

PR 3 把公共面和错误形状放在一起，因为同一批 doctest、示例和集成测试要改一次（VER-3）。PR 内可以分两组提交，每组门禁都绿：先请求、`Form`、`Fill`、`Font`，再把 kind 里的位置搬到访问器。第二组会改 `ErrorKind` 的变体形状，测试在这一组一起改。

PR 3 之前若要改提案书，单独一个文档提交，不混进搬文件的 PR。

## 11. 文档同步

### 11.1 与提案书 §10 冲突、确认时要改的句子

确认前这些句子仍然有效，实现不得提前违反：

- §10 的两个函数签名：`list_fields` 与 `fill_and_flatten`。确认后删除这两个名字。
- `FillOptions`、`FontBytes { bytes, index }`、按字段名的覆盖表、`line_below`、`absolute_lines` 的平铺字段。
- 「`pdfcraft-forms` 的 `FontBytes`（借用 `&[u8]`）不受影响」出现在 §4.1、§13 与确认总表。本轮若确认，改为：forms 在边界解析 `Font`，单面与集合用不同的构造函数，仍是单次字体，类型不与 orgchart 共享，仍然不依赖 `pdfcraft-pdf`。
- 错误节写「`Error` 是结构体，含 `kind: ErrorKind`」，以及 kind 表里带 `field_name` 的变体。改为私有错误、`kind()` 与 `field()` / `page()` / `scalar()`。
- 版本句 `0.2.0`。PR 3 才改成 `0.3.0`。

建议的提案书条目写法与 §13 相同：一条「`pdfcraft-forms` 公共值形状」，列出 D1 至 D14 的要点，行为变化写 §6，指向本文。条目编号取当时的下一节。不要把 OTF、建表单、加密写进该条。

### 11.2 其他文档

- **行为规格** `pdfcraft-forms-spec.md`：PR 4 改 §3 的签名与 §5 的错误形状。§4 的发现、flatten、字体子集、勾、未知名字改用 `FieldFill` 与 `FieldRule` 叙述。绘制语义不变。§6 的版本句随 PR 3 的发布版本改。
- **子集规格**与 **repair 记录**是已落地的历史合同，不改原文。PR 2 把 `embed` 挪走之后，repair 文档里的函数名会过时；在 reshape 规格 §7.3 保留对象顺序即可，不回头改 repair 文档。
- **调用指南** `crates/pdfcraft-forms/docs/guide.md` 与两个 example：PR 3 改。README 里的示例同样改。crate README 的示例不受门禁编译，PR 3 人工核对。
- **工程规范 API-9**：第三类举例删掉 `FillOptions`。第五类举例加上 `FieldFill`、`FieldRule`、`FillRequest`、`AbsoluteLine`。第四类按「名字以 `Style` 结尾」覆盖 `FillStyle`、`FieldStyle`、`StrokeStyle`。
- **API-12**：`Font` 内部的 `Arc<Vec<u8>>` 补一行论证，与 orgchart `Font` 相同：克隆字体不复制面字节。`from_shared` 直接持有调用方的 `Arc`。`from_shared` 与 `from_shared_collection` 不修改调用方的 `Arc`。

### 11.3 不改

- `pdfcraft-forms-otf-spec.md`。
- hcsw 工程里的任何文件。hcsw 若钉着 forms 的旧版本，由它自己决定何时迁移。
- `pdfcraft-pdf`、`pdfcraft-orgchart`、`pdfcraft-post` 的源码。

## 12. 不做

- 改折行、缩字步长、最小字号、内边距、行高系数、勾的形状、子集算法、资源名字。错误 `Display` 的整句可以改，种类名不改。
- 新产品能力：OTF/CFF 成功路径、读 `/Q` `/DA` `/MaxLen` `/Rotate` / CropBox、多行标志、建表单、写 `/V`、写外观流、签名、加密、XFA、单选分组、按 widget 而非按名字填写。
- 给 `line_below` 或勾选单独的颜色、线宽、字号 API。勾选继续用矩形短边，不读 `FillStyle::font_size`。
- `Pt`、`FontSize`、`CheckboxState` 新类型。
- builder、serde 或 JSON 主接口。
- 把矩形放进 `FieldInfo`，或把 `WidgetSnap` 变成公共类型。
- 暴露测宽器、内容流构建器或 `lopdf::Document`。
- 进程级默认字体，或填写用的全局锁。
- 与 `pdfcraft-pdf` 或 `pdfcraft-orgchart` 共享字体类型或错误类型。
- 提交、挂进编译或删掉工作区里未跟踪的 `cff_subset.rs`。
- 修改 hcsw。
- 擅自 `git commit` 或 `push`。

## 13. 迁移示例

`0.2.0`：

```rust
let font_bytes = std::fs::read("face.ttf")?;
let mut fields = std::collections::HashMap::new();
fields.insert("employee_name".into(), FieldValue::Text("Ada".into()));
fields.insert("agree".into(), FieldValue::CheckBox(true));
let filled = fill_and_flatten(
    &pdf,
    &fields,
    &FillOptions {
        font: Some(FontBytes { bytes: &font_bytes, index: 0 }),
        font_size: 14.0,
        h_align_overrides: Some(&align),
        line_below: Some(&rules),
        ..FillOptions::default()
    },
)?;
```

`0.3`：

```rust
let form = Form::open(&pdf)?;
let _widgets = form.fields();
let font = Font::new(font_bytes)?;
let name = FieldFill {
    style: FieldStyle {
        h_align: Some(HAlign::Center),
        ..FieldStyle::default()
    },
    ..FieldFill::text("Ada")
};
let request = FillRequest {
    style: FillStyle {
        font_size: 14.0,
        ..FillStyle::default()
    },
    rules: BTreeMap::from([("employee_name".to_owned(), FieldRule::new(2.0))]),
    ..FillRequest::new(BTreeMap::from([
        ("employee_name".to_owned(), name),
        ("agree".to_owned(), FieldFill::check(true)),
    ]))
};
let filled = Fill::new(&form, request, Some(&font))?.to_pdf()?;
```

只剥离、不填写：

```rust
let form = Form::open(&pdf)?;
let filled = Fill::new(&form, FillRequest::default(), None)?.to_pdf()?;
```

迁移时：旧的 `fields` 变成 `BTreeMap<String, FieldFill>`，值必填，对齐全写进 `style`。只出现在字号或对齐表、没有值的名字丢掉；它们本来就不画。`line_below` 变成 `rules: BTreeMap<String, FieldRule>`，名字可以不在 `fields` 里。字体从 `FillOptions::font` 移到 `Fill::new` 的第三个参数。`absolute_lines` 按原顺序放进 `lines`，线宽和颜色放进 `StrokeStyle`。单个 `sfnt` 用 `Font::new`，`ttcf` 用 `Font::from_collection(bytes, index)`。

## 14. 确认前的检查清单

维护者确认本文时，逐条回答。未回答的条目保持草案，不进提案书。

1. 是否接受删除 `list_fields` 与 `fill_and_flatten`，不留兼容包装。
2. 是否接受 `FieldFill` 必须带值、横线单独成 `FieldRule`、线段笔触收进 `StrokeStyle`，且 `fields` 与 `rules` 是以名字为键的 `BTreeMap`。
3. 是否接受 `Font::new` 与 `Font::from_collection` 分开，单面不再接收 index。
4. 是否接受字体作为 `Fill::new` 的参数，测字与缩字在 `Fill::new` 完成，`to_pdf` 只剩嵌入与写出类失败。
5. 是否接受错误的字段名、页号、字符从 `ErrorKind` 搬到 `field()` / `page()` / `scalar()`，并且 `Display` 整句不再冻结。
6. §6 第 2、3、8、9 条：损坏 PDF 先于非法字号；非有限线段先于缺字；线段页号越界改为参数错误；缺字与放不下从 `Fill::new` 返回。是否接受。
7. 未跟踪的 `cff_subset.rs` 不进入本系列任何 PR，是否同意。
8. D23 与 D24 进入本系列（PR 3 与 PR 2b），以 §9 的字节基线证明输出不变，是否同意。
9. §15 的后续项不进本系列，各自单独立项，是否同意。其中 §15.2 第 2 条会推翻子集规格「保留原 glyph id」的约定，需要单独确认。

## 15. 后续项（不在本系列）

这些改进会改变输出字节、改变已落地的子集合同，或改变依赖边界，所以不进 reshape（D22）。每条单独一个 PR，单独写 CHANGELOG，在 `0.3.0` 发布之后做。排列顺序就是建议的落地顺序。

### 15.1 实测：子集体积

2026-11-05 用 `wqy-zenhei.ttc`（`glyf` 字体集合，16.8 MB，约 4.5 万个字形）跑 `fill_tax_refund_guarantee` 示例，填入约 40 个汉字。模板 178 KB，输出 998 KB。嵌入的 `FontFile2` 共 823,776 字节：

| 表 | 字节 |
| --- | --- |
| `post` | 450,887 |
| `loca` | 179,844 |
| `hmtx` | 179,296 |
| `glyf` | 9,944 |
| 其余 8 张表合计 | 约 3,600 |

真正的字形只占 1.2%。原因有两个。保留原 glyph id 使 `numGlyphs` 不变，`loca` 与 `hmtx` 仍按整套字体分配。`post` 按 `TablePolicy::Copy` 原样复制，这份字体是 format 2，带全部字形名。

### 15.2 条目

1. **`post` 改写成 3.0 版。** PDF 阅读器不读 CIDFontType2 的字形名。`table_policy` 把 `post` 从 `Copy` 改为 `Rewrite`，保留 32 字节表头里的度量字段，只把版本号写成 3.0，并由 sfnt 写出器重算表校验和。按 §15.1 可省约 451 KB。改动最小，收益最大。
2. **子集重新编号字形。** 第 1 条 `post` format 3 必须先落地；否则 format 2 的字形名索引也要随新 id 重写。保留的字形连续编号，子集函数返回旧 id 到新 id 的映射。必须把这张映射用于全部相关位置：重写 `glyf` 中复合字形的组件 id；按新顺序重建 `glyf`、`loca`、`hmtx` 与 `cmap`；更新 `maxp.numGlyphs` 和 `hhea.numberOfHMetrics`；让 `paint`、`/W` 与 ToUnicode 使用新 id。`/CIDToGIDMap` 仍是 `/Identity`。这会推翻 `pdfcraft-forms-font-subset-spec.md` 里「保留原 glyph id」的约定，要先改那份规格并单独确认。按 §15.1 可再省约 35 万字节。
3. **`FontDescriptor` 用真实度量，`BaseFont` 加子集标签。** 今天 `FontBBox` 写死为 `[0 descent 1000 ascent]`，`CapHeight` 用 ascent，`ItalicAngle` 固定为 0。改用 skrifa `Metrics` 的 `bounds`、`cap_height`、`italic_angle`，并换算到千分之一 em。PDF 32000-1 §9.6.4 要求子集字体的 `BaseFont` 以六个大写字母加 `+` 开头。标签由字体面内容与用到的 glyph id 集合共同确定性地推出（DET-1），后面接 `name` 表里的 PostScript 名；没有时仍用 `QuireForms`。
4. **sfnt 读取改用 `skrifa::raw`。** 今天有三份手写的表目录解析：`zero_head_times`、`extract_collection_face`、`subset::parse_sfnt`，严格程度不一致。前两份不查表的重叠与对齐，第二份不查重复的表标签。`skrifa::raw` 就是 read-fonts。`FileRef::new` 区分单个字体与集合，`CollectionRef::get` 取一面，`FontRef::table_data` 按标签取表，`tables::glyf::CompositeGlyph::components()` 可替代 `close_composites` 里手工解析标志位的部分。只换读取，写出继续手写，不加 write-fonts。需要先把 §8.2「除 `font` 以外 `use skrifa`」放宽为允许 `subset` 与 `sfnt` 使用 `skrifa::raw`。应在 OTF 草案落地之前做，免得 CFF 再加一份解析。
5. **去掉 forms 的 `autohint_shaping`。** 它在 skrifa 0.48 里不引入额外依赖，但会切换自动 hinting 内部的 shaping 路径。forms 不调用 outline hinting，因此关闭后不影响输出。同步改 AGENTS.md 里 forms 的 feature 句。
6. **分词的已知限制。** 这一条属于产品取舍，要走提案书。今天任何空白都折成 U+0020 并可断行，包括 NBSP；U+3000 全角空格按半角空格的宽度画；所有码位大于 U+007F 的字符都可断，所以 `Zürich` 可能在 `ü` 后断开。至少写进 guide 的已知限制；要改的话，把逐字可断收窄到 CJK 区段，并让 NBSP 不可断。
