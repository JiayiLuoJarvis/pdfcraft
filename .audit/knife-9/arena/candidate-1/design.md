# 刀 9 候选 1 —— 字形同构（Glyph analogue）

基线 `7cbe823`。方向：图片走字体已经走通的那条路。注册时一次解码，库内表持有「已解码的不透明句柄」；layout 只拿 `DecodedImage`（id + 像素）和**注册时就解好的 pt 尺寸**；`KrillaCanvas::draw_image` 像 `draw_glyph_run` 一样按 id 查表，把发射所需材料塞进 `Mark::Image`，`finish` 先丢掉表再发射。`LayoutCtx` 不动。

---

## Usage（调用方视角，先写）

### README 片段

```rust
use quire::{document, Align, Cell, ImageFormat, Margins, NonNegative, Positive, Rect, A4};

let pdf = document(A4, |doc| {
    doc.font(std::fs::read("NotoSansSC.ttf")?)?;
    doc.margins(Margins::uniform(NonNegative::pt(56.0)?));

    // 注册一次：校验字节上限 → 解码 → 拿回可 Copy 的句柄。
    // 头部损坏、格式不符、超限，都在这里立刻报错。
    let logo = doc.load_image(std::fs::read("logo.png")?, ImageFormat::Png)?;
    let seal = doc.load_image(seal_bytes, ImageFormat::Png)?;
    let w120 = Positive::pt(120.0)?;

    // 流内图：整块不可拆，放不下就整张去下一页
    doc.image(&logo, |i| {
        i.width(w120);            // 只给一边：高按像素比算
        i.align(Align::Center);
    });
    doc.image(&logo, |_| {});     // 两边都不给：1 px = 1 pt

    // 表格单元格里放图
    doc.table(|t| {
        t.header(["项目", "签章"]);
        t.row([Cell::new("甲方"), Cell::image(&seal, |i| i.height(Positive::pt(40.0).unwrap()))]);
    });

    // 页面坐标逃逸：闭包每页执行，句柄是 Copy，直接 move 进去
    doc.front(move |page| {
        if page.is_last() {
            page.image_at(&seal, Rect::try_pt(430.0, 680.0, 90.0, 90.0).unwrap());
        }
    });
    Ok(())
})?
.to_pdf()?;
```

### 调用点 2：水印（behind，每页一张，同一句柄）

```rust
let mark = doc.load_image(wm_png, ImageFormat::Png)?;
doc.behind(move |page| {
    let s = page.page().size;
    let rect = Rect::try_pt(0.0, 0.0, s.width().as_pt(), s.height().as_pt()).unwrap();
    page.image_at(&mark, rect);   // 宽高都给：拉伸，不保比
});
// 三页文档 → 录制里三条 DrawImage，PDF 里只有一个图像 XObject（同一 krilla Image）。
```

### 调用点 3：库内测试（无 backend 也能跑的 layout 断言）

```rust
let img = DecodedImage { id: ImageId::from_raw(0), pixel_width: 200, pixel_height: 100 };
let node = ImageNode::new(img, Some(Positive::pt(100.0)?), None, TextAlign::End)?; // → 100×50 pt
let rec = render(FlowNode::Image(node), frame_w = 300.0);
assert_eq!(rec.draw_images(), [(img, Rect::try_pt(200.0, 0.0, 100.0, 50.0)?)]); // to_bits 比较
```

### 调用方看得到的错误

| 时机 | 情形 | 错误 |
| --- | --- | --- |
| `load_image` | 字节 > 32 MiB（解码前） | `Error::Image { message }` |
| `load_image` | 头部坏 / 格式不符 / 0 像素边 / 像素数超限 | `Error::Image { message }` |
| `load_image` | 未开 `backend-krilla` | `Error::Image { message: "… needs feature `backend-krilla`" }` |
| `document()` | 只给一边算出的另一边下溢为 0 | `Error::NonPositiveLength`（`Positive::pt` 原样） |
| `document()` | 别的文档的句柄 | `Error::UnknownImage { id }` |
| `document()` | 显示宽 > 当前框 `W` | `Error::UnbreakableWiderThanFrame { width, avail }` |
| `document()` | 高 > 整页正文框 | `Error::UnsplittableTallerThanFrame` |
| `document()` | 图放进列表项 / 页眉页脚 | `Error::InvalidCellContent { node: "image", .. }` |
| `to_pdf()` | 头部好、像素数据坏 | `Error::Backend(..)`（krilla 延迟报错，见风险） |

---

## Shape

### 模块地图（改动范围）

```text
src/canvas.rs                 + pub enum ImageFormat          （图片词汇和 ImageId/DecodedImage 放一起；backend 可向下依赖）
src/error.rs                  + Image { message }, UnknownImage { id }；UnbreakableWiderThanFrame 文档放宽
src/backend/images.rs   (新)  ImageTable / Raster / decode / 两个上限常量 —— 全仓唯一碰 krilla::image 的文件之一
src/backend/mod.rs            + mod images; pub(crate) use images::ImageTable;
src/backend/krilla_canvas.rs  + images: ImageTable 字段；Mark::Image；draw_image 换掉毒化桩；finish 先 drop 表
src/backend/emit.rs           + Mark::Image 分支（push_transform → draw_image → pop）；替换桩测试
src/layout/image.rs     (新)  ImageNode + resolve_size（纯函数）+ split（原子块）
src/layout/mod.rs             + pub mod image;
src/layout/flow.rs            + FlowNode::Image / FragmentCommand::Image；with_offset、draw_fragment、split 各一臂
src/layout/list.rs            first_baseline 一臂（None）
src/layout/lower.rs           check_content / node_name 一行
src/layout/decorate.rs        LayerContent: Vec<LayerText> → Vec<LayerItem>；measure_layer 一臂
src/document.rs               Image 句柄、ImageBox、Align、load_image、Flow::image、Cell::image、Layer::image_at、Rendered 带表
src/lib.rs                    + Align, Image, ImageBox, ImageFormat 重导出
```

`LayoutCtx`、`paginate.rs`、`table.rs`、`columns.rs`、`Cargo.toml` **不动**。

### 数据流

```text
load_image(bytes, fmt)
  └─ backend::ImageTable::register ── 上限 → Image::from_*(.., false) → size() → 像素上限
        存 Raster(krilla Image) ；返回 DecodedImage{id, px}
  └─ 包成 Image{decoded, doc: DocToken}   (Copy)

Flow::image(&img, |b|..) / Cell::image(&img, |b|..)
  └─ ImageBox::lower(img, flow.doc)  ── 校验 token → ImageNode::new(decoded, w, h, align) → resolve_size
        FlowNode::Image(ImageNode{ image: DecodedImage, size: Size, align })      ← layout 只有 id+px+pt

paginate → ImageNode::split(avail) → Fit{ FragmentCommand::Image{image, rect} } / MoveToNextPage / Err
Layer::image_at(&img, rect) → LayerItem::Image{image, rect} → measure_layer → FragmentCommand::Image
draw_fragment → canvas.draw_image(&image, rect + origin)          → RecordingCanvas::DrawImage

Rendered{ recording, fonts, images } ── to_pdf ──> KrillaCanvas::new_with_resources(fonts, images)
replay → KrillaCanvas::draw_image: images.get(id) → Mark::Image{ raster.clone(), rect }
finish: drop(font_store, images) → emit::render → draw_mark(Image) → surface.draw_image(raster.krilla().clone(), size)
```

### 类型草图

#### `src/canvas.rs`（只加一个枚举）

```rust
/// Encoded format of image bytes passed to [`crate::Document::load_image`].
/// GIF and WebP draw their first frame as a still image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ImageFormat { Png, Jpeg, Gif, Webp }
```

`ImageId` / `DecodedImage` / `Canvas::draw_image` / `RecordingCanvas` 不变。

#### `src/error.rs`

```rust
/// Image bytes were rejected: over a limit, unparsable, empty, or no decoder in this build.
Image { message: String },
/// `id` is not registered in this document's image table.
UnknownImage { id: u32 },
// UnbreakableWiderThanFrame 的文档改为：
/// Content that cannot break or shrink is wider than the available frame width.
```

不拆出 `ImageTooLarge` / `LimitKind`：照 `Error::Font { message }` 和 `MAX_FONT_BYTES` 的先例，一类输入一种变体（per encode-lessons-in-structure：调用方要分支的只有"这张图不能用"）。

#### `src/backend/images.rs`（新，`backend-krilla` 下）

```rust
//! Image decode and the per-document image table. Only this file and `emit` name krilla images.

use krilla::image::Image as KrillaImage;
use krilla::Data;

use crate::canvas::{DecodedImage, ImageFormat, ImageId};
use crate::error::{Error, Result};

/// Encoded bytes above this are rejected before krilla sees them. Without `rayon`,
/// `Image::from_*` decodes pixels synchronously, so this must run first.
pub(crate) const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
/// Decoded pixel count above this is rejected after the header is read.
pub(crate) const MAX_IMAGE_PIXELS: u64 = 40_000_000;

/// Decoded image ready to paint. Cheap to clone (krilla `Image` is an `Arc`).
#[derive(Clone)]
pub(super) struct Raster(KrillaImage);

impl Raster {
    pub(super) fn krilla(&self) -> &KrillaImage { &self.0 }
}
impl std::fmt::Debug for Raster { /* "Raster(w×h)" — 不依赖 krilla 的 Debug 输出 */ }

/// Images registered with one document, indexed by `ImageId`.
/// Ids are dense from 0; the table is append-only, so an id never changes meaning.
#[derive(Debug, Default)]
pub(crate) struct ImageTable { rasters: Vec<Raster> }

impl ImageTable {
    /// Checks the byte limit, decodes with `interpolate = false`, checks pixel limits,
    /// and returns the id plus pixel size layout will use.
    pub(crate) fn register(&mut self, bytes: Vec<u8>, format: ImageFormat) -> Result<DecodedImage> {
        // if bytes.len() > MAX_IMAGE_BYTES → Error::Image("image is N bytes; max is …")
        // let data = Data::from(bytes);
        // let image = match format { Png => KrillaImage::from_png(data, false), Jpeg => from_jpeg, Gif => from_gif, Webp => from_webp }
        //     .map_err(|message| Error::Image { message })?;
        // let (w, h) = image.size();
        // if w == 0 || h == 0 → Error::Image("image has an empty side")
        // if u64::from(w) * u64::from(h) > MAX_IMAGE_PIXELS → Error::Image(..)
        // let id = ImageId(u32::try_from(self.rasters.len()).map_err(..)?);
        // self.rasters.push(Raster(image));
        // Ok(DecodedImage { id, pixel_width: w, pixel_height: h })
        todo!("not implemented")
    }

    pub(super) fn get(&self, id: ImageId) -> Option<&Raster> { self.rasters.get(id.0 as usize) }
}
```

#### `src/backend/krilla_canvas.rs`

```rust
pub(super) enum Mark {
    Fill { .. }, Stroke { .. }, Glyphs { .. },
    /// Decoded image painted into `rect` (page coordinates, top-left, y down).
    Image { raster: Raster, rect: Rect },
}

pub(crate) struct KrillaCanvas {
    font_store: Arc<FontStore>,
    images: ImageTable,          // 新
    /* 其余不变 */
}

impl KrillaCanvas {
    pub(crate) fn new_with_resources(fonts: Arc<FontStore>, images: ImageTable) -> Self;
    // new() / new_with_fonts / *_observable_* 传 ImageTable::default()
}

fn draw_image(&mut self, image: &DecodedImage, rect: Rect) -> Result<()> {
    self.open_page_mut("draw_image")?;
    let raster = match self.images.get(image.id) {
        Some(raster) => raster.clone(),
        None => return self.fail(Error::UnknownImage { id: image.id.as_u32() }),
    };
    let page = self.open_page_mut("draw_image")?;
    page.marks.push(Mark::Image { raster, rect });
    Ok(())
}

// finish(): `drop(font_store); drop(images);` —— marks 已自带 Raster，发射不查表。
```

文件头的"不 `use krilla`"仍成立：`Raster` 是本仓库类型。

#### `src/backend/emit.rs`

```rust
Mark::Image { raster, rect } => {
    let (x, y) = (rect.origin().x().as_pt(), rect.origin().y().as_pt());
    let size = krilla::geom::Size::from_wh(rect.size().width().as_pt(), rect.size().height().as_pt())
        .ok_or(Error::InvalidGeometry)?;
    surface.push_transform(&krilla::geom::Transform::from_translate(x, y));
    surface.draw_image(raster.krilla().clone(), size);   // 顶边落在当前原点；不翻 y
    surface.pop();                                       // 与 push 成对，中间没有 `?`
}
```

#### `src/layout/image.rs`（新）

```rust
//! Atomic raster block: resolved size, Start/Center/End inside the frame, never splits.

/// An image in the flow. `size` is resolved when the node is built, so split only places it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImageNode {
    image: DecodedImage,
    size: Size,
    align: TextAlign,
}

impl ImageNode {
    /// Pixel sides must be non-zero (guaranteed by `ImageTable::register`).
    pub(crate) fn new(image: DecodedImage, width: Option<Length<Positive>>,
                      height: Option<Length<Positive>>, align: TextAlign) -> Result<Self> {
        let size = resolve_size(image.pixel_width, image.pixel_height, width, height)?;
        Ok(Self { image, size, align })
    }

    pub(crate) fn split(self, avail: Frame) -> Result<Split> {
        // let (w, h) = (self.size.width().as_pt(), self.size.height().as_pt());
        // if w > avail.width        → Err(UnbreakableWiderThanFrame { width: w, avail: avail.width })
        // if h > avail.full_height  → Err(UnsplittableTallerThanFrame)
        // if h > avail.height       → Ok(MoveToNextPage(FlowNode::Image(self)))
        // let x = match align { Start => 0, Center => (avail.width - w) / 2, End => avail.width - w };
        // Ok(Fit { height: h, fragment: Fragment { height: h, spacing_after: 0.0,
        //          commands: vec![FragmentCommand::Image { image, rect: Rect::try_pt(x, 0.0, w, h)? }] } })
        todo!("not implemented")
    }
}

/// §2.7 size rules. Both → as given (aspect not kept); one → other from pixel aspect; none → 1 px = 1 pt.
pub(crate) fn resolve_size(px_w: u32, px_h: u32, width: Option<Length<Positive>>,
                           height: Option<Length<Positive>>) -> Result<Size> {
    // (Some(w), Some(h)) => Size::new(w, h)
    // (Some(w), None)    => Size::new(w, Positive::pt(w.as_pt() * px_h as f32 / px_w as f32)?)
    // (None, Some(h))    => Size::new(Positive::pt(h.as_pt() * px_w as f32 / px_h as f32)?, h)
    // (None, None)       => Size::try_pt(px_w as f32, px_h as f32)
    todo!("not implemented")
}
```

宽度检查放在高度之前：同一张过宽的图无论落在页中还是页顶都报同一个错误。

#### `src/layout/flow.rs`

```rust
pub(crate) enum FlowNode { /* …九个… */ Image(ImageNode) }
// split:          FlowNode::Image(node) => node.split(avail),
pub(crate) enum FragmentCommand { Glyphs(..), Stroke { .. }, Image { image: DecodedImage, rect: Rect } }
// with_offset:    FragmentCommand::Image { rect, .. } => *rect = rect.translated(dx, dy)?,
// draw_fragment:  FragmentCommand::Image { image, rect } => canvas.draw_image(image, rect.translated(ox, oy)?)?,
```

`Rect::translated(dx, dy) -> Result<Rect>` 加在 `geom.rs` 里，`pub(crate)`，与 `Path::translated` 并列（两处平移共用）。

#### `src/layout/lower.rs` / `list.rs`

```rust
FlowNode::Image(_) => matches!(container, Container::Column | Container::TableCell),
// node_name: FlowNode::Image(_) => "image",
// list::first_baseline: FragmentCommand::Image { .. } => None,
```

#### `src/layout/decorate.rs`

```rust
/// One behind/front primitive, painted in the order the closure added it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LayerItem {
    Text(LayerText),
    /// Page-coordinate rect; not inset by margins, not checked against the page.
    Image { image: DecodedImage, rect: Rect },
}
/// Same shape as `BandContent`: the deferred builder error surfaces here.
pub(crate) type LayerContent<'a> = Box<dyn Fn(&PageInfo) -> Result<Vec<LayerItem>> + 'a>;
// measure_layer: `for item in content(page)? { match item { Text(..) => 现逻辑, Image { image, rect } => commands.push(FragmentCommand::Image { image, rect }) } }`
```

`LayerContent` 改成返回 `Result`，与已有的 `BandContent` 同形，`measure_bands` 本来就在 `Result` 里，出错时画布仍为空。调用方的闭包签名不变（仍是 `Fn(&mut Layer)`），错误像 `Flow::push` 一样先记在 `Layer` 里。

#### `src/document.rs`

```rust
/// An image registered with one document. Copy it into page closures freely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Image { decoded: DecodedImage, doc: DocToken }

impl Image {
    pub fn pixel_width(&self) -> u32;
    pub fn pixel_height(&self) -> u32;
}

/// Identifies one `Document`. Never reaches the canvas or the PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DocToken(u32);   // 来自进程内 AtomicU32 计数器，只用于句柄归属校验

/// Horizontal placement of an image narrower than its frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Align { #[default] Start, Center, End }

/// Display size and alignment of a flow or cell image. Unset sides follow §2.7.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageBox { width: Option<Length<Positive>>, height: Option<Length<Positive>>, align: Align }

impl ImageBox {
    pub fn width(&mut self, w: Length<Positive>);
    pub fn height(&mut self, h: Length<Positive>);
    pub fn align(&mut self, a: Align);
    /// The one lowering path for flow and cell images.
    fn lower(self, image: &Image, doc: DocToken) -> Result<FlowNode> {
        // if image.doc != doc → Err(UnknownImage { id: image.decoded.id.as_u32() })
        // ImageNode::new(image.decoded, self.width, self.height, self.align.into()).map(FlowNode::Image)
        todo!("not implemented")
    }
}

pub struct Document<'a> {
    /* … */
    doc: DocToken,
    #[cfg(feature = "backend-krilla")]
    images: crate::backend::ImageTable,
}

impl Document<'_> {
    /// Decodes `bytes` once and returns a handle for `image`, cells, and `image_at`.
    /// Fails now on oversized, unparsable, or empty images, or without `backend-krilla`.
    pub fn load_image(&mut self, bytes: Vec<u8>, format: ImageFormat) -> Result<Image>;
}

pub struct Flow { style: Style, nodes: Vec<FlowNode>, error: Option<Error>, doc: DocToken }  // child() 继承 doc

impl Flow {
    /// Appends an image that moves to the next page whole instead of splitting.
    /// Wider than the frame or taller than the page body is an error; it never shrinks.
    pub fn image(&mut self, image: &Image, f: impl FnOnce(&mut ImageBox)) {
        // let mut b = ImageBox::default(); f(&mut b); self.push(b.lower(image, self.doc));
    }
}

pub struct Cell { content: CellContent, col_span, row_span }        // 仍 Eq
enum CellContent { Text(String), Image(Image, ImageBox) }
impl Cell {
    /// A one-by-one cell holding `image`.
    pub fn image(image: &Image, f: impl FnOnce(&mut ImageBox)) -> Self;
}
// Table::lower: CellContent::Text(t) => flow.text(t), CellContent::Image(i, b) => b.lower(&i, flow.doc)?

pub struct Layer { page: Page, style: Style, doc: DocToken, items: Vec<LayerItem>, error: Option<Error> }
impl Layer {
    pub fn text_at(&mut self, s: impl AsRef<str>, at: Point);       // 直接推 LayerItem::Text
    /// Draws `image` stretched into `rect`, in page coordinates. Not clipped or margin-inset.
    pub fn image_at(&mut self, image: &Image, rect: Rect) {
        // if image.doc != self.doc → 记 UnknownImage（只记第一个），不推
        // else items.push(LayerItem::Image { image: image.decoded, rect })
    }
}
// layer_content: hook(&mut layer); match layer.error { Some(e) => Err(e), None => Ok(layer.items) }

pub struct Rendered {
    recording: RecordingCanvas,
    fonts: FontStore,
    #[cfg(feature = "backend-krilla")]
    images: crate::backend::ImageTable,
}
// to_pdf: KrillaCanvas::new_with_resources(Arc::new(self.fonts), self.images)
```

id 是稠密下标，所以别家文档的句柄如果放过去，到 `to_pdf` 会**静默画成本文档同下标的另一张图**，而不是报 `UnknownImage`。因此 flow、cell、layer 三个入口都在 DSL 边界校验 `DocToken`，`KrillaCanvas` 里的 `UnknownImage` 只兜越界。

### 关键决定

1. **注册即解码，句柄 = 已解码结果。** 不开 `rayon` 时 `from_*` 会同步解完像素（`util.rs:193-208`），而只给一边或都不给时 layout 本来就要像素尺寸，所以解码这一步省不掉。既然已经付了代价，就把 krilla `Image` 留下来（`Raster`），不要在 emit 里再解一遍。"每页重复一张章"的问题（how.md gotcha 5）因此不需要缓存：同一 `Raster` clone 进每页的 mark。（per data-structures-first：不出现"以后加个缓存"。）
2. **表不进 `LayoutCtx`。** layout 需要的只有像素尺寸，而它在句柄里。`ImageNode` 构造时就把 §2.7 三条规则解成 `Size`，split 只做比较和放置。layout 层对图片的全部知识是 `DecodedImage + Size + TextAlign`，满足 §2.7"layout 只持 `ImageId` 与 pt 矩形"。
3. **表的所有权线性移动。** `Document`（唯一写者，`register`）→ `Rendered` → `KrillaCanvas`（只读）→ `finish` 丢弃。没有共享写，没有 `Arc<Mutex>`（per separate-before-serializing-shared-state）。
4. **`Mark::Image` 自带发射材料**，与 `Mark::Glyphs` 自带字体字节同构；`emit::render` 签名不变。
5. **krilla 关在 `backend/images.rs` + `emit.rs`。** `Raster` 字段私有，只有 `pub(super) fn krilla()`；`ImageTable` 对 crate 只暴露 `register`。公共 API 只有 `ImageFormat`、`Image`、`ImageBox`、`Align`（per boundary-discipline）。
6. **一条降级路径。** `Flow::image` 和 `Cell::image` 都走 `ImageBox::lower`；`image_at` 是另一件事（页面矩形，无对齐、无分页），不共用 `ImageBox`，避免"有字段被静默忽略"。
7. **句柄归属在 DSL 边界校验。** `DocToken` 让"别的文档的句柄"在 `document()` 返回前报错，而不是悄悄画成本文档里同下标的另一张图（id 是稠密下标，这个风险是真的）。

### 接口深度

公共面：1 个方法（`load_image`）、1 个句柄、3 个调用点（`Flow::image`、`Cell::image`、`Layer::image_at`）、1 个三字段 builder、2 个小枚举。藏在后面的：字节/像素上限、四种格式解码与首帧、0 边拒绝、尺寸三规则与下溢、对齐算 x、原子分页三分支、容器矩阵、两段坐标平移、跨页去重、krilla transform 配对。调用方不需要知道 `ImageId`。

### 不做

具名字典、fit/cover、图上链接、`image_at` 越界检查、旋转/透明、列表项和页眉页脚里的图、无 backend 下的解码、公共 `Limits`。

---

## Synthesis decision

n/a — filled by orchestrator

## Tradeoffs accepted

- 我们接受**解码后的像素从 `load_image` 一直留在内存里到 `to_pdf`**（只录制不出 PDF 也一样），换来每张图整份文档只解码一次、无 emit 侧缓存、头部错误提前到注册时。
- 我们接受**未开 `backend-krilla` 时 `load_image` 一律报错**（即使调用方宽高都给了），换来句柄永远带真实像素尺寸、layout 里没有 `Option<px>` 分支、不引入第二套头部解析依赖。默认 feature 开着，影响面只有显式关 feature 的人。
- 我们接受**krilla 类型出现在一个 crate 内结构体（`Raster`）里，并被 `Document`/`Rendered` 以 cfg 字段持有**，换来不在 emit 里重复解码。`Raster` 不可从 `backend` 外构造或解包，`krilla_canvas.rs` 仍不 import krilla。
- 我们接受**像素上限是事后检查**：`from_*` 已经同步分配完像素才拿得到 `size()`，所以 `MAX_IMAGE_PIXELS` 限制的是"留存与嵌入"，不是解码峰值；峰值只靠 32 MiB 字节上限和解码库自身限额兜底。
- 我们接受**复用 `UnbreakableWiderThanFrame`** 报图片超宽（放宽其文档措辞），换来不新增一个语义相同的变体；代价是错误名带着文字时代的"unbreakable"。
- 我们接受**`Flow::image(&img, |_| {})` 这种空闭包写法**，换来与 `paragraph`/`table`/`columns` 一致的闭包 DSL，以及 flow 与 cell 共用一个 `ImageBox`。
- 我们接受**一个进程级 `AtomicU32` 计数器**只为发 `DocToken`；它不进录制、不进 PDF，输出仍然确定。
- 我们接受**`LayerContent` 从不可失败改为返回 `Result`**（与 `BandContent` 同形），换来外来句柄在 `document()` 阶段报错、不会静默画错图。

## Alternatives considered

- **字节进 mark，emit 里按需 `from_*`（最字面的字形同构）。** `Mark::Image { bytes: Arc<[u8]>, format }`，emit 每遇一次就解码。为了拿像素尺寸注册时本来就要解一次，所以这是双解码；每页一章还得在 `render` 里按 id 加缓存，正是"以后加个缓存"那种结构错误。接口一样小，但把复杂度推进了 emit。落选。
- **`ImageStore` 穿过 `LayoutCtx`（并行仓库）。** layout 在 split 时按 id 查像素尺寸。layout 多知道一张表，`LayoutCtx` 扩字段，所有 split 签名都背着它；而尺寸是注册时就确定的不变量，没必要在 split 时再查。接口没变小，耦合变多。落选。
- **句柄自带 `Raster`，不要表（`Image::decode(bytes)` 独立构造，不经 `Document`）。** 调用方在 `document()` 外就能造句柄，体验更自由；但 layout 节点和层条目就得携带 `Raster` 才能把它送到 backend，违背 §2.7"layout 只持 `ImageId` 与 pt 矩形"，还把 krilla 句柄拉进 layout 层。落选。
- **`ImageNode` 在 split 时才解尺寸。** 尺寸只依赖像素和调用方给的边，不依赖 `avail`；推迟只会让下溢错误晚到、让 split 多一份逻辑。落选。

## Open questions and risks

1. 头部完好、像素数据损坏的图，`document()` 成功、`to_pdf()` 才报 `Error::Backend`。是接受并写测试固定，还是要求在 `load_image` 里强制求值 krilla 的 `Deferred`（krilla 0.8.2 没有公开 API，可能做不到）？
2. 同一 `Raster` clone 进多页时，krilla 是否只写一个图像 XObject？我判断它按 `Image` 的 `Hash/Eq` 去重，但没在 registry 源码里核实。实现第一步应该写 backend 测试（observable 序列化，三页一章，数 `/Subtype /Image`）。如果不去重，就需要别的方案，本设计的"无缓存"论点也随之失效。
3. `DocToken` 值不值？它防的是"句柄被带出 `document()` 闭包、塞进另一份文档"。这种用法要绕路才写得出来（外层 `let mut slot`），但一旦发生就是静默画错图。如果判断这种场景不现实，可以把 token 和 `LayerContent -> Result` 一起删掉，大约少 30 行。
4. 超宽判断要不要容差？表格格宽由浮点比例算出，一张标称 = 格宽的图可能因 1 ulp 被拒。是用严格 `>`，还是用 `PT_ABS_TOLERANCE`？
5. `Align` 取通用名（以后段落对齐也用它），还是叫 `ImageAlign`，把通用名留给样式那一刀？
6. `MAX_IMAGE_PIXELS = 40_000_000` 和 32 MiB 都只是设计稿数字，提案书未确认。这一刀先作为 crate 私有常量落地，可以吗？
7. png/gif/webp 解码库在 krilla 内部有没有自己的内存限额（防解压炸弹）？如果没有，32 MiB 的 PNG 仍可能在 `from_*` 里分配出数 GB。

## Red-flag screen

| 红旗 | 结论 |
| --- | --- |
| 浅模块 | 公共面 7 项，背后是解码、限额、尺寸规则、原子分页、去重、transform 配对。调用方一次调用完成一件事，不需要协调多个方法。✓ |
| 信息泄漏 | krilla `Image` 只在 `backend/images.rs`、`emit.rs`；公共 API 只有自有枚举和句柄。像素尺寸同时存在于句柄和表里，但它在注册时写一次、之后不可变（派生，不是同步）。✓ |
| 时序拆分 | 解码、限额、id 分配都在 `ImageTable::register` 一处；尺寸规则和放置都在 `layout/image.rs`。没有按"load / validate / store"拆文件。✓ |
| 直通方法 | `load_image` 在 `register` 之上加了 token 和 cfg 策略；`ImageBox::lower` 加了归属校验和尺寸解析。✓ |
| 所有权分裂 | `ImageTable` 只有一个写者（`Document::load_image`），之后整表 move，没有副本。✓ |
| 一事两法 | flow 和 cell 共用 `ImageBox::lower`；`image_at` 是另一件事（页面矩形），不共用 builder，没有被静默忽略的字段。✓ |
| 可导入内部 | `Raster` 是 `pub(super)`，字段私有；`ImageNode` 是 `pub(crate)`；`Image` 字段私有，只能经 `load_image` 构造，所以"0 像素边"和"token 有效"这两个不变量无法从外部绕过。✓ |
| 手工同步清单 | `FlowNode` / `FragmentCommand` / `Mark` 的新变体都在穷尽 `match` 里，漏一处编译就会失败。`ImageFormat` 到 `from_*` 的映射只在 `register` 里出现一次。✓ |
| 待观察 | `Document`、`Rendered` 上的 `#[cfg] images` 字段，加上 `load_image` 里的 cfg 分支，一共 3 处 cfg。如果后面再长出更多 cfg，应该把它们收进一个小的 `Resources { fonts, images }`。 |

## Next implementation step

先写 `backend/images.rs` 的 `ImageTable::register` 和 `emit` 里的 `Mark::Image` 分支，加上"1×1 PNG 三页同章 → `%PDF` 且只有一个图像 XObject"的 backend 测试。这样在动 layout 之前，就把开放问题 2（本设计的承重假设）钉死。
