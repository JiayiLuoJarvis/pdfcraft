===== C1 RATIONALE Shape+Alts =====
## Shape

**核心主张：一个泛型 `Length<K>` + 三个密封标记类型 `Finite` / `NonNegative` / `Positive`。**

- 内部统一存 `f32` pt；`K` 只在类型层携带不变量，避免三个互不相关的 newtype 复制同一套 `from_pt`/`from_mm`/校验逻辑。
- **升级链**：`Finite → assume_non_negative → NonNegative → assume_positive → Positive`；降级不提供，避免 silent 变宽。
- **几何体字段私有**：`Point`、`Size`、`Rect`、`Margins` 不暴露 public f32；对外只给 typed getter 与 `from_pt` 便捷构造（内部仍走 `Length`）。
- **纸张常量**：`A4` 必须由 **210 mm × 297 mm × `MM_TO_PT`** 生成并单测 f64 参照；`LETTER` 用精确 612×792 pt（英寸定义，不走 mm 舍入）。
- **错误面**：单一 `Error::InvalidLength(InvalidLength { NonFinite | Negative | NotPositive })`；跨字段关系（边距和 > 页宽）**不在本 slice**。

模块拆成 `geom::{length, point, size, rect, margins, color, path, paint}`，公共面深而窄：`Length` 三别名、`MM_TO_PT`、几何类型、绘制原语、两纸张常量。

## Tradeoffs accepted

1. **泛型语法噪音**：调用方要写 `Length::<NonNegative>::from_mm(20.)?` 或依赖 type alias `NonNegLen`；换更少 boilerplate 的 API _surface_。
2. **私有字段 + getter**：比公开字段多一层访问；换不变量不可能被绕过。
3. **Slice 1 不做长度算术**：layout 尚未落地，公开 `Add` 会逼早定 overflow/非有限规则；先在 crate 内用 raw pt 相加，下一 PR 再开 `pub(crate) ops`。
4. **`try_new` on Margins 暂为恒 Ok**：四边已是 `NonNegLen` 时无单字段失败；保留签名以便日后加「预置」或关系校验时不改调用形状。
5. **单一错误变体**：诊断信息较粗；实现期可在 `InvalidLength` 加 `&'static str context` 而不增顶层 variant 数量。

## Alternatives considered

| 方案 | 为何未选 |
| --- | --- |
| 设计稿式 public `f32` 字段 + `Option` 工厂 | 不变量在类型外，违背 §2.0；热路径仍要防御性检查。 |
| 三个独立 newtype（`PtFinite`/`PtNonNeg`/`PtPos`） | 可行但重复；本候选用 **单泛型** 显式表达「同一物理量、不同约束域」。 |
| `u32` 存 pt | 提案明确禁止；亚点精度（A4 边长）会丢。 |
| 构造全返回 `Option` | 与 crate 级 `Result`/`Error` 分裂；组装期需要可打印原因。 |
| 本 slice 引入 `PageSize` 枚举 | grounding 禁止；A4/Letter 仅为 `Size` 常量。 |
| 默认边距 72 pt（设计稿） | 提案 §7 已定默认 **0**；示例边距由调用方显式传入。 |

## Open questions and risks

===== C2 RATIONALE Shape+Alts =====
## Shape

`Length<C>` stores one private `f32` plus `PhantomData<C>`. The public,
uninhabited brands are `Finite`, `NonNegative`, and `Positive`. The brands own
the fallible pt/mm factory methods; `Length<C>` only exposes `as_pt`.

The allowed proof lattice is explicit:

```text
Positive ──> NonNegative ──> Finite
    └──────────────────────> Finite
```

Only these widening `From` conversions exist. There is no narrowing cast and
no `Add`/`Sub` implementation that could silently produce infinity or violate
a sign constraint.

`Point`, `Size`, `Rect`, `Margins`, `Color`, `Paint`, and `Stroke` have private
fields and small semantic constructors/accessors. In particular:

- `Point` may contain negative but finite coordinates.
- `Size` is strictly positive on both axes.
- `Rect` therefore cannot be degenerate.
- `Stroke` is strictly positive; “no stroke” is represented by not drawing.
- `Margins::default()` is exactly zero on all four sides.
- `Color` is RGBA `u8`; every bit pattern is valid.
- `Paint` is solid-color only.

`Path` stores immutable boxed verbs. `PathBuilder` begins with `MoveTo` and is
the only general construction route. The verb vocabulary is exactly
`MoveTo`, `LineTo`, and `Close`.

`Error` has only three construction failures: non-finite, negative where
non-negative was required, and zero/non-positive where positive was required.
No future font, image, limits, backend, page, or layout variants are guessed.

`MM_TO_PT` is the sole mm conversion factor. `A4` is formed from
`210 * MM_TO_PT` by `297 * MM_TO_PT`; `LETTER` is exactly `612 pt` by `792 pt`.
The implementation test compares A4 against the f64 formula with absolute
error below `1e-3 pt`.

## Tradeoffs accepted

- The generic spelling `Length<Positive>` is more visible than three plain
  type names. In exchange, storage and conversion behavior have one source of
  truth, and APIs state their required proof directly.
- Ordinary arithmetic returns to raw pt and then revalidates. This is less
  convenient than `+` and `-`, but prevents infallible traits from manufacturing
  invalid branded values through overflow or sign changes.
- `Size` and `Rect` disallow zero extents. This is intentionally stricter than
  a generic mathematical rectangle and matches page/image/backend geometry.
  Absence of visible output is represented structurally, not by a zero-sized
  object.
- The path builder allows `Close` only as the final verb. Multiple open
  subpaths are possible through `move_to`; drawing several closed contours
  requires several `Path` values in this slice. That keeps invalid path states
  unrepresentable without adding a path-specific error family.
- Fields are private. Callers lose struct-literal convenience but cannot bypass
  construction checks, including during later API refactors.

## Alternatives considered

### Three numeric newtypes

`FiniteLength(f32)`, `NonNegativeLength(f32)`, and `PositiveLength(f32)` are
straightforward, but repeat nearly identical representations and factory
methods. Their relationship is conventional rather than structural. Rejected
in favor of one branded representation and explicit proof widening.

### One finite `Length` plus runtime checks in aggregates

This makes `Margins::new` and `Stroke::new` fallible and forces later APIs to
recheck values whenever the same `Length` changes role. It moves invariants
out of types and into branches. Rejected.

### Const-generic constraint levels

`Length<0>`, `Length<1>`, and `Length<2>` would reduce marker declarations but
permit meaningless public spellings such as `Length<37>`, or require awkward
public trait machinery to seal valid values. Named brands communicate intent

===== C4 RATIONALE Shape+Alts =====

