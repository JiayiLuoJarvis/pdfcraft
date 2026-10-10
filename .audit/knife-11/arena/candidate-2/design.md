# Knife 11 candidate 2 — analytic figures on Layer

Synthesis decision: TBD.

## Problem

§2.12 要在 `behind` / `front` 的 `Layer` 上增加 `line` / `rect` / `ellipse`、描边与填充、旋转、以及颜色半透明。钩子、section 主挂、`doc` 糖、叠层 `behind → header → 正文 → footer → front`、以及「测量阶段跑钩子、失败则画布为空」都已经定死，本刀只补原语。

缝上的缺口让形状并不显然。`Canvas::fill_path` / `stroke_path` 和 `Color` 的 alpha（`backend/convert.rs` 里 `a/255`）已经通到 krilla；`FragmentCommand` 只有 `Stroke`，没有 `Fill`。`PathVerb` 只有 `MoveTo` / `LineTo` / `Close`。`Rect` 的宽高是 `Length<Positive>`，零面积矩形造不出来。`Layer` 方法返回 `()`，资源错误写入 `Layer.error`，由 `layer_content` 在开画前变成 `Err`。层片段原点是 `(0, 0)`，命令里的坐标就是页坐标。

必须一次定死四件互锁的事：椭圆以什么形式存在、旋转作用在哪一层数据上、填充如何进入已有的 fill/stroke 绘制、`Layer` 对调用方露出几个入口。

## Usage (caller's view)

调用方仍然只注册 `behind` / `front`。路径不进正文流，不占高度，不换行。坐标相对当前页左上，y 向下，不自动缩进 margin。半透明就是 `Color::rgba` 的 alpha。旋转是某个图形自己的刚体转动：顺时针为正，单位是度，y 向下。文字和图片沿用 `text_at` / `image_at`，不跟着转。

```rust
use quire::{
    document, Angle, Color, Figure, Ink, PageSize, Point, Positive, Rect, Size, Stroke,
};

let pdf = document(PageSize::A4, |doc| {
    doc.paragraph(|p| p.text("正文"));
    doc.behind(|layer| {
        let page = layer.page().size;
        let from = Point::try_pt(36.0, 36.0).unwrap();
        let to = Point::try_pt(page.width().as_pt() - 36.0, page.height().as_pt() - 36.0).unwrap();
        layer.draw(
            Figure::line(from, to),
            Ink::stroke(Stroke::new(Positive::pt(0.6).unwrap()), Color::rgba(0, 0, 0, 48)),
        );
        let bar = Rect::new(
            Point::try_pt(120.0, 400.0).unwrap(),
            Size::try_pt(360.0, 36.0).unwrap(),
        );
        let center = Point::try_pt(300.0, 418.0).unwrap();
        layer.draw(
            Figure::rect(bar).turned(Angle::degrees(32.0).unwrap(), center),
            Ink::fill(Color::rgba(160, 160, 160, 40)),
        );
    })?;
    doc.front(|layer| {
        if layer.is_last() {
            let c = Point::try_pt(430.0, 700.0).unwrap();
            layer.draw(
                Figure::ellipse(c, Positive::pt(42.0).unwrap(), Positive::pt(42.0).unwrap()),
                Ink::fill_then_stroke(
                    Color::rgba(180, 0, 0, 30),
                    Stroke::new(Positive::pt(1.25).unwrap()),
                    Color::rgba(180, 0, 0, 200),
                ),
            );
        }
    })?;
    Ok(())
})?;
```

第二处：多 section 时钩子挂在 section 上，`doc.behind` 只是隐式 section 的糖。后一次注册覆盖前一次。

```rust
doc.section(|sec| {
    sec.behind(|layer| {
        if layer.page().section_number == 1 {
            let frame = Rect::new(Point::ZERO, layer.page().size);
            layer.draw(Figure::rect(frame), Ink::stroke(hairline, Color::rgb(0, 0, 0)));
        }
    });
    sec.paragraph(|p| p.text("本篇"));
});
```

第三处：同一图形先填后描，转动写在图形上，再 `draw` 一次。第二次 `turned` 替换上一次的角度和枢轴，不叠加。

```rust
let stamp = Figure::ellipse(center, rx, ry)
    .turned(Angle::degrees(15.0).unwrap(), center)
    .turned(Angle::ZERO, center);
layer.draw(stamp, Ink::stroke(ring, Color::rgba(0, 0, 0, 255)));
```

`Angle::ZERO` 表示不旋转。枢轴在角度为 0 时不参与计算。

## Shape

数据先于调用。调用方拿得到的类型只有四个，外加已有的 `Color` / `Stroke` / `Rect` / `Point`。

```rust
/// Clockwise degrees in y-down page space. Finite, not reduced modulo 360.
pub struct Angle { /* degrees: f32 */ }

impl Angle {
    pub const ZERO: Self;
    pub fn degrees(degrees: f32) -> Result<Self>; // NonFiniteAngle { value }
    pub const fn as_degrees(self) -> f32;
}

/// One layer mark. Opaque. The only geometry `Layer::draw` accepts.
pub struct Figure { /* body: Body, turn: Turn */ }

impl Figure {
    pub fn line(from: Point, to: Point) -> Self;
    pub fn rect(rect: Rect) -> Self;
    pub fn ellipse(center: Point, rx: Length<Positive>, ry: Length<Positive>) -> Self;
    /// Replaces any turn already stored. Same arguments are idempotent.
    pub fn turned(self, clockwise: Angle, around: Point) -> Self;
}

pub struct Ink { /* private */ }

impl Ink {
    pub fn stroke(stroke: Stroke, color: Color) -> Self;
    pub fn fill(color: Color) -> Self;
    /// Lowers to a fill command followed by a stroke command.
    pub fn fill_then_stroke(fill: Color, stroke: Stroke, stroke_color: Color) -> Self;
}

impl Layer {
    pub fn draw(&mut self, figure: Figure, ink: Ink);
}
```

`Figure` / `Ink` / `Angle` 放在 `geom`，由 `lib.rs` 再导出。`Ellipse` 也是公开的，但是绘制结果，不是 `draw` 的入参：

```rust
/// Axis-aligned radii, then a clockwise turn about `center`.
pub struct Ellipse { /* center, rx, ry, turn: Angle */ }
// accessors: center, rx, ry, turn
```

内部表示（外部匹配不到）：

```rust
enum Body {
    Line { from: Point, to: Point },
    Rect(Rect),
    Ellipse { center: Point, rx: Length<Positive>, ry: Length<Positive> },
}
struct Turn { clockwise: Angle, around: Point } // default: ZERO about Point::ZERO

pub(crate) enum Outline {
    Path(Path),
    Ellipse(Ellipse),
}

pub(crate) enum FragmentCommand {
    Glyphs(GlyphRun),
    Fill { outline: Outline, paint: Paint },
    Stroke { outline: Outline, paint: Paint, stroke: Stroke },
    Image { image: DecodedImage, rect: Rect },
}

pub(crate) enum LayerItem {
    Text(LayerText),
    Image { image: DecodedImage, rect: Rect },
    Mark { figure: Figure, ink: Ink },
}
```

`FragmentCommand::Stroke` 的 `path` 字段改为 `outline`。表格边框变成 `Outline::Path`。填充和描边共用同一种轮廓，椭圆不会再长出一对 `FillEllipse` / `StrokeEllipse` 命令。

### 谁拥有哪条规则

`lower_figure` 独占在新模块 `src/layout/figure.rs`（`pub(crate) mod`）。它是旋转的唯一实现。`Layer::draw` 只把 `Mark` 推进 `items`，不在钩子里做几何。`measure_layer` 对每个 `Mark` 调用 `lower_figure`，失败则整页测量失败，`paint_pages` 不会 `start_page`。

```text
lower_figure(figure, ink) -> Result<Vec<FragmentCommand>>
  angle == 0: 点原样返回，保持位模式
  angle != 0: 顺时针、y 向下
      x' = x cos θ - y sin θ
      y' = x sin θ + y cos θ
      相对枢轴 around
  Line:  两端重合 → Err(InvalidGeometry)；否则 Path::line
  Rect:  四角 tl, tr, br, bl 旋转后闭合成 Path（与表格同向）
  Ellipse:
      center' = rotate(center)
      Ellipse { center: center', rx, ry, turn: angle }
  Ink::Stroke          → 一条 Stroke
  Ink::Fill            → 一条 Fill
  Ink::FillThenStroke  → Fill，然后 Stroke（描边盖住填色）
  颜色进 Paint::solid；alpha 不再包装
```

椭圆不进入 `Path`。`PathVerb` 保持 `MoveTo` / `LineTo` / `Close`。四段三次贝塞尔只在 `backend/convert.rs` 的 `ellipse_to_krilla` 里生成，直接调用 krilla `PathBuilder` 的 cubic，不经过 crate 的 `Path`。`to_krilla_path` 不动。

```text
κ = 4/3 * tan(π/8)
u = rotate_clockwise_ydown((rx, 0), θ)
v = rotate_clockwise_ydown((0, ry), θ)
从 C+u 起，每象限一段 cubic，控制点为端点 ± κ·共轭半径，最后 Close
绕向与矩形 tl→tr→br→bl 一致，配合现有 FillRule::NonZero
```

画布只为这种解析椭圆加两个方法，变换栈不加：

```rust
fn fill_ellipse(&mut self, ellipse: &Ellipse, paint: &Paint) -> Result<()>;
fn stroke_ellipse(&mut self, ellipse: &Ellipse, paint: &Paint, stroke: &Stroke) -> Result<()>;
```

`CanvasCommand` 增加 `FillEllipse` / `StrokeEllipse`。`RecordingCanvas::pages`、`replay`、`KrillaCanvas` 三处穷尽匹配一起改。krilla 侧用现成的 `to_fill` / `to_stroke`，然后 `draw_path`。不调用 `Transform::from_rotate`，也不调用 `push_opacity`。

`draw_fragment` 按 `Outline` 分发：`Path` 走现有 `fill_path` / `stroke_path`，`Ellipse` 走新方法。平移仍用 `try_pt`。层原点是 `(0, 0)`，测量阶段已经得到有限坐标，这里是恒等平移。`Fragment::with_offset` 对椭圆只平移 `center`，半径和 `turn` 不动。`list::first_baseline` 对 `Fill` 以及两种 `Outline` 返回 `None`。

矩形四角闭合流从 `table::rect_path` 收成 `geom` 里一个 `pub(crate)` 的四点闭合函数，表格和 `lower_figure` 都调用它。

### 不变量落在哪

| 不变量 | 编码 |
| --- | --- |
| 线宽 > 0、椭圆半径 > 0、矩形面积 > 0 | 已有 `Stroke` / `Length<Positive>` / `Rect` |
| 角度有限 | `Angle::degrees` 拒绝 NaN/∞，新错误 `Error::NonFiniteAngle { value }` |
| 线段两端不重合 | `lower_figure` 返回 `InvalidGeometry`（关系约束，类型装不下） |
| 旋转后坐标仍有限 | `Point::try_pt` 的 `NonFiniteLength`，从 `lower_figure` 原样返回 |
| 360° 不折成 0° | `Angle` 保存调用方度数；三角运算自然周期 |
| 填充在描边之下 | `fill_then_stroke` 的命令顺序，调用方排不了相反顺序 |
| 同层绘制顺序 | `items` 的 push 顺序；一个 `Mark` 内部先填后描 |
| alpha | `Color` 通道，后端已有 opacity |

钩子类型、`Section` 槽、`doc.behind` / `doc.front` 的覆盖语义和 `fail_outside` 不改。空 `items` 仍得到 `Ok(None)`，钩子已经跑过。

### 接口深度

公开入口是 `Figure` 的三个构造器、一次 `turned`、`Ink` 的三种油墨、`Layer::draw`。调用方不接触贝塞尔、κ、枢轴轨道、`Outline`、`FragmentCommand`。

复杂度留在 `lower_figure`（刚体转动、退化线段）和 `ellipse_to_krilla`（四段 cubic）。画布增长的是椭圆这一种记录，因为直线和矩形旋转之后仍是 `Path`，测试用现有 `StrokePath` / `FillPath` 断言顶点；椭圆测试断言 `center` / `rx` / `ry` / `turn`，不断言 κ。κ 的数值测试只放在 `convert` 的单元测试里。

`layout/` 不 `use krilla`。`text_at` 的颜色、流内分隔线、流内 canvas、水印 API、cap/join/dash、页面裁剪都不在本刀。

## Tradeoffs accepted

- 接受文字和图片不能旋转，以换取画布不引入变换栈，并避开 `emit` 里 `push_transform` / `pop` 必须成对、中间不能失败的约束。
- 接受 `RecordingCanvas` 里矩形和线段是折线、椭圆是 `Ellipse`，以换取椭圆测试不依赖 κ，且 `PathVerb` 不必增加曲线臂。
- 接受 `Figure::line` 能造出两端重合的值，到 `lower_figure` 才失败，以换取层钩子保持 `Fn(&mut Layer)`、调用处不必 `?`。失败仍在 `start_page` 之前，画布为空。
- 接受角度 0 时枢轴被忽略、录制结果与「从未 `turned`」相同，以换取 `turned` 替换语义：同一参数再调用一次不改变结果。
- 接受未归一化的角度（360 与 0 在 `Ellipse.turn` 上不相等，几何相同），以换取录制能看见调用方传入的度数。
- 接受 `a == 0` 仍发绘制命令，以换取与现有 fill/stroke opacity 管线一致，不在层上另判透明。
- 接受 `Canvas` 增加椭圆专用的 fill/stroke，以换取不把 cubic 放进公开 `Path`。这两个方法承担「解析椭圆 → krilla cubic」的适配，不是把同一参数原样转发。

## Alternatives considered

- **`PathVerb::CubicTo`，椭圆在 `geom` 里收成四段 cubic，旋转烘焙进控制点。** 公开路径枚举因此长出曲线，表格、`translated`、`to_krilla_path`、测试辅助都要认识 cubic。调用方能手写任意曲线，超出 §2.12 的三种图形。录制断言绑在 κ 上。复杂度露在 `Path` 上，而不是藏在一次 lower 里。
- **测量时把椭圆折成固定段数的 `LineTo`。** `Path` 和画布都不用改，但弦高误差成为排版结果的一部分，`RecordingCanvas` 无法按圆心和半径断言，段数会变成第二份手写清单。
- **`Canvas` 变换栈，`push_transform` 包住后续 item。** 文字水印能一起转，但变换状态跨 `text_at` / `image_at` / 路径，调用方必须自己配对；`CanvasCommand`、`replay`、`Mark`、`emit` 全部改道。层钩子里再叠加「当前矩阵」后，同一效果有两条路径（item 上的角度，以及栈上的角度）。
- **`stroke_line` / `fill_rect` / `stroke_ellipse` 六七个方法，或层上 `rotate` 改变后续状态。** 每加一种油墨或图形就乘一次方法数；层上的旋转是时间状态，和 `turned` 并存就会有两种旋转。浅，而且有两条路。
- **椭圆用外接 `Rect` 再旋转四个角。** 旋转后的外接矩形角点不是椭圆。轴对齐半径加绕中心的角度才能在旋转后仍是椭圆。

## Open questions and risks

- §2.12 的「支持旋转」是否包含 `text_at` / `image_at`？本设计只旋转 `Figure`。若斜向「草稿」文字也在本刀，变换栈要另案，本形状不够。
- 顺时针为正是否就是调用方语言？本设计按页面 y 向下的顺时针烘焙，不跟随 krilla `from_rotate` 的符号。
- 两端重合的线用现成的 `InvalidGeometry` 是否够？它不携带这两个点。
- `fill_then_stroke` 是否要允许填色与描边色不同？本设计允许。若产品只要同一 `Color`，可以收成一个颜色参数，命令顺序不变。
- 椭圆 cubic 与 krilla 对「仅 MoveTo」的拒绝无关；若 `ellipse_to_krilla` 的 `finish()` 返回空，应映射为 `InvalidGeometry`，且该失败发生在 `to_pdf`，录制测试仍然是绿的。实现时要在 `convert` 单测里钉住 `finish()` 成功，避免把几何错误留到开画之后。

## Next implementation step

在 `geom` 加入 `Angle`、`Figure`、`Ink`、`Ellipse`，并在 `layout/figure.rs` 实现 `lower_figure` 的纯函数测试（角度 0 的位模式、`turned` 替换、重合线段、矩形四角、椭圆中心绕枢轴公转且半径不变），然后再改 `FragmentCommand` 与画布。
