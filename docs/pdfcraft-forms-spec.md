# pdfcraft-forms 规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-12 |
| 状态 | **已与提案书 §10 对齐，2026-10-12 确认。** 公共 API 已在 `crates/pdfcraft-forms`。成功路径嵌入保留原 glyph id 的 `glyf` 子集。OTF 仍未做 |
| 相关 | [`workspace-crates-spec.md`](workspace-crates-spec.md)；[`提案书.md`](提案书.md) §10 |
| 意图来源 | pdfcraft-forms 的行为合同（bytes → flatten）。API 按本仓库重做，不逐文件翻译 |

本文是 **reference**。产品面以提案书 §10 为准。实现细节不在这里钉死。

## 1. 产品一句话

`pdfcraft-forms` 读取已有 AcroForm 的第三方 PDF 字节，按调用方给出的字段值在控件矩形上绘制，并始终输出已 flatten 的 PDF 字节。

它不生成表单模板，也不属于 `pdfcraft-pdf` 的排版管线。

## 2. 范围

### 2.1 做

- 列出 PDF 中的 AcroForm widget（按控件，不是逻辑字段树）。
- 填写 **Text**、**CheckBox**、**ListBox**、**ComboBox**。
- 每次成功调用都 flatten（含空字段表）。
- 管线为 **overlay 绘制 + 剥离 Widget 与 `/AcroForm`**。不写字段 `/V`，不维护外观流。
- 缺字、框内放不下（缩到最小字号后仍溢出）硬失败。
- 拒绝加密输入、带 `/XFA` 的 AcroForm、以及填写 Signature / RadioButton / PushButton。

### 2.2 不做

- 创建或编辑空白表单结构。
- XFA 运行时、数字签名写入、加密写入。
- 读 `/Q`、`/DA`、`/MaxLen`、多行标志、`/Rotate`、CropBox（v1）。
- HTTP 适配、模板磁盘注册表、政府表单字段名表。
- 依赖或反向依赖 `pdfcraft-pdf`；使用 `krilla`；把第二套 PDF 栈引进 `pdfcraft-pdf`。
- 从 `pdfcraft-pdf` 抽共享测量 crate（v1）。两边可各自钉同一版本的 `skrifa`。
- 进程级默认字体、全局 fill 锁（除非日后证明存在不得不共享的可变字体注册表）。

### 2.3 与 `pdfcraft-pdf` 的边界

| | `pdfcraft-pdf` | `pdfcraft-forms` |
| --- | --- | --- |
| 输入 | `DocumentSpec` | 已有 PDF 字节 + 字段值 |
| 坐标 | 左上原点，y 向下 | PDF 用户空间，左下原点，y 向上 |
| PDF 栈 | krilla（仅 backend） | 独立栈（见 §6） |
| 表单 | 不做 AcroForm | 只做填已有 AcroForm |

`pdfcraft-pdf` 正文里的填空下划线（如 `BlankWidth`）不是 AcroForm，与本 crate 无关。

## 3. 公共 API

命名以 Rust 习惯为准。下列签名是提案书 §14 确认过的形状。不改变语义时，实现可以调整可见性与模块路径。

### 3.1 入口

```rust
impl Form {
    pub fn open(pdf: &[u8]) -> Result<Self, Error>;
    pub fn fields(&self) -> &[FieldInfo];
}

impl Fill {
    pub fn new(form: &Form, request: FillRequest, font: Option<&Font>) -> Result<Self, Error>;
    pub fn to_pdf(&self) -> Result<Vec<u8>, Error>;
}
```

- PDF 只接受内存字节。模板路径、缓存、HTTP 在应用层。
- 字体在 `Font::new` 或 `Font::from_collection` 解析（见 §3.4）。`Fill::new` 只在有非空文本要画时读取它。公共 API 不提供 `set_default_font`。
- `Fill::new` 做请求与数据检查，并完成测字、折行和缩字。`to_pdf` 子集、清零子集 `head`、叠绘、剥离并写出。同一份 `Fill` 调用两次 `to_pdf`，字节相同。

### 3.2 `FieldInfo`

`Form::fields` 对每个 widget 返回一条（同名多框 → 多条）。字段私有，用访问器读取。

| 访问器 | 类型 | 含义 |
| --- | --- | --- |
| `name()` | `&str` | `/T`（沿 Parent 继承，见 §4.1） |
| `field_type()` | `FieldType` | 见下 |
| `page_index()` | `usize` | 0-based 页码 |
| `options()` | `Option<&[String]>` | ListBox/ComboBox 的合法值；其它为 `None` |

公开 `FieldInfo` **不含** 矩形与 checkbox on-state。矩形与 on-state 仅内部 fill 使用。

### 3.3 `FieldType` / `FieldValue`

```text
FieldType = Text | CheckBox | ListBox | ComboBox | Signature | RadioButton | PushButton

FieldValue =
  | Text(String)        // Text / ListBox / ComboBox 的填写值
  | CheckBox(bool)      // 仅 CheckBox
```

- 不提供松散的 `str | bool | int | float` 联合，避免把 `false` 画成文本 `"False"`。
- ListBox/ComboBox 使用 `FieldValue::Text`；值必须落在展平后的 `/Opt` 中（export 与 display 皆合法）。画出的是调用方字符串，不改写成 display。
- 把 Signature 放进 `FillRequest::fields` 会得到 `SignatureNotWritable`。把 RadioButton 或 PushButton 放进 `fields` 会得到 `WidgetUnsupported`。没有放进 `fields` 的这些控件，flatten 时和其它未填控件一起剥掉。
- `CheckBox(true)` 画勾。`CheckBox(false)` 不画。`FieldType` 与 `FieldValue` 可穷尽匹配。

### 3.4 请求与字体

`FillStyle` 的默认是 12 pt、左对齐、底对齐。单个字段的覆盖在 `FieldFill.style`（`FieldStyle`）。`None` 沿用文档样式。`fields` 与 `rules` 都是 `BTreeMap`，键按字节序比较。同一个名字可以同时出现在两张表里。

| 条目 | 含义 |
| --- | --- |
| `FillRequest::fields` | 字段名 → `FieldFill`（值必填） |
| `FieldStyle` | 可选字号、水平对齐、垂直对齐。挂在勾选上时仍检查字号，但不改变勾的几何 |
| `FillRequest::rules` | 字段名 → `FieldRule`。框底下方一条与框同宽的 1 pt 黑线。不需要字体，也可以没有对应的 `FieldFill` |
| `FillRequest::lines` | `AbsoluteLine`。笔触在 `style: StrokeStyle`，默认红色 0.8 pt |
| `FillRequest::missing` | `Error`：未知名字失败。`Ignore`：跳过未知名字 |
| `Font::new` / `Font::from_shared` | 一个 `sfnt`。字节以 `ttcf` 开头则失败。构造不改调用方字节 |
| `Font::from_collection` / `Font::from_shared_collection` | 从 `ttcf` 抽出 `index` 这一面。不是 `ttcf` 则失败 |

`FillStyle`、`FieldStyle`、`FieldRule`、`AbsoluteLine` 都已实现。语义以本表和提案书 §14 为准。

### 3.5 缩字与折行常量（v1 默认）

`12.0` 只出现在 `FillStyle::default`。最小字号、步长、行高系数和内边距留在 `layout` 的私有常量里：

| 常量 | 值 |
| --- | --- |
| 最小字号 | `7.0` pt |
| 缩字步长 | `0.5` pt |
| 行高系数 | `1.2` × 字号 |
| 内边距 | 左/右 1 pt，底 1 pt，顶 0.5 pt |

折行规则：

- 码位 `> 0x7F`：可按字符断。
- ASCII：按词断；超宽词再按字符硬断。
- 显式 `\n` 分段。
- 最小字号下单字仍宽过框 → `BoxOverflow`。

文字与勾选绘制均 clip 到 widget `/Rect`。

## 4. 行为合同

### 4.1 Widget 发现

1. 打开 PDF；加密 → 失败。
2. Catalog → AcroForm；存在 `/XFA` 键（含空数组）→ 失败。
3. 遍历各页 `/Annots`，取 `/Subtype /Widget`。
4. 缺少 `/T` 或 `/Rect` 的 annot 跳过。
5. `/T`、`/FT`、`/Ff`、`/Opt`、`/AP`：沿 `/Parent` 取**第一个已有键**，不拼接点分全名。
6. `/Rect` 规范化为 `(x0,y0,x1,y1)`（颠倒边先交换），坐标在 PDF 用户空间。
7. 类型判定：
   - 无 `/FT` → Text  
   - `/Btn`：Push 位 → PushButton；Radio 位 → RadioButton；否则 CheckBox  
   - `/Ch`：Combo 位 → ComboBox；否则 ListBox  
   - `/Sig` → Signature  

`Form::fields` 可返回 Signature 等类型供探查。`Fill::new` 拒绝填写它们。

### 4.2 Flatten

- 每次成功的 `Fill::to_pdf` 都剥离全部 Widget 注释与 catalog `/AcroForm`。
- 未出现在本次绘制中的控件区域不保留原有外观；预印页面内容保留。
- 空 `FillRequest` 仍执行剥离（结果为无表单的 PDF）。
- 同名多框使用同一条 `FieldFill` 和同一条 `FieldRule`；类型与 `/Opt` 校验以遍历到的**第一个**同名 widget 为准。

### 4.3 字体与缺字

- 至少一个将绘制的文本或 choice 字符串时，必须提供可用 TrueType（含 `glyf`）字体。单面用 `Font::new`，集合用 `Font::from_collection`。
- 有文本而 `font` 为 `None` → `FontNotConfigured(FontReason::Missing)`，发生在 `Fill::new`。字体构造失败也是 `FontNotConfigured`。解析失败是 `FontReason::Unreadable`，解析器错误留在 `source`。
- `to_pdf` 成功时，`FontFile2` 嵌入用到的 glyph 的 `glyf` 子集，并保留原 glyph id。子集的 `head` 时间戳清零。构造 `Font` 时不清零调用方字节。
- cmap 缺字 → `MissingGlyph`，从 `Fill::new` 返回，不输出降级 PDF。
- 无内置 CJK 回退字体。
- 同一份 `Fill` 两次 `to_pdf` 得到相同字节。

### 4.4 Checkbox

- `true`：在框内绘制勾选标记（clip 到矩形）。推荐路径或标准 dingbat，视觉对标 ZapfDingbats 字符 `"4"`、短边 0.7 倍字号居中。
- `false`：不绘制。
- 其它 `FieldValue` 变体 → `InvalidCheckbox`。

### 4.5 未知字段名

当 `missing = Error` 时，`FillRequest::fields` 的键先检查，然后是 `rules` 的键。组内按 `BTreeMap` 的字节序，第一个不在快照里的名字失败（`UnknownField`，`field()` 为该名字）。

`lines` 不参与字段名检查。线段 `page_index` 不小于文档页数时，`Fill::new` 返回 `InvalidArgument(PageOutOfRange)`，`page()` 为该下标。

`missing = Ignore` 仍跳过未知名字，并且不检查它们的字号和偏移。表单上存在的名字若字号或 `FieldRule::offset` 不是有限数，仍是 `InvalidArgument`，`field()` 为该名字。

## 5. 错误模型

形状对齐 `pdfcraft-pdf` 的习惯，类型留在本 crate，**不** `use pdfcraft_pdf::Error`。

- `Error` 字段私有。`kind()` 返回种类。`field()`、`page()`、`scalar()` 返回位置。可选 `source`。
- `Error` 不实现 `PartialEq`。没有公共 `Error::new`，也没有 `From<ErrorKind>`。
- `Display` 供人读。整句不是合同，且不包含 `source()` 的文本。
- 不提供面向 HTTP 的 `to_dict`；应用层自行映射。

确认的 `ErrorKind` 名称与提案书 §14 相同。只有一种故障时，名字与 `0.2.0` 相同。线段页号越界除外：它是 `InvalidArgument(PageOutOfRange)`，不再是 `FlattenFailed`。

| Kind | 何时 |
| --- | --- |
| `NotAPdf` | 无法当 PDF 打开。`Form::open` |
| `Encrypted` | 加密输入。`Form::open` |
| `HasXfa` | AcroForm 含 `/XFA`。`Form::open` |
| `UnknownField` | 未知字段名（见 §4.5）。`field()` 为该名字 |
| `IllegalChoice` | 值不在 `/Opt`。`field()` 为该名字 |
| `InvalidCheckbox` | checkbox 值类型非法。`field()` 为该名字 |
| `InvalidArgument` | 非正字号、非有限数、勾选值用在文本或选项上，或线段页号越界。页号在 `page()` |
| `MissingGlyph` | 缺字。`field()` 与 `scalar()`。`Fill::new` |
| `BoxOverflow` | 最小字号仍超框。`field()`。`Fill::new` |
| `FontNotConfigured(FontReason)` | 需字体而未提供，或字体不可用。缺字体在 `Fill::new`。坏字体在 `Font` 的构造函数 |
| `SignatureNotWritable` | 试图填写 Signature。`field()` 为该名字 |
| `WidgetUnsupported` | 试图填写 RadioButton 或 PushButton。`field()` 为该名字 |
| `FlattenFailed` | overlay 合并或写出失败。`to_pdf` |

## 6. 依赖（crate 内）

目标尽量少。

| 角色 | 版本 | 备注 |
| --- | --- | --- |
| 读结构 / merge / 剥表 | `lopdf = "=0.45.0"`，`default-features = false` | 已用夹具验证可列字段 |
| 量宽 / glyphs | `skrifa = "=0.48.0"` | 与 `pdfcraft-pdf` 同钉，不依赖该 package |
| overlay 绘制 | `lopdf` 拼内容流 | **不**默认引入 `pdf-writer` / `printpdf` |
| 字体子集 | crate 内实现 | 成功路径嵌入用到的 glyph 的 `glyf` 子集，并保留原 glyph id。缺字仍然失败 |

禁止：`krilla`、`pdfcraft-pdf`、运行时 PyMuPDF。

版本是 `0.3.0`，可以发布。产品范围已由提案书 §10 与 §14 确认。OTF 仍未做。

## 7. 测试合同

- 夹具放在 `crates/pdfcraft-forms/tests/fixtures/`（basic / chinese / signature / xfa）。可复制或重生成；记录出处。不把 PyMuPDF 列为运行依赖。
- 断言优先：输出以 `%PDF` 开头；flatten 后无 catalog `/AcroForm`；`pdftotext` 或内容检索含填入文本；错误 kind；同输入两次字节相等；空 map 剥离全部 widget。
- 不移植 pdfcraft 的生产模板测例（NAR1、租约等）与 HTTP 映射测例。
- CI：在现有 forms clippy / rustdoc 之外增加 `cargo test -p pdfcraft-forms`。

## 8. 确认清单

提案书 §10 已确认下列范围。文末总表仍把加密、书签，以及在 `pdfcraft-pdf` 里生成或填写交互表单，列为不做。填写已有 AcroForm 的产品是 `pdfcraft-forms`。TrueType `glyf` 子集已实现并保留原 glyph id。OTF 仍未做。

- [x] §1 与 §2 的范围和边界
- [x] §3 的公共 API，含字体字节，不含全局默认字体
- [x] §4 的行为合同，含绘制后剥离、空字段表也 flatten、同名规则
- [x] §5 的错误模型
- [x] §6 的依赖。`lopdf` 与 `skrifa` 已钉版本。TrueType `glyf` 子集已实现
- [x] [`workspace-crates-spec.md`](workspace-crates-spec.md) 里的产品状态与 §10 一致

## 9. 附录：相对 pdfcraft-forms 的有意差异

| pdfcraft-forms | pdfcraft-forms（本规格） |
| --- | --- |
| `set_default_font` 进程默认 | `Font::new` 或 `Font::from_collection`，传给 `Fill::new` |
| `FormFieldValue = str\|bool\|int\|float` | 类型化 `FieldValue` |
| 异常树 + `to_dict` | `Error` / `ErrorKind`，无 HTTP 面 |
| 字体路径 | 字体字节 |
| reportlab 全局字体表 + 进程锁 | 默认无全局锁 |
| 生产模板 / IRD `line_below` 语义 | 只保留通用 knobs；业务在应用层 |

行为对齐（应保留）：bytes in/out、总是 flatten、overlay 不写 `/V`、缺字/超框硬失败、同名多框同值、`/Opt` export+display、拒绝加密与 XFA。
