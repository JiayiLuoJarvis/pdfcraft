# 刀 11 候选 1：逃逸层路径原语，「烘焙几何 + 旋转作用域」

Tip：`feat/layer-path` @ `9eda8da`。依据：`docs/提案书.md` §2.12，`.audit/knife-11/how.md`。

一句话：**几何全部在 `Layer` 方法里一次算成最终页坐标的 `Path`**（椭圆是真 cubic，旋转直接烘焙进顶点），布局层只多一个 `FragmentCommand::Fill`，`Canvas` / `CanvasCommand` / `Mark` 一行不动。旋转用一个**只收路径的作用域类型**表达，因此"旋转文字"在类型上就写不出来，而不是运行时报错。

## Problem

§2.12 要求 behind/front 闭包里有 `line` / `rect` / `ellipse`，可描边、可填充，支持旋转和半透明。今天 `Layer` 只有 `text_at` / `image_at`。

下层情况不对称（how.md）：

- `Canvas::fill_path` / `stroke_path` 和 alpha 到 opacity 已经通到 krilla。半透明不需要新管线。
- `FragmentCommand` 没有 `Fill`。`PathVerb` 没有曲线。`Canvas`、`CanvasCommand`、`Mark` 都没有变换。
- 层闭包在 `measure_bands` 里跑，不在 `paint_pages` 里跑。所有几何错误必须在那时暴露，才能保住"失败则画布为空"。
- 单个 `MoveTo` 能进 `RecordingCanvas`，到 `to_krilla_path` 才失败。记录测试是绿的，PDF 却失败。层路径必须在入口拒绝退化几何。
- `layout/` 不许 `use krilla`。变换真要落 krilla，只能落在 `backend/emit.rs`，而且 `push_transform` / `pop` 之间不能有可失败调用，否则 krilla 会 panic。

真正的分叉在旋转：烘焙对路径是精确的（纯旋转保距，描边宽度不变），但救不了文字和图片；`Canvas` 变换能救，代价是 trait、记录、回放、Mark、emit 六处一起长，还有 panic 风险。§2.12 写了"支持旋转"，没写范围。本候选的判断是：**本刀只旋转路径**，并把公共形状设计成以后换成画布变换时调用方一个字不用改。

## Usage (caller's view)

调用方导入：

```rust
use quire::{document, Color, Ink, PageSize, Pen, Point, Positive, Rect};
```

### 1. 文档糖：每页两条半透明对角线和一个页框（behind）

```rust
doc.behind(|page| {
    let info = page.page();
    let (w, h) = (info.size.width().as_pt(), info.size.height().as_pt());
    let faint = Pen::new(Color::rgba(200, 0, 0, 64), Positive::pt(0.8).unwrap());
    page.line(Point::ZERO, Point::try_pt(w, h).unwrap(), faint);
    page.line(Point::try_pt(w, 0.0).unwrap(), Point::try_pt(0.0, h).unwrap(), faint);

    let m = info.margins;
    let frame = Rect::try_pt(
        m.left().as_pt(), m.top().as_pt(),
        w - m.left().as_pt() - m.right().as_pt(),
        h - m.top().as_pt() - m.bottom().as_pt(),
    ).unwrap();
    page.rect(frame, Ink::Stroke(Pen::new(Color::rgb(0, 0, 0), Positive::pt(0.5).unwrap())));
});
```

### 2. section 主挂：末页电子章（front），填充 + 描边 + 旋转

```rust
doc.section(|s| {
    s.front(|page| {
        if !page.is_last() {
            return;
        }
        let seal = Rect::try_pt(420.0, 660.0, 120.0, 120.0).unwrap();
        let red = Color::rgb(200, 0, 0);
        page.ellipse(
            seal,
            Ink::FillStroke(Color::rgba(200, 0, 0, 40), Pen::new(red, Positive::pt(2.0).unwrap())),
        );
        // 章内斜框：绕章心顺时针转 -15°（页面上看是逆时针）。
        page.rotated(-15.0, Point::try_pt(480.0, 720.0).unwrap(), |r| {
            r.rect(
                Rect::try_pt(440.0, 710.0, 80.0, 20.0).unwrap(),
                Ink::Stroke(Pen::new(red, Positive::pt(1.0).unwrap())),
            );
        });
        page.text_at("SEAL", Point::try_pt(462.0, 712.0).unwrap());
    });
    // ...正文...
});
```

### 3. 只在奇数页画装订线（per-page 决策）

```rust
s.behind(|page| {
    let info = page.page();
    if info.number % 2 == 1 {
        let x = 18.0;
        let h = info.size.height().as_pt();
        page.line(
            Point::try_pt(x, 36.0).unwrap(),
            Point::try_pt(x, h - 36.0).unwrap(),
            Pen::new(Color::rgba(0, 0, 0, 128), Positive::pt(0.5).unwrap()),
        );
    }
});
```

### 4. 测试里调用方能断言什么（RecordingCanvas）

```rust
let page = &rendered.pages()[0];
// behind 的两条对角线在 header 文字之前，front 的椭圆填充、描边在正文之后。
assert!(matches!(page.ops()[1], CanvasCommand::StrokePath { .. }));
// 椭圆 = MoveTo + 4×CubicTo + Close，先 FillPath 后 StrokePath，路径同一条。
```

写不出来的东西（刻意）：

- `page.line(a, b, Ink::Fill(..))`：`line` 只收 `Pen`，直线没有填充。
- `page.rotated(30.0, c, |r| r.text_at(..))`：`Shapes` 没有 `text_at` / `image_at`。
- 既不描边也不填充的形状：`Ink` 三个臂都至少有一种墨。
- 零面积矩形或椭圆：`Rect` 的宽高是 `Positive`。

## Shape

### 模块改动地图

| 文件 | 改动 |
| --- | --- |
| `src/geom.rs` | `PathVerb::CubicTo`；公共 `Pen`、`Ink`；`pub(crate)` 的 `Path::rect` / `Path::ellipse` / `Path::rotated`；`Path::translated` 认 `CubicTo` |
| `src/lib.rs` | 重导出 `Ink`、`Pen`、`Shapes` |
| `src/document.rs` | `Layer::{line, rect, ellipse, rotated}`；新公共类型 `Shapes<'l>`；私有 `Layer::push_shape` |
| `src/layout/decorate.rs` | `LayerItem::Shape { path, ink }`；`measure_layer` 多一臂 |
| `src/layout/flow.rs` | `FragmentCommand::Fill { path, paint }`；`with_offset`、`draw_fragment` 各一臂 |
| `src/layout/list.rs` | `first_baseline` 的穷尽 match 多一臂 |
| `src/layout/test_util.rs` | `strokes` 认 `CubicTo`；新 `fills` |
| `src/backend/convert.rs` | `to_krilla_path` 多 `CubicTo => builder.cubic_to(..)` |
| `canvas.rs`、`krilla_canvas.rs`、`emit.rs` | **不动** |

调用链只有三跳：`Layer` 方法（`document.rs`，几何在 `geom.rs`）→ `measure_layer`（`decorate.rs`）→ `draw_fragment`（`flow.rs`）到已有的 `Canvas::fill_path` / `stroke_path`。

### geom（公共）

```rust
/// One command in a [`Path`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathVerb {
    MoveTo(Point),
    LineTo(Point),
    /// Cubic Bézier from the current point through two control points to `to`.
    CubicTo { c1: Point, c2: Point, to: Point },
    Close,
}

/// A colored stroke. Width is strictly positive, so a pen always leaves a mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Pen {
    color: Color,
    stroke: Stroke,
}

impl Pen {
    pub const fn new(color: Color, width: Length<Positive>) -> Self { todo!() }
    pub const fn color(self) -> Color { todo!() }
    pub const fn width(self) -> Length<Positive> { todo!() }
}

/// How a closed shape is painted. Every arm paints something.
///
/// Opacity comes from the colors' alpha. With `FillStroke` the fill is painted first,
/// then the stroke on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    Fill(Color),
    Stroke(Pen),
    FillStroke(Color, Pen),
}
```

### geom（crate 内）

```rust
impl Path {
    /// Closed rectangle: MoveTo(top-left), three LineTo clockwise, Close.
    pub(crate) fn rect(rect: Rect) -> Self { todo!() }

    /// Ellipse inscribed in `rect`: MoveTo(right-middle), four CubicTo quarter arcs
    /// clockwise on the page (y down), Close. Control offset = KAPPA * radius.
    pub(crate) fn ellipse(rect: Rect) -> Self { todo!() }

    /// Every point (including control points) rotated by `degrees` about `about`.
    /// Positive is clockwise on the page, matching krilla `Transform::from_rotate`.
    /// Fails only if a rotated coordinate is not finite.
    pub(crate) fn rotated(&self, degrees: f32, about: Point) -> Result<Self> { todo!() }
}

/// 4/3 · (√2 − 1). Radial error of the 4-arc cubic ellipse is about 0.027 %.
const KAPPA: f32 = 0.552_284_8;
```

`Path::rect` 取代 `table.rs` 私有的 `rect_path` 的念头**不在本刀做**（最小 diff）；两者顶点顺序相同，以后合并是纯重构。

### document（公共）

```rust
impl Layer {
    /// Straight segment in page coordinates. `from == to` fails the document with
    /// `Error::InvalidGeometry` before any page is drawn.
    pub fn line(&mut self, from: Point, to: Point, pen: Pen) { todo!() }

    /// Rectangle in page coordinates. Not inset by margins, not clipped.
    pub fn rect(&mut self, rect: Rect, ink: Ink) { todo!() }

    /// Ellipse inscribed in `rect`, in page coordinates.
    pub fn ellipse(&mut self, rect: Rect, ink: Ink) { todo!() }

    /// Draws the shapes added in `draw` rotated by `degrees` about `about`.
    /// Positive is clockwise on the page. A non-finite angle fails the document
    /// with `Error::InvalidGeometry`. Shapes keep their order relative to the
    /// layer's other items.
    pub fn rotated(&mut self, degrees: f32, about: Point, draw: impl FnOnce(&mut Shapes<'_>)) {
        todo!()
    }
}

/// Path-only view of a [`Layer`] inside [`Layer::rotated`].
pub struct Shapes<'l> {
    layer: &'l mut Layer,
    degrees: f32,
    about: Point,
}

impl Shapes<'_> {
    pub fn line(&mut self, from: Point, to: Point, pen: Pen) { todo!() }
    pub fn rect(&mut self, rect: Rect, ink: Ink) { todo!() }
    pub fn ellipse(&mut self, rect: Rect, ink: Ink) { todo!() }
}
```

两组同名方法的实现都只有一行，汇到一个私有入口：

```rust
impl Layer {
    /// Single place that turns a shape into a `LayerItem`. `turn` is `None` outside `rotated`.
    fn push_shape(&mut self, path: Result<Path>, ink: Ink, turn: Option<(f32, Point)>) {
        // TODO:
        // let path = path.and_then(|p| match turn { Some((d, c)) => p.rotated(d, c), None => Ok(p) });
        // match path { Ok(path) => self.items.push(LayerItem::Shape { path, ink }),
        //              Err(e)   => self.error.get_or_insert(e) }   // first error wins, as image_at
    }
}
```

`line` 传 `if from == to { Err(InvalidGeometry) } else { Ok(Path::line(from, to)) }`；`rect` / `ellipse` 传 `Ok(Path::rect(r))` / `Ok(Path::ellipse(r))`。`rotated` 先查 `degrees.is_finite()`，不合法则写 `self.error` 并**不跑** `draw`。

### layout（crate 内）

```rust
pub(crate) enum LayerItem {
    Text(LayerText),
    Image { image: DecodedImage, rect: Rect },
    /// Final page-coordinate geometry: ellipse arcs and rotation are already baked in.
    Shape { path: Path, ink: Ink },
}

pub(crate) enum FragmentCommand {
    Glyphs(GlyphRun),
    Fill { path: Path, paint: Paint },
    Stroke { path: Path, paint: Paint, stroke: Stroke },
    Image { image: DecodedImage, rect: Rect },
}
```

`measure_layer` 的新臂（纯映射，不做几何）：

```rust
LayerItem::Shape { path, ink } => {
    // TODO:
    // Ink::Fill(c)          => push Fill { path, paint: Paint::solid(c) }
    // Ink::Stroke(p)        => push Stroke { path, paint: Paint::solid(p.color()), stroke: Stroke::new(p.width()) }
    // Ink::FillStroke(c, p) => push Fill { path.clone(), .. } then Stroke { path, .. }
}
```

`draw_fragment` 的新臂：`FragmentCommand::Fill { path, paint } => canvas.fill_path(&path.translated(ox, oy)?, paint)`，和 `Stroke` 对称。

### 不变量放在哪

| 不变量 | 落点 |
| --- | --- |
| 形状至少有一种墨 | 类型：`Ink` 三臂 |
| 直线不能填充 | 类型：`line` 只收 `Pen` |
| 描边宽度 > 0 | 类型：`Pen` 内 `Length<Positive>` |
| 矩形 / 椭圆非退化 | 类型：`Rect` 的 `Positive` 宽高 |
| 旋转作用域内不能放文字 / 图片 | 类型：`Shapes` 没有这些方法 |
| 直线两端不重合、角度有限 | 运行时，`Layer` 方法入口写 `Layer.error`，在 `measure_layer` 里变成 `Err`，早于任何 `start_page` |
| 层坐标不被平移 | 已有：层片段 origin `(0,0)` |
| 叠层顺序 | 已有：`paint_pages` 不改 |
| 半透明 | 已有：`Color` alpha 到 `to_fill` / `to_stroke` |

### 接口深度

公共面新增：`Layer` 4 个方法、`Shapes` 3 个方法、`Pen`、`Ink`、`PathVerb::CubicTo`。隐藏在后面的：椭圆的 cubic 构造、旋转矩阵和旋转中心换算、fill/stroke 先后次序、退化校验、fill 进 paint 管线。调用方不需要知道 `Path`、`Paint`、`Stroke` 的存在（`Pen` 吃 `Color` 和宽度）。**不提供** `Layer::path(Path, Ink)`：§2.12 只点名三种形状，`PathBuilder` 本身不防单 `MoveTo`，开放它就要在层入口补一层"路径合法性"策略（per boundary-discipline，等有调用方再开）。

## Synthesis decision

TBD

## Tradeoffs accepted

- 我们接受**本刀不能旋转文字和图片**（经典斜 `DRAFT` 文字水印要等下一刀），换来 `Canvas` trait、`CanvasCommand`、`replay`、`Mark`、`emit` 全部不动，也没有 `push_transform` / `pop` 不平衡导致 krilla panic 的风险。斜线、斜框这类路径水印今天就能画。
- 我们接受 `Layer` 和 `Shapes` 上各有一组同名的 `line/rect/ellipse`，换来"旋转作用域里只有路径"由类型保证。两组都是一行转发到同一个 `push_shape`，规则只有一份。
- 我们接受 `PathVerb` 加公共变体（对穷尽 match 它的外部代码是破坏性变更），换来 `Path::verbs()` 如实反映几何。版本是 `0.x`，`publish = false`。
- 我们接受 `FillStroke` 生成两条命令、路径 clone 一次，换来 `Canvas` 维持 fill / stroke 两个原子操作，不新增"带填充又带描边"的第三种画布指令。
- 我们接受旋转角度是裸 `f32`（运行时校验），而不是新的品牌类型 `Angle`。层闭包返回 `()` 不能 `?`，`Layer.error` 已经是这种"方法不返回 `Result` 但开画前失败"的先例。
- 我们接受 RecordingCanvas 里看到的是**旋转后的顶点**而不是"矩形 + 变换"。断言要算三角函数，用 `pt_near` 比较，不比 bit。

## Alternatives considered

- **椭圆折成折线（N 段 `LineTo`）。** 不用改 `PathVerb`，也不用改 `to_krilla_path`。输掉的原因：段数是一个调用方看不见、却决定输出质量的策略旋钮（放大打印就看到棱角）；每个椭圆几十个点，RecordingCanvas 断言变成噪声；PDF 体积白涨。cubic 在旋转（仿射）下精确，和烘焙方案天然配合。
- **`Canvas::push_transform` / `pop`（或每条命令带 `Transform` 字段）。** 能旋转文字和图片，覆盖完整的 §2.12。输掉的原因：要在 `Canvas` trait、`CanvasCommand`、`RecordingCanvas::pages` 的穷尽 match、`replay`、`KrillaCanvas`、`Mark`、`emit::draw_mark` 七处同时加东西，而 `emit.rs` 明说成对调用之间不能有可失败调用。为了一个本刀没有调用方的能力（旋转文字），付出最大的 diff 和唯一的 panic 风险。`Shapes` 作用域保留了以后切过去的余地。
- **每个形状带可选旋转参数：`page.rect(r, ink, Some(Rotation::deg(30.0, c)))`，或 `Shape` 值类型 + `page.draw(shape, ink)`。** 不需要 `Shapes` 类型。输掉的原因：九成调用不旋转却都要写 `None`；`draw(Shape, Ink)` 把 §2.12 点名的三个原语藏到一个枚举后面，调用方先学 `Shape` 再学 `Ink`；而且一组形状共用一个旋转中心（章里的框和字）时要在每个调用上重复角度和中心。
- **一个 `Path` 值 + `stroke_path` / `fill_path` 成对方法（贴近 Canvas）。** 输掉的原因：把 `Path` 构造和"单 `MoveTo` 合法性"推给调用方，还要两次调用才能"填充加描边"，属于 design-red-flags 的"调用方协调多个方法完成一件事"。

## Open questions and risks

- §2.12 的"支持旋转"是否接受**本刀只覆盖路径**？如果斜 `DRAFT` 文字是第一版必须的，`Shapes` 作用域是否就是加 `text_at` 的地方（实现切换为画布变换，调用方不变）？
- `rotated` 是否需要允许嵌套（`Shapes::rotated`，角度和中心复合）？本候选不提供，因为没有调用方；加上只是矩阵乘法。
- 角度正方向取"页面上顺时针"（跟 krilla / y 向下一致）。调用方画左下到右上的斜线要写负角度。这比"数学上逆时针为正"更不直观，是否要反过来、在烘焙里取反？
- `PathVerb` 要不要顺手加 `#[non_exhaustive]`，免得以后再加 `QuadTo` 之类又破坏一次？代价是外部测试的 match 要多写 `_`。
- `PathBuilder::cubic_to` 是否公开？本候选保持 `pub(crate)`：公共 `PathVerb` 能读到 `CubicTo`，但调用方只能经 `ellipse` 产生它。读写不对称是否可以接受？
- `Color::rgba(.., 0)` 照样画一条看不见的路径。要不要在 `push_shape` 里把全透明静默丢掉？本候选**不丢**（不做静默策略），和今天表格边框行为一致。
- 风险：`Path::rotated` 用 `f32` 算 sin/cos，90° 的结果不是精确的 0（约 `-4.4e-8`）。测试必须用 `pt_near`，不能复用 `bits` 风格的精确比较。

## Next implementation step

先在 `geom.rs` 加 `PathVerb::CubicTo`、`Path::ellipse` 和 `Path::rotated`，并让 `translated`、`to_krilla_path`、`test_util::strokes` 认识 `CubicTo`，用单元测试钉住椭圆的 4 段控制点和 90° 旋转后的顶点，再接 `FragmentCommand::Fill` 和 `Layer` 方法。
