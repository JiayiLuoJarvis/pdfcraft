# Knife 18 Arena Cross-Judge

## 结论

推荐以 **candidate 2** 为基底。四个候选的核心方向其实一致；candidate 2 的优势是调用示例最贴近当前生产 API（`quire_pdf`、`PageSize::A4`），边界也完整收敛在 `geom` + crate-root re-export。candidate 3 次之，设计和测试尤其扎实，但 usage 中把 crate 写成了不存在的 `quire`。

最终建议再收紧一处：公开实现 `From<MarginPreset> for Margins`，由标准库 blanket impl 自动得到 `Into<Margins>`；不要直接实现 `Into`，也不要增加 `doc.margin_preset(...)`。若没有明确的 const-context 用例，可不公开重复的 `into_margins` 方法。

## Candidate 1

| 准则 | 分数 | 评语 |
| --- | ---: | --- |
| 1. Proposal fidelity | 5 | 默认仍为四个零；只提供精确 72/36 pt 的四边统一预置。 |
| 2. Caller surface | 3 | enum、`From` 和既有 `margins(Margins)` 的形状正确，但主示例使用了不存在的 `quire` 与 `PageSize::a4()`，不能直接作为公共 API 文档。 |
| 3. Honesty | 5 | 明确 top/bottom 同时是 header/footer 带高，没有第二种 margin 表示。 |
| 4. Fit to seams | 5 | 只要求 `geom.rs`、`lib.rs` 和测试改动；preset 在进入 document 前即降为 `Margins::uniform`。 |
| 5. Testability | 5 | 值相等、默认零、section 作用域和 RecordingCanvas 等值渲染都有明确 oracle。 |
| 6. Laziness | 5 | 两档、无 mm 糖、无专用 setter、无 layout/backend/CHANGELOG 扩张。 |
| **合计** | **28/30** | 架构正确；主要问题是 caller sketch 没有按当前 API 校对。 |

## Candidate 2

| 准则 | 分数 | 评语 |
| --- | ---: | --- |
| 1. Proposal fidelity | 5 | 完整保持零默认，并把 72/36 pt 明确解析成四个 `Length<NonNegative>`。 |
| 2. Caller surface | 5 | `MarginPreset::{Inch, HalfInch}` 经 `From` 进入现有 document/section setter；示例使用当前 crate 名和 `PageSize::A4`。 |
| 3. Honesty | 5 | 对 decoration band、section 不继承和小纸张 `InvalidGeometry` 的说明准确。 |
| 4. Fit to seams | 5 | `#[non_exhaustive]` enum 位于 `geom`，根级重导出；document/layout/backend 都不需要生产改动。 |
| 5. Testability | 5 | unit equality、默认零、页面坐标、section 隔离、等值 recording 与小纸张失败均可由现有夹具证明。 |
| 6. Laziness | 5 | 只有两档，无额外单位、依赖或产品语义；测试虽较全，但没有扩大生产面。 |
| **合计** | **30/30** | 最适合作为实现底稿；只需决定是否删去重复的公开 `into_margins`。 |

## Candidate 3

| 准则 | 分数 | 评语 |
| --- | ---: | --- |
| 1. Proposal fidelity | 5 | 数值、零默认、统一四边和可选糖均与 §7 一致。 |
| 2. Caller surface | 4 | API 形状简洁，且 `PageSize::A4`、字段式 `page.margins` 等细节准确；但示例导入写成 `quire`，当前包应为 `quire_pdf`。 |
| 3. Honesty | 5 | 对 band 高度、Band 当前水平 auto-inset、page-coordinate layer 和 geometry validation 的边界说明最具体。 |
| 4. Fit to seams | 5 | preset 明确在 geom 边界终止，生产模块图和当前依赖方向一致。 |
| 5. Testability | 5 | 三组测试集中证明值等价、document/section 行为和既有几何错误，没有依赖 PDF 压缩字节。 |
| 6. Laziness | 5 | 测试集合比其他候选更集中，且没有加入第三档或平行 DSL。 |
| **合计** | **29/30** | 几乎可并列；修正 crate 名后是很好的精简版实施计划。 |

## Candidate 4

| 准则 | 分数 | 评语 |
| --- | ---: | --- |
| 1. Proposal fidelity | 5 | 核心产品选择全部正确：零默认、两档、72/36 pt、统一四边。 |
| 2. Caller surface | 2 | 多个示例与当前 API 不符：`quire`、`PageSize::a4()`、双参数 `PageSize::custom`、`Band::left/right` 传 closure、`page.margins()`，以及把 unit variant `Error::InvalidGeometry` 当 struct variant 匹配。 |
| 3. Honesty | 4 | margin/band 语义正确，但“layout regressions are mathematically impossible”是不成立的过强保证；等值数据路径仍可能因 wiring 或测试实现出错。 |
| 4. Fit to seams | 5 | 生产边界仍然正确地限制在 geom/re-export，不要求 layout/backend 感知 preset。 |
| 5. Testability | 3 | 覆盖面大，但 50 pt header line box、错误 variant pattern、宽松的 glyph `>=` 等计划需先按现有夹具重写。 |
| 6. Laziness | 4 | 生产 API 很小，但五个 unit 加六个 integration 的矩阵、trait 行为测试和大量重复说明超过本刀所需。 |
| **合计** | **23/30** | 核心设计可用，但 public usage 与测试计划需要较多校正。 |

## 推荐基底与 graft

以 candidate 2 为主，保留：

1. `MarginPreset` 放在 `geom`，crate root 与 `Margins` 一起重导出。
2. `#[non_exhaustive] enum MarginPreset { Inch, HalfInch }`。
3. 精确 72.0/36.0 pt，经私有 trusted constants 和 `Margins::uniform` 解析。
4. `Margins::default()` 与 `Section::new` 继续为四边零；document/layout/decorate/backend 不改。

建议 graft：

- 从 candidate 3 取更集中的测试组合：一组 geom equality/default 测试，一组 document + explicit section recording 测试，再加一组 tiny custom page 的 `InvalidGeometry`；避免把既有 band 行为整套重测。
- 从 candidate 1 取 rustdoc 中“选择 preset 同时选择 top/bottom header/footer band 高度”的醒目说明。
- 不 graft candidate 4 的额外 trait 测试和大测试矩阵；`Hash` 也没有当前需求。

## 命名与转换结论

- **命名：** 采用 `Inch` / `HalfInch`，不要用含糊且容易暗示默认值的 `Normal` / `Narrow`。rustdoc 应写“exactly 72/36 PDF points，international inch”；“约一寸”只作提案语境说明，不宣称中国传统寸。
- **From / Into：** 实现 `From<MarginPreset> for Margins`；调用方自然获得 `.into()`。不要直接实现 `Into<Margins>`。
- **`into_margins`：** 它与 `From` 语义重复。只有确认需要 `const M: Margins = MarginPreset::Inch.into_margins()` 时才公开；当前 rubric 没有该需求，较懒的 API 是只保留 `From`。
- **setter：** 保持 `Document::margins(Margins)` / `Section::margins(Margins)` 精确签名。不要增加 `margin_preset`，也不要改成 `impl Into<Margins>`；`doc.margins(MarginPreset::Inch.into())` 已足够清楚。
