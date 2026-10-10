# Knife 11 cross-judge

只读评审。依据：`arena/frame.md` 的评分细则、两份 `design.md`、`docs/提案书.md` §2.12，并对照了 `src/geom.rs`、`src/backend/krilla_canvas.rs`、`src/backend/emit.rs`。

## 评分

| # | 评分项 | C1（几何烘焙进路径 + `Shapes` 旋转作用域） | C2（`Figure` + `Layer::draw` + 画布解析椭圆） |
| --- | --- | --- | --- |
| 1 | 提案书一致性 | 4 | 4 |
| 2 | 接缝契合 | 5 | 2 |
| 3 | 接口深度 | 4 | 3 |
| 4 | diff 成本 / 克制程度 | 5 | 2 |
| 5 | 可测试性 | 4 | 4 |
| 6 | 非法状态 | 4 | 3 |
| | **合计** | **26** | **18** |

### 评分理由

1. **提案书一致性。** 两份都覆盖了三种原语、描边和填充、颜色 alpha 半透明，都保留了 section 主挂、doc 糖和叠层顺序。两份都只旋转路径，不旋转 `text_at` / `image_at`，也都把这一点列为待确认问题。§2.12 写的是"支持旋转"，没限定范围，所以各扣 1 分。C1 的 `Layer::line/rect/ellipse` 和提案书的用词一一对应；C2 把三种原语收进 `Figure::*`，再交给 `Layer::draw`，可以接受，但离提案书的写法远一层。
2. **接缝契合。** C1 只加了 `FragmentCommand::Fill`，然后走已有的 `Canvas::fill_path` / `stroke_path`；`Canvas`、`CanvasCommand`、`Mark`、`emit` 都不动，路线和评分细则里"LayerItem → measure_layer → FragmentCommand → 已有 Canvas fill/stroke"完全一致。C2 给 `Canvas` trait 加了 `fill_ellipse` / `stroke_ellipse`，加了 `CanvasCommand` 变体，还把 `FragmentCommand::Stroke.path` 改名为 `outline`，连带改到表格代码。另外它**漏算了** `Mark` / `emit`：`KrillaCanvas` 现在把 crate 自己的 `Path` 存进 `Mark::Fill/Stroke`，到 `emit` 才转成 krilla 类型（`krilla_canvas.rs:57`、`:252–268`）。椭圆要么新增 `Mark` 变体和 `emit` 分支，要么在记录阶段就转换，打破现有的延迟转换模式。
3. **接口深度。** C2 的入口只有 `Layer::draw` 一个，但新增了 `Angle`、`Figure`、`Ink`、`Ellipse` 四个公共类型。其中 `Ellipse` 是公开的却不是任何调用的入参，只为测试暴露。`turned` 采用"替换"语义，用法 3 里的 `.turned(15°).turned(ZERO)` 会让读者困惑。C1 新增 4 个 `Layer` 方法、3 个 `Shapes` 方法和 `Pen`、`Ink`。它有一组重复的同名方法，但都一行转发到同一个 `push_shape`，换来的是"旋转作用域里不能放文字"由类型保证。
4. **diff 成本。** C1 改约 8 个文件，三个画布文件不碰，也不重构表格。C2 改动画布 trait 和三处穷尽 match，加上漏算的 `Mark`/`emit`，再把 `table::rect_path` 收进 `geom`（细则要求最小 diff，这次重构是顺手做的），并新开 `layout/figure.rs`。细则写的是"除非旋转烘焙更差，否则不要扩 Canvas trait"，而 C2 扩 trait 不是为了旋转，是为了不让 `PathVerb` 加曲线。
5. **可测试性。** C1 能在 `RecordingCanvas` 里断言椭圆的 MoveTo + 4×CubicTo + Close、先 Fill 后 Stroke 的顺序，以及旋转后的顶点。代价是要用 `pt_near` 比较，κ 值会出现在断言里。C2 对椭圆按 center/rx/ry/turn 断言，语义更清楚，κ 只在 `convert` 的单元测试里出现，这一点更好。但 C2 自己也承认，`ellipse_to_krilla` 的失败要到 `to_pdf` 才暴露，记录测试会保持绿色。两份打平。
6. **非法状态。** C1 在 `Layer` 方法入口就把首尾重合的线段和非有限角度写进 `Layer.error`；`Ink` 三个分支都至少画一种墨；`line` 只收 `Pen`，所以直线没法填充。扣 1 分是因为角度是裸 `f32`，只在运行时检查。C2 的 `Angle` 是品牌类型，这一点更好。但首尾重合的线段要到 `lower_figure` 才拒绝；`turned` 会静默覆盖前一次的角度；椭圆转换的失败时间点晚于 `start_page`，违反了"失败则画布为空"。

## 推荐底稿：C1

理由：C1 让 `Canvas` 抽象保持原样，半透明完全复用已有的 alpha 管线。所有几何错误都在测量阶段暴露，早于任何 `start_page`。diff 最小，也没有 `push_transform` / `pop` 带来的 panic 风险。将来如果要旋转文字，可以在 `Shapes` 作用域里加 `text_at`，改成画布变换实现，调用方代码不用改。C2 的核心选择（画布上的解析椭圆）恰好是细则明确要避免的那类改动，而且它的改动地图低估了范围。

## 从 C2 嫁接的内容

1. **测试辅助按语义断言椭圆。** 在 `layout/test_util` 里提供一个从 MoveTo + 4×CubicTo 的落点还原 center/rx/ry 的辅助函数，层测试只断言这四个落点和顺序；κ 控制点的数值只钉在 `geom` 的 `Path::ellipse` 单元测试里。
2. **可选：品牌化 `Angle`。** 可以考虑用 `Angle::degrees(f32) -> Result<Angle>` 替代 `rotated` 的裸 `f32`，和 §2.0 的品牌化风格一致。代价是层闭包里不能用 `?`，调用处要写 `.unwrap()`。C1 已经把这条列为有意接受的取舍。我倾向保留裸 `f32` 加运行时检查，交给 synthesis 决定。
3. **"非零角度才旋转"的约定。** 角度为 0 时直接跳过 `Path::rotated`，这样未旋转的路径保持精确的位模式，现有的精确比较测试不受影响。

## 风险提示

- **两份共有：** §2.12 的"支持旋转"是否包含文字和图片，提案书没有定。如果斜写的 `DRAFT` 文字水印是第一版必须的，两份都不够，需要先回提案书确认，再进入实现。
- **C1：** `PathVerb::CubicTo` 是公共枚举的破坏性变更。建议 synthesis 当场决定是否加 `#[non_exhaustive]`，不要留到下次再破坏一次。旋转用 `f32` 三角函数计算，90° 时结果不是精确的 0，相关测试必须用 `pt_near`。
- **C1：** 角度方向取"页面上顺时针为正"，画左下到右上的斜线要写负角度。这一点需要在 rustdoc 里写清楚，可以拿用法 2 当例子。
- **C2（若被采用）：** 改动地图漏了 `Mark` 和 `emit::draw_mark`；椭圆转换失败会发生在 `start_page` 之后。
