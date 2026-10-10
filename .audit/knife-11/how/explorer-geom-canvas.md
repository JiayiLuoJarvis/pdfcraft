# How explorer: geom Path / Color opacity / Canvas / backend-krilla

Source: tip `feat/layer-path` @ `9eda8da` (`9eda8da14d645b84f79e94ef83ceb510e4dab708`).

behind/front 今天只把文字和图片放进页面坐标层。路径、旋转、半透明原语还没有公共入口。填充和描边已经能从 `Canvas` 走到 krilla，透明度已经从 `Color` 的 alpha 接到 `Fill`/`Stroke` 的 opacity。旋转和曲线两边都还没有。

### Components Found

**`Path` / `PathVerb` / `PathBuilder`** — `src/geom.rs` 539–627 行

- `PathVerb` 只有三臂：`MoveTo(Point)`、`LineTo(Point)`、`Close`。没有 quad、cubic、arc、ellipse。
- `Path` 非空，`verbs: Box<[PathVerb]>`，文档要求第一条是 `MoveTo`。构造不在类型上强制后续动词合法。
- `Path::begin(at) -> PathBuilder`：唯一动词是 `MoveTo(at)`。
- `PathBuilder::{line_to, move_to}` 追加后返回 `self`。`finish` 不追加 `Close`。`close` 先 `Close` 再 `finish`。
- `Path::line(from, to)` = `begin(from).line_to(to).finish()`。两条动词，不闭合。
- 没有 `Path::rect`、`Path::ellipse`、`Path::rotated`。
- `Path::translated(dx, dy)` 是 `pub(crate)`。对 `MoveTo`/`LineTo` 的点做 `Point::try_pt(x+dx, y+dy)`，`Close` 原样。非有限结果变成 `Error`。只平移，不旋转、不缩放。
- 公共重导出：`src/lib.rs` 12–15 行导出 `Path`、`PathBuilder`、`PathVerb`。

**`Color` / `Paint` / `Stroke`** — `src/geom.rs` 473–537 行

- `Color { r, g, b, a: u8 }`。`rgb` 把 alpha 固定为 255。`rgba` 接受任意 `u8` alpha。`channels()` 返回四元组。
- `Paint` 只有 `color: Color`。`Paint::solid`。注释是 “A solid fill”。没有渐变、没有单独的 opacity 字段。
- `Stroke` 只有 `width: Length<Positive>`。零宽和负宽在 `Positive::pt` 就被拒（`NonPositiveLength`）。没有 cap、join、miter、dash。

**`Canvas`** — `src/canvas.rs`

- `Canvas::fill_path(&mut self, path: &Path, paint: &Paint)`、`stroke_path(..., stroke: &Stroke)`（194–197 行）。
- trait 上没有 `push_transform`、`pop`、`rotate`、`save`、opacity group、clip。
- `CanvasCommand::{FillPath { path, paint }, StrokePath { path, paint, stroke }}`（145–160 行）。`#[non_exhaustive]`。没有 Transform 变体。
- `RecordingCanvas` 的 `fill_path` / `stroke_path`（322–336 行）克隆 path、拷贝 paint/stroke，无条件 `Ok(())`。不检查页是否打开，不检查路径是否能被 krilla `finish`。
- `pages()`（261–296 行）把 `FillPath` 和 `StrokePath` 留在 `RecordedPage::ops` 里。
- `replay`（361–380 行）原样再调用 `fill_path` / `stroke_path`。

**`KrillaCanvas` marks** — `src/backend/krilla_canvas.rs`

- `Mark::Fill { path, paint }`、`Mark::Stroke { path, paint, stroke }`（57–73 行）。原样克隆，不在这里算 opacity，也不挂 transform。
- `fill_path` / `stroke_path`（252–268 行）要求已开页，然后 `push` mark。失败走 `fail` 毒化。
- 本文件不 `use krilla`。

**opacity 接线** — `src/backend/convert.rs` 52–73 行，`src/backend/emit.rs` 115–130 行

- `to_fill`：`opacity = NormalizedF32::new(f32::from(a) / 255.0)`，失败则 `Error::InvalidGeometry`。`paint` 只用 rgb。`rule: FillRule::NonZero`。
- `to_stroke`：同一套 alpha→opacity。`width: stroke.width().as_pt()`。其余 `..KStroke::default()`（cap/join/miter/dash）。
- `draw_mark`：`Fill` 先 `set_fill(Some(to_fill))`、`set_stroke(None)`，再 `draw_path`。`Stroke` 先 `set_fill(None)`、`set_stroke(Some(to_stroke))`，再 `draw_path`。
- 字形走 `to_fill(Paint::solid(*color))`（161 行），所以字形 alpha 已经是同一条 opacity 管线。
- `to_krilla_path`（36–49 行）只 `move_to` / `line_to` / `close`。`builder.finish()` 为 `None` 时 `InvalidGeometry`。测试：单独 `MoveTo` 被拒（107–109 行）。
- 点/尺寸/矩形映射是恒等。文件头写明不预翻转。

**唯一的 `push_transform`** — `src/backend/emit.rs` 172–182 行

- 只包住 `Mark::Image`。`Transform::from_translate(rect.origin)`，然后 `draw_image`，然后 `pop`。
- 注释：`push`/`pop` 之间不能插入可能失败的调用；不平衡会在 krilla 里 panic，不收成 `Error`。
- 路径 mark 不 push。

**表边框已经在发 stroke** — `src/layout/table.rs`

- `rect_path`（393–399 行，私有）：`MoveTo(x,y)` → `LineTo(x+w,y)` → `LineTo(x+w,y+h)` → `LineTo(x,y+h)` → `Close`。
- `measure`（236–267 行）：`Positive::pt(border)` 成功才 `Stroke::new`。`paint = Paint::solid(grid.border.color)`。每个 cell 一条 `FragmentCommand::Stroke`，路径是格子外框。`border` 为 0 时 `Positive::pt` 失败，`stroke` 为 `None`，不 push。
- 公共 DSL 的 `Flow::table` 文档写 “borderless”（`src/document.rs` 598 行）。默认 `Border.width = NonNegative::ZERO`、颜色 `Color::rgb(0,0,0)`（`table.rs` 42–45 行）。描边路径在 layout 里，当前公共表不打开它。
- 测试 `borders_stroke_each_cell_centered_on_grid_lines`：每格一条闭合矩形，页边距加在坐标上。`zero_border_draws_no_strokes`：零宽不产生 stroke。

**`FragmentCommand`** — `src/layout/flow.rs` 77–89、338–363 行

- 三臂：`Glyphs`、`Stroke { path, paint, stroke }`、`Image`。没有 `Fill`。
- `draw_fragment` 对 stroke 调用 `canvas.stroke_path(&path.translated(ox, oy)?, ...)`。origin 来自 `PlacedFragment`。
- `Fragment::with_offset` 同样只平移 stroke 路径。layout 里没有任何 `fill_path` 调用点。

**behind / front 今天** — `src/document.rs`、`src/layout/decorate.rs`、`src/layout/paginate.rs`

- `Section` 和隐式 `Document` 各有 `behind` / `front`。后一次调用替换前一次。显式 section 之后，`Document::behind/front` 走 `fail_outside`。
- 闭包类型 `LayerHook = Box<dyn Fn(&mut Layer) + 'a>`。`layer_content` 在每页 `PageInfo` 上跑闭包，收集 `Vec<LayerItem>`。`Layer.error` 有值则整页层失败。
- `Layer` 只有 `page()`、`is_last()`、`text_at`、`image_at`。坐标是页坐标，不扣 margin（1013 行注释）。`image_at` 在 `DocToken` 不匹配时把 `UnknownImage` 放进 `layer.error` 并 return，不 push。
- `LayerItem`：`Text(LayerText)` 或 `Image { image, rect }`。没有 path。
- `measure_layer`（180–218 行）：文字变成 `FragmentCommand::Glyphs`（`at` 加上 `single_line_run` 的 origin，对齐被改成 `Start`）；图片变成 `FragmentCommand::Image`，矩形原样。空命令返回 `None`。非空则 `PlacedFragment { origin: (0,0), fragment.height: 0 }`。
- `paint_pages`（266–278 行）顺序：`behind` → `header` → `page.placed`（正文）→ `footer` → `front`。层不占正文高度。
- 测试 `layers_paint_behind_header_body_footer_front` 用字形顺序钉住 W, H, A, F, S。

### Flow

1. 组装：`Document::behind/front` 或 `Section::behind/front` 存一个 `LayerHook`。此时不画。
2. 分页之后 `measure_bands` 对每一物理页调用 hook，得到 `LayerItem`。缺字在 `single_line_run` 失败，整份文档在开画之前失败（`document.rs` 的 `missing_glyph_in_front_fails_before_any_page`：canvas 命令为空）。
3. 层变成 origin `(0,0)`、height `0` 的 `PlacedFragment`。命令坐标已经是页坐标。
4. `paint_pages` 按叠层顺序 `draw_fragment`。stroke 会再 `translated(origin)`；层的 origin 是 0，所以层内坐标不再被 margin 平移。正文表的 stroke 会，因为 body `PlacedFragment.origin` 在 margin 内。
5. `RecordingCanvas` 记下 `FillPath`/`StrokePath`。`to_pdf` 经 `replay` 进 `KrillaCanvas`，变成 `Mark`，`emit::draw_mark` 才 `to_fill`/`to_stroke` + `to_krilla_path` + `draw_path`。
6. 图片是这条链上唯一碰 `Surface::push_transform` 的 mark，而且只做平移，立刻 `pop`。

刀 11 若沿现有层走，要新开的缝在 2–4：`Layer` 方法、`LayerItem`、`measure_layer` 的 match、以及 `FragmentCommand` 缺的 Fill。5–6 的 `fill_path`/`stroke_path` 和 alpha→opacity 已经能画轴对齐的直线路径。旋转和椭圆还进不了 `PathVerb`，也进不了 `to_krilla_path`。

### Files Read

- `src/geom.rs`（`Rect::translated`、`Color`、`Paint`、`Stroke`、`Path*`、路径测试）
- `src/canvas.rs`（trait、`CanvasCommand`、`RecordingCanvas`、`replay`）
- `src/backend/convert.rs`
- `src/backend/krilla_canvas.rs`（`Mark`、`fill_path`、`stroke_path`）
- `src/backend/emit.rs`（`draw_mark`、图片 `push_transform`、内容流实验注释）
- `src/layout/flow.rs`（`FragmentCommand`、`with_offset`、`draw_fragment`）
- `src/layout/table.rs`（`Border`、`rect_path`、描边测试）
- `src/layout/decorate.rs`（`LayerItem`、`measure_layer`、叠层测试）
- `src/layout/paginate.rs` `paint_pages`
- `src/document.rs`（`behind`/`front`、`Layer`、`layer_content`）
- `src/lib.rs` 重导出
- `src/layout/test_util.rs` `strokes`
- `docs/quire-design.md`：几何草图 226–263；「v1 没有…旋转页面」290；坐标转换 292–299；图片 translate 337–350；Canvas 与 `to_fill` 352–424
- `docs/提案书.md` §2.12（529–543）：path 原语、旋转、颜色 opacity 是产品要求，不是当前实现
- krilla 0.8.2 源码（registry）：`src/geom.rs` `Transform` / `PathBuilder`；`src/surface.rs` `set_fill`/`set_stroke`/`draw_path`/`push_transform`/`pop`/`push_opacity`

### Boundaries

- 公共 `Layer` 没有 `line` / `rect` / `ellipse` / `fill` / `stroke` / `rotate`。`LayerItem` 没有路径臂。
- `PathVerb` 没有曲线。`to_krilla_path` 的 match 也没有。krilla 自己的 `PathBuilder` 另有 `quad_to`、`cubic_to`、`push_rect`，quire 未调用。没有 ellipse API。
- `Canvas` 没有变换栈。`RecordingCanvas` / `Mark` 也没有变换字段。旋转若要进 PDF，要么先把点写进 `Path`，要么给 mark/命令加 transform，并在 `draw_mark` 里成对 `push_transform`/`pop`。
- krilla `Transform::from_rotate` / `from_rotate_at` 的 `angle` 单位是度。quire 没有包装。
- krilla `Surface::push_opacity` 会叠乘，和 `Fill.opacity` / `Stroke.opacity` 不是同一个字段。当前实现只用后者，且只来自 `Color.a`。
- `FillRule` 写死 `NonZero`。没有 even-odd。
- 设计稿 290 行的「旋转页面」指的是页面级 crop/bleed/旋转页，不是路径旋转。适配器仍是 `PageSettings::from_wh`。
- 设计稿 416 行：`stroke.width <= 0` 当 `InvalidGeometry`。实现更早：`Stroke` 的宽度品牌是 `Positive`，零宽根本造不出 `Stroke`。表用跳过 `stroke_path` 表示不画边。
- 流内 canvas 是提案书 §2.14 延后项。刀 11 的路径属于 §2.12 逃逸层，不进正文流、不占高度。
- layout 不能 `use krilla`。变换若调用 `Surface::push_transform`，只能留在 `backend/emit.rs`。

### Non-Obvious Things

- 透明度已经通了，但没有任何测试断言 `to_fill`/`to_stroke` 的 opacity。`a == 0` 仍会 `set_fill`/`set_stroke` 并 `draw_path`（设计稿 414 行：照画，opacity 为 0）。`u8 as f32 / 255` 落在 `0..=1`，`NormalizedF32::new` 对合法 `Color` 得到 `Some`；`InvalidGeometry` 那条臂对 `u8` alpha 实际上到不了。
- 单独 `MoveTo` 能进 `RecordingCanvas`，`to_krilla_path` 才失败。层上如果允许 `Path::begin(p).finish()`，记录测试会绿，PDF 会在 emit 失败。
- `Path::translated` 和 `Rect::translated` 都会经 `try_pt`。极大坐标平移可以在绘制时变成 `NonFiniteLength`，而不是静默溢出。
- 表的矩形路径是私有 `rect_path`，不经过公共 `Path` 助手。相邻格各画一次自己的四边，共享边画两次。设计稿 1298 行接受双边框重叠。
- 层 fragment 的 height 是 0，origin 是 `(0,0)`。文字的 `at` 是行盒左上，再加 run 里的 baseline，不是字形基线本身（测试里 y 是 `120.0 + baseline`）。路径若直接用页坐标，不应再加 baseline，也不应再加 margin。
- 图片的 `push_transform` 只影响那一次 `draw_image`。下一条 path mark 回到页面 CTM。路径旋转若抄这个模式，必须在同一个 `draw_mark` 臂里 push 和 pop，中间不能 `?`。
- krilla `draw_path`：fill 和 stroke 都是 `None` 时用黑色填充（`surface.rs` 161–169 行；设计稿 52 行）。quire 的 fill/stroke 臂都先把另一支笔清掉，避免表线被填、字形沿用上一次 stroke。
- 内容流实验（`emit.rs` 254–260 行）：200pt 页根变换是 `1 0 0 -1 0 200 cm`，路径操作数仍是 API 坐标 `10 20` / `40 20` / `40 50`。注释矩形才是翻转后的 `[10 126 90 170]`。给路径加旋转时，不要为了内容流数字去改 `to_krilla_point`。
- `RecordingCanvas::pages` 的 match 对命令变体是穷尽的。新增 Transform 命令必须改这里和 `replay`，否则编不过。

### Open Questions

- 旋转是烘焙进路径点（`Canvas` 不变），还是一条带 `from_rotate_at` 的 mark（要新 `CanvasCommand`、新 `Mark` 字段，且 stroke 宽度跟着 CTM）？现有代码两条都没做。
- 椭圆是折线近似（现有 `LineTo` 就够），还是给 `PathVerb` 加 quad/cubic 并扩展 `to_krilla_path`？krilla 能收曲线，quire 的动词不能表达。
- `Layer` 上 fill 与 stroke 是两个方法，还是一条路径加一个 paint 模式？`FragmentCommand` 今天只有 `Stroke`，fill 必须新增一臂，`draw_fragment` / `with_offset` 才画得出。
- 层上的路径错误（非有限点、krilla 拒收的空路径）是像 `image_at` 一样记在 `Layer.error`，还是像测量缺字一样从 `measure_layer` 返回 `Result`？空路径要到 emit 才失败，记录画布看不出来。
- 半透明是否只复用 `Color::rgba`（后端已接上），还是还要 `Surface::push_opacity`？两者叠乘，设计稿只描述了颜色 alpha。
- 旋转中心是路径自身的点、调用方给的页坐标点，还是 krilla `from_rotate` 的原点？公共 API 还没有这个参数。
