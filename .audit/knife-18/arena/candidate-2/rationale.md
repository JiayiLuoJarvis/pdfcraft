# Knife 18 取舍说明

## 选择摘要

选择 frame 默认方案：

- `enum MarginPreset { Inch, HalfInch }`；
- 精确值分别为四边 72 pt 与 36 pt；
- 通过 `into_margins` / `From<MarginPreset> for Margins` 立即降为现有 `Margins`；
- 调用方继续使用 `doc.margins(...)` / `section.margins(...)`；
- `Margins::default()` 保持四边 0。

没有启用 reverse word：`assoc-consts`、`mm-round`、`Normal`/`Narrow`、
`add-quarter`、`default-inch`、`doc.margin_preset(...)` 均不采用。

## 为什么这样选

### `Margins` 仍是唯一语义值

分页、页面元数据和装饰层都已围绕四个 `Length<NonNegative>` 工作。预置若在
`Section` 或 `PaginateConfig` 中继续存活，就会迫使后续层同时处理“预置”与“已解析
边距”，并可能让自定义值和预置值走出不同语义。

在 `geom` 边界立即转成 `Margins::uniform` 后，预置与手写 72/36 pt 值无法在布局
中分叉。已有正文框校验、section 作用域、页面闭包元数据和 header/footer 带高规则
全部自然复用。

### 枚举比 associated consts 更诚实地表达小目录

`MarginPreset` 明确告诉调用方这是一个有限、可发现、以后可审议扩展的选择目录；
`Margins::INCH` 之类 associated const 容易看成几何常量，与
`Margins::new/uniform/default` 的值构造职责混在一起。独立枚举也让 rustdoc 能集中
解释“inch 是 72 PDF pt，不是中国传统寸”。

`#[non_exhaustive]` 防止外部把当前两项当作永不扩展的封闭业务分类。调用方的正常
动作是转换成 `Margins`，无需匹配枚举。

### 精确 72/36 pt，而不是 mm 往返

PDF 点定义为每国际英寸 72 点，因此 `Inch = 72 pt`、`HalfInch = 36 pt` 是直接且
无舍入的合同。先写 25.4/12.7 mm 再乘 `MM_TO_PT` 只会引入不必要的浮点路径，也会
模糊公开数值。

提案中的“约一寸”适合作为中文说明，不适合作为传统长度承诺。`Inch` /
`HalfInch` 同时给出单位与比例，比 `Normal` / `Narrow` 更稳定：后两者会随组织、
纸张和模板习惯改变含义。

### 保持现有 `margins(Margins)` 精确签名

把参数泛化成 `impl Into<Margins>` 对本刀没有必要。`.into()` 在现有参数位置已有
目标类型推断，`Margins::from(...)` 也提供完全显式的写法。保留精确签名有三个
好处：

1. 不扩大 `Document` 与 `Section` 的公共泛型表面；
2. 不增加 `doc.margin_preset` 这类只转发一次的平行方法；
3. API 继续表明最终设置的是完整四边值，而不是长期保存一个 preset。

### 不给 `MarginPreset` 实现 `Default`

§7 已确认默认边距是四边 0。即便 `Margins::default()` 不变，若
`MarginPreset::default()` 返回 `Inch`，也会制造“库推荐默认值到底是哪一个”的第二
答案。调用方必须显式选择预置；不选择时仍得到零边距。

### 首批只提供两个统一预置

72 pt 与 36 pt 正是提案点名的两档，足以验证公共命名和转换 seam。此时加入
quarter-inch、非对称公文模板或地区规范，只会提前承诺未经确认的产品目录。自定义
`Margins::new` 已覆盖这些需求，不需要本刀猜测。

## 被拒绝的方案

### `assoc-consts`

拒绝该 reverse word。`Margins::INCH` / `Margins::HALF_INCH` 虽然少一个根级类型，
但把可增长的产品目录混进核心几何值命名空间，也没有枚举清楚地表达“从 preset
解析为 Margins”的边界。

### `mm-round`

拒绝该 reverse word。72 pt 已精确等于一国际英寸；经 mm 换算没有额外语义，只增加
浮点舍入机会。测试应锁精确 pt 值。

### `Normal` / `Narrow`

拒绝该 reverse word。这两个名称没有跨项目稳定的数值含义，也无法让调用者从调用
点判断究竟选择了 72 还是 36 pt。单位型名称更适合作为长期合同。

### `add-quarter`

拒绝该 reverse word。18 pt 未被 §7 点名，也没有当前夹具证明它是高频文档预置。
需要时调用方可以写 `Margins::uniform(NonNegative::pt(18.0)?)`；新增公开 variant
应由后续需求驱动。

### `default-inch`

拒绝该 reverse word。它直接违反 §7 的四边 0 默认，并会让新 section、空文档、
正文框及 header/footer 带高全部静默变化。

### `doc.margin_preset(...)`

拒绝该 reverse word。它需要在 `Document` 与 `Section` 各复制一个入口，却只做
一次 `Into<Margins>`。转换后调用现有 `margins` 更小，也不会造成 custom 与 preset
两条设置路径。

### 把 `margins` 改成 `impl Into<Margins>`

不采用。虽然能把调用点缩为 `doc.margins(MarginPreset::Inch)`，但会为一个两项糖
修改两个既有签名。显式 `.into()` 很短，并更清楚地显示 preset 到语义值的边界。

### 数值型 variant（如 `Pt72` / `Pt36`）

不采用。它们把实现单位编码进产品名称，却弱化了两档之间的一英寸/半英寸关系。
精确 pt 数值应在 rustdoc 和测试中承诺，调用名则表达调用者的选择意图。

### 非对称或文种专用预置

不采用。此类预置需要命名、规范来源和适用纸张的独立依据。现有
`Margins::new(top, right, bottom, left)` 已是无损逃逸口，本刀无需扩大目录。

## 风险与约束

- `Inch` 可能被中文读者误解为传统“寸”；rustdoc 必须明确 international inch 和
  精确 72 PDF pt。
- 预置并不保证适用于任意 custom page。四个单边值合法后，仍可能在跨字段校验中因
  正文框宽高不为正而失败；测试必须保留这一点。
- 选择预置同时选择 top/bottom 装饰带高。不能把它宣传成只影响正文四周留白。
- 显式 section 各自从零边距开始，不继承前一 section 的 preset；集成测试应锁住
  独立选择。
- 当前 `Band::left/right` 自动使用左右边距内缩，与提案 §2.10 的全页宽文字仍有
  独立契约差异。预置测试只能记录现状，不能借此决定或修复该问题。
- 新枚举需要根级重导出，否则示例会被迫使用 `quire_pdf::geom::MarginPreset`，与
  当前 `Margins` 的调用面不一致。

## 最小实施边界

生产实现只需触及 `src/geom.rs` 和 `src/lib.rs`；公共路径测试位于
`tests/document.rs`。`document.rs`、layout、canvas、backend 及依赖清单均不应有
功能改动。若实现发现必须让 preset 穿过 `Section::lower`，说明边界设计已经偏离本
方案，应停止而不是扩刀。
