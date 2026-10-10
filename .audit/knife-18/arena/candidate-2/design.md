# Knife 18 设计：可选页边距预置

## 结论

新增公开枚举 `MarginPreset`，首批只有 `Inch` 和 `HalfInch`。它们分别精确解析为
四边统一 72 pt 和 36 pt 的 `Margins`。预置只是一种构造糖：在进入
`Document::margins` 或 `Section::margins` 前就转换成现有 `Margins`，不进入
document、layout、decorate 或 backend 的数据模型。

`Margins::default()` 仍为四边 0；`MarginPreset` 不实现 `Default`。本设计采用全部
autonomy defaults，没有启用 reverse word。

## 1. 调用方用法

隐式单 section 文档：

```rust
use quire_pdf::{document, MarginPreset, PageSize};

let rendered = document(PageSize::A4, |doc| {
    doc.margins(MarginPreset::Inch.into());
    doc.paragraph(|p| p.text("正文"));
    Ok(())
})?;
```

显式 section 可各自选择预置或继续使用自定义四边值：

```rust
use quire_pdf::{MarginPreset, Margins, NonNegative};

doc.section(|section| {
    section.margins(Margins::from(MarginPreset::HalfInch));
    section.paragraph(|p| p.text("第一篇"));
    Ok(())
})?;

doc.section(|section| {
    section.margins(Margins::new(
        NonNegative::pt(24.0)?,
        NonNegative::pt(36.0)?,
        NonNegative::pt(48.0)?,
        NonNegative::pt(36.0)?,
    ));
    section.paragraph(|p| p.text("第二篇"));
    Ok(())
})?;
```

`doc.margins(...)` 和 `section.margins(...)` 的参数类型保持精确的 `Margins`，不改成
`impl Into<Margins>`，也不增加 `margin_preset(...)` 平行入口。调用处的 `.into()`
由既有参数类型完成推断；想强调转换时可写 `Margins::from(...)`。

## 2. 类型与转换

公开形状：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[must_use]
pub enum MarginPreset {
    /// Exactly 72 PDF points (one international inch) on every side.
    Inch,
    /// Exactly 36 PDF points (one half international inch) on every side.
    HalfInch,
}

impl MarginPreset {
    pub const fn into_margins(self) -> Margins;
}

impl From<MarginPreset> for Margins {
    fn from(value: MarginPreset) -> Self;
}
```

`into_margins` 的语义固定为：

```text
Inch     -> Margins::uniform(72 pt)
HalfInch -> Margins::uniform(36 pt)
```

实现位于 `geom`，可用该模块已有的私有、仅供编译期常量使用的
`Length::from_trusted_pt` 建立两个已知合法的 `Length<NonNegative>`，再调用
`Margins::uniform`。这使 `into_margins` 可以是 `const fn`，且不为调用方开放绕过
长度校验的入口。`From` 是总转换，不返回 `Result`：两个枚举值的有限、非负数值由
库本身保证。

枚举标为 `non_exhaustive`，调用方应把它作为选择 token 并立即转成 `Margins`，
而不是穷举其内部目录；以后若经单独决策增加预置，不必破坏外部穷举。枚举不实现
`Default`，避免暗示 `Inch` 或 `HalfInch` 是文档默认值。

## 3. 语义与数据流

```text
MarginPreset
  -> MarginPreset::into_margins / Margins::from
  -> Margins { top, right, bottom, left }
  -> Document::margins / Section::margins
  -> 现有 Section::lower
  -> 现有 PaginateConfig
```

转换后不保留“来自预置”的标记。因此预置与手写等值 `Margins` 在以下方面完全相同：

- 左右边距收窄正文框，并决定正文的 x 原点；
- 上下边距收窄正文框，并分别充当 header/footer 固定带高；
- 有效值通过 `Page::margins` 暴露给逐页闭包；
- 自定义小纸张若被四边挤到正文宽或高不为正，仍由现有
  `PaginateConfig::validate` 返回 `InvalidGeometry`；
- 显式 section 不继承其他 section 或隐式 section 的边距，每个新 section 仍从
  四边 0 开始。

名称中的 inch 只指 PDF 单位定义下的国际英寸：`1 in = 72 pt`。rustdoc 可说明它
在常见公文语境中对应“约一寸”留白，但不能把 `Inch` 描述成中国传统长度单位。

## 4. 模块图

| 位置 | 变更 |
| --- | --- |
| `src/geom.rs` | 定义 `MarginPreset`、`into_margins`、`From<MarginPreset> for Margins`；复用 `Margins::uniform`；增加转换单测 |
| `src/lib.rs` | 与 `Margins` 一起根级重导出 `MarginPreset` |
| `src/document.rs` | 不改；两个 `margins(Margins)` 入口原样复用 |
| `src/layout/paginate.rs` | 不改；继续只接收 `Margins` 并做跨字段几何校验 |
| `src/layout/decorate.rs` | 不改；top/bottom 继续是 header/footer 带高 |
| `tests/document.rs` | 增加公共调用路径、section 作用域、等值渲染与小纸张失败测试 |
| backend / canvas / fonts / text | 不改 |

不新增模块或依赖。公开文档放在枚举、两个 variant 和转换方法上；本刀不要求
CHANGELOG。

## 5. 测试计划

### 5.1 `geom` 单元测试

1. `inch_margin_preset_is_uniform_72pt`
   - 断言 `Margins::from(MarginPreset::Inch)` 等于
     `Margins::uniform(NonNegative::pt(72.0).unwrap())`；
   - 同时逐边断言 top/right/bottom/left，防止转换顺序错误。
2. `half_inch_margin_preset_is_uniform_36pt`
   - 同样断言四边精确为 36 pt。
3. 保留并继续运行 `default_margins_are_zero`
   - 明确锁住新增预置后 `Margins::default()` 四边仍为
     `NonNegative::ZERO`。

### 5.2 公共 document 测试

1. `document_accepts_margin_preset_as_margins`
   - 经根级重导出的 `MarginPreset::Inch.into()` 调用 `doc.margins`；
   - 用 `RecordingCanvas` 断言正文首个 glyph run 的 x 从 72 pt 开始，页面闭包看到
     的四边也都是 72 pt。
2. `explicit_sections_choose_presets_independently`
   - 两个 section 分别使用 `Inch` 与 `HalfInch`；
   - 断言各 section 首页正文 x 分别为 72 pt、36 pt，证明预置不被继承或串用。
3. `preset_and_equivalent_custom_margins_render_identically`
   - 同一模板分别使用 `MarginPreset::HalfInch.into()` 与手写 uniform 36 pt；
   - 比较 `RecordingCanvas` 页面命令、链接矩形及页数，不比较压缩 PDF bytes。
4. `margin_preset_still_validates_against_custom_page`
   - 在宽或高小于等于 144 pt 的 custom page 上使用 `Inch`；
   - 断言现有 `InvalidGeometry`，证明预置没有绕开正文框校验。
5. `preset_top_and_bottom_are_decoration_band_heights`
   - 使用 36 pt 预置绘制可容纳的 header/footer；
   - 断言 header 位于页顶带、footer 以 `page_height - 36` 为带起点，并确认正文
     y 从 36 pt 开始。

所有正向渲染断言走现有公共 `document(...).pages()` 与 `RecordingCanvas`，不增加
backend 字节级断言。实现后运行 `cargo fmt --all`、
`cargo clippy --all-targets -- -D warnings`、`cargo test`。

## 6. 明确不在本刀范围

- 不把任一预置设为默认；默认四边仍为 0。
- 不增加 `QuarterInch` 或任何非对称预置。
- 不采用 `Normal`、`Narrow` 等依赖语境的名称或别名。
- 不按 25.4 mm 运行时换算预置；72/36 pt 是公开的精确数值合同。
- 不增加 `doc.margin_preset` / `section.margin_preset`，也不泛化现有
  `margins(Margins)` 签名。
- 不新增第二种 margin 存储、preset 字段或 layout node。
- 不改变 section 继承、隐式/显式 section 所有权和 `ContentOutsideSection` 规则。
- 不调整 header/footer 带高、正文框或分页算法。
- 不借本刀修正当前 `Band::left/right` 自动按左右边距内缩与提案 §2.10 全页宽描述
  的差异。
- 不改纸张枚举、方向或 custom page 设计；这些属于 knife 19。
