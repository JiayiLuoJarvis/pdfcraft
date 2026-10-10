# How explorer: DSL + proposal image rules

Source agent: `06ae14b7-05e8-4033-bb16-1383195db420` (2026-10-09).

流式 `image` 和逃逸层 `image_at` 在提案书里已经分开定过，公共 DSL 里两者都还没有。刀 8 只接到 `text_at`；位图解码和绘制仍停在画布占位。

### Components Found

**已确认的产品规则（`docs/提案书.md`，优先于设计稿）**

两条路，解码和绘制共用，API 与分页不同（§2.7 开头，约 386–388 行）：

| | 流内 `image` | `image_at` |
| --- | --- | --- |
| 层 | 正文节点，可进栏 / 单元格 / 列表项 | `behind` / `front` 页面坐标逃逸 |
| 分页 | 整块不可拆（§2.5.1、§2.7） | 不进正文流、不占正文高度（§2.12） |
| 坐标 | 跟当前框走 | 相对当前物理页左上，y 向下；左右 margin 不自动缩进 |
| 图源 | 原始字节 + 格式 | 与 §2.7 相同 |

尺寸三规则（§2.7，401–407 行；§8 再钉一次，614 行）：

- 宽和高都给：用给定 pt，不保持像素比。
- 只给一边：另一边按像素宽高比，必须先解码。
- 两边都不给：`1 px = 1 pt`，必须先解码。
- 剩余高度不够则整张去下一页。显示高大于整页正文框，或显示宽大于当前框 `W`，报错，不自动缩小。
- `fit` / `cover` 第一版不做。框内对齐默认 Start，可 Center / End。
- `interpolate` 固定 `false`。layout 只持 `ImageId` 和 pt 矩形；解码与 krilla 句柄在 backend。

格式与来源（§2.7，392–399 行）：

- 主路径：PNG / JPEG / GIF / WebP。GIF / WebP 只取第一帧，当静止图。
- 不做：`http(s)`、本地路径、Data URI 主路径。Base64 不是排版核心类型。
- 具名 `images` 字典：**延后**（§2.7 与确认总表 656 行）。
- 图上链接：随 §8，不挡图片元素本身。§8（619 行）第一版只要文字外验 URI；「仅图可点」可后补。

相邻已定、但不是图片节点：

- qr / svg / toc 不做独立元素（§2.15–§2.17）。
- 流内 canvas **延后**（§2.14）。
- 叠层：`behind` → `header` → 正文 → `footer` → `front`。
- 格内图片允许，随图片能力落地。

§1 完整模板只示范调用形状；文首写明具体函数签名等条目定完再写，所以那不是已冻结的签名。

**设计稿里才有、提案书未确认的建议**（不可当产品规则）

- 静态 `ImageBlock` 形状（已被 §1 作废的静态文档定义）。
- 限额数字：32 MiB / 4000 万像素 / 边 8192（`limits.rs` 仍占位）。
- `Error::Image` / `ImageTooLarge` 名字未钉。
- `decode_image` 草图、`LayoutCtx.images`。

**krilla 事实（设计稿已核实）**

- `Image::from_* (Data, interpolate: bool)`，`interpolate = false`；GIF 第一帧；`size() -> (u32, u32)`。
- 适配器只 `translate` 到左上，不再二次 y 翻转；`push`/`pop` 成对。

**现在的公共 DSL**

无 `image` / `image_at` / `ImageFormat`。`Layer` 只有 `text_at`。`FlowNode` 无 `Image`。`Canvas::draw_image` 已有；`KrillaCanvas` 返回 `UnsupportedCanvasOperation`。

### Flow

组装闭包一次 → lower 到 `FlowNode` → `paginate_root` → `measure_bands` 跑 header/footer/behind/front → `paint_pages`。图片应对齐：流内占高整块搬页；`image_at` 在层闭包按页矩形画。两边落到 `Canvas::draw_image`。

### Files Read

见 agent return（提案书、design、document/layout/canvas/backend、knife-8 handoff）。

### Boundaries

提案书 > AGENTS > 设计稿未确认。`ImageId`/`DecodedImage` 是画布层，不是命名字典。无图片 store。

### Non-Obvious Things

- 两边都给尺寸时不保持像素比；缺一边才需要解码。
- `Document::font` 丢弃 `FontId`；图片 id 应留库内（具名字典延后）。
- 限额数字未进确认总表。

### Open Questions

- 具体 DSL 签名
- 错误变体命名
- 是否采纳设计稿限额数字
- Center/End 对齐参照矩形
- 无 backend 时显式宽高是否可只记矩形
