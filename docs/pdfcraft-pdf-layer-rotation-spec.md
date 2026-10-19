# pdfcraft-pdf §2.12 LayerMark 旋转（斜向水印）规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-19 |
| 状态 | **已确认。** 提案书 §2.12（2026-10-19）。与提案书冲突时以提案书为准 |
| 范围 | `pdfcraft-pdf` 的 `behind` / `front`：`LayerMark::Rotated` 扩展为可包 Text / Image |
| 相关 | [`提案书.md`](提案书.md) §2.12；`crates/pdfcraft-pdf/src/spec/mod.rs`（`LayerMark` / `LayerShape`）；`spec/lower.rs`；`layout/decorate.rs`；`canvas.rs`；`backend/` |
| 调用方 | `/mnt/data/demo/loco-rs` 的 `pdfcraft-docs` 当前**未使用** `LayerShape` / `Rotated`；本变更对其存量模板无感 |

本文是实现合同。新会话按本文落地；不重开产品讨论。

## 1. 产品一句话

斜向叠字水印（及需要倾斜的页坐标图）走现有逃逸层：`behind` / `front` + `LayerMark::Rotated`。不另设 `watermark` API。顶层 `LayerMark::Text` / `Image` 仍不旋转。

## 2. 范围

### 2.1 做（本 PR / 本主题）

- `LayerShape` → `LayerShape<K>`，增加 `Text`、`Image`。
- `LayerMark::Rotated.shapes: Vec<LayerShape<K>>`；path / Text / Image **可混排**。
- 绘制期对 Text / Image（及宜统一的 path）施加绕 `pivot` 的顺时针旋转。
- 集成测试：`RecordingCanvas` 断言斜向文字/图片指令；path 旋转行为不回退。
- `CHANGELOG.md` 记 breaking：`LayerShape` 增加类型参数 `K`。
- 更新 crate 文档（`guide.md` / README 图层一节）写明 `Rotated` 可含 Text/Image。

### 2.2 不做

- 平行的文档级 `watermark` API。
- 嵌套 `Rotated`。
- 公开测宽 / 自动居中辅助（调用方自估字宽字高并偏移 `at`）。
- `LayerMark::Text` / `Image` 顶层自带 `degrees` 字段。
- 流内 canvas、具名 images 字典、章跟流走、某页关 header/footer（见提案书总表）。
- `pdfcraft-post` 的 Watermark（仍延后；与本条无关）。
- 改 loco-rs / hcsw 工程文件。

## 3. 公共 API

### 3.1 形状

```rust
pub enum LayerMark<K> {
    Text { when, text, font, at, style },
    Image { when, image, rect },
    Line { when, from, to, pen },
    Rect { when, rect, ink },
    Ellipse { when, rect, ink },
    Rotated {
        when: PageWhen,
        degrees: f32,          // 顺时针，单位度
        pivot: Point,          // 页坐标
        shapes: Vec<LayerShape<K>>,
    },
}

pub enum LayerShape<K> {
    Line { from, to, pen },
    Rect { rect, ink },
    Ellipse { rect, ink },
    Text {
        text: Vec<TextRun>,
        font: K,
        at: Point,             // 行盒左上，旋转前页坐标
        style: RunStyle,
    },
    Image {
        image: ImageRef,
        rect: Rect,            // 目标矩形，旋转前页坐标
    },
}
```

字段语义与顶层 `LayerMark::Text` / `Image` 对齐；组内子项**无** `when`（跟 `Rotated.when`）。

### 3.2 语义

| 项 | 约定 |
| --- | --- |
| 坐标系 | 页左上原点，y 向下；与全库一致 |
| 旋转 | 绕 `pivot` **顺时针** `degrees`；与现有 path `Path::rotated` / krilla `Transform::from_rotate` 正方向一致 |
| 子项坐标 | 全部是**旋转前**的页坐标 |
| 文字 | 单行、不断行；强制换行仍失败（同现 `LayerMark::Text`）；`TextRun` 页码照旧 |
| 混排 | 同一组内 path + Text + Image 按 `shapes` 顺序画 |
| 空 `shapes` | 静默忽略 |
| 非有限 `degrees` | 失败，沿用 `GeometryKind::NonFiniteRotation` |
| 半透明 | 颜色 opacity；不加 mark 级 opacity 字段 |
| 叠层顺序 | 不变：`behind` → header → 正文 → footer → `front` |
| `PageWhen` | 不变：`All` = 本节每页；`Last` = 全文末页 |

### 3.3 调用示例（斜向水印）

```rust
section.behind.push(LayerMark::Rotated {
    when: PageWhen::All,
    degrees: 45.0,
    pivot: Point::pt(page_w / 2.0, page_h / 2.0),
    shapes: vec![LayerShape::Text {
        text: vec![TextRun::Lit("机密".into())],
        font: MyFont::Song,
        // 调用方按字号粗估半宽半高，把行盒中心对准 pivot
        at: Point::pt(page_w / 2.0 - half_w, page_h / 2.0 - half_h),
        style: RunStyle {
            font_size: Some(Length::pt(48.0)),
            color: Some(Color::rgba(/* 带 opacity */)),
            ..RunStyle::default()
        },
    }],
});
```

### 3.4 Breaking

- `LayerShape` → `LayerShape<K>`。写出 `LayerShape` / `Vec<LayerShape>` 的调用方需跟类型参数（多可推断）。
- loco-rs `pdfcraft-docs` 当前无 `LayerShape` 引用；存量模板预期零改动。

## 4. 实现约束

### 4.1 为何不能只「烤几何」

现状（`spec/lower.rs`）：path 旋转在 lower 时调用 `Path::rotated`，把点写进最终 `LayerItem::Shape`；Canvas **无** transform 指令。

文字与图片**不能**靠改 path 点完成旋转，必须在绘制期有变换（或等价地旋转每个 glyph 原点并旋转 image 绘制）。

### 4.2 推荐实现路径

1. **Canvas 增加可嵌套的 transform 栈**（`push_transform` / `pop_transform`，或单一 `with_transform` 若更贴合现有 trait）。矩阵语义与页坐标一致：平移 + 绕点顺时针旋转。`RecordingCanvas` 记录对应命令，供测试断言。
2. **`LayerItem` 携带可选旋转**（或在 paint 绕组时 push/pop）：
   - Text / Image：在画 glyph run / image 前 push「绕 pivot 转 degrees」，画完 pop。
   - path：允许继续 lower 时烤进几何（保持现测例），**或**改为与 Text/Image 同一套 transform（优先长期只保留一套旋转语义；若同 PR 改 path，须重跑现有 path 旋转集成测试）。
3. **backend（krilla）**：已有 `surface.push_transform` / `pop`（image 平移处）。扩展为支持旋转矩阵；push/pop 之间不可夹 fallible 路径导致栈不平衡（现有 image 注释已强调）。
4. **`spec/lower.rs`**：`own_marks` / `OwnedMark` / `lower_layer_marks` 识别 `LayerShape::{Text, Image}`；字体解析与顶层 Text 相同（`family_of` + `resolve_run`）；图片校验与顶层 Image 相同。
5. **模块边界**：`spec` 仍禁止 → `backend`；transform 类型用自有几何（或 canvas 层类型），不把 krilla `Transform` 漏出公共 API。

### 4.3 错误面

| 情况 | 行为 |
| --- | --- |
| `degrees` 非有限 | `NonFiniteRotation`（已有） |
| 旋转后坐标非有限（若 path 仍烤几何） | 沿用现有 path `rotated` 错误 |
| 组内 Text 缺字 / 未注册字体 | 与顶层 `LayerMark::Text` 相同 |
| 组内 Image 未知 / 越权 | 与顶层 `LayerMark::Image` 相同 |
| 文字含强制换行 | 与顶层 Text 相同 |
| `shapes` 为空 | 不报错、不画 |

### 4.4 测试要求

- 单元 / 集成：`Rotated` + `LayerShape::Text`，`RecordingCanvas` 可见旋转（transform 命令或旋转后的等效绘制），`PageWhen::All` 每页都有。
- `Rotated` + `LayerShape::Image` 至少一条。
- 混排：同一组 path + Text，顺序与 `shapes` 一致。
- 回归：现有仅 path 的 `Rotated` 用例仍通过。
- `shapes: []` 不产生图层绘制、不失败。
- `degrees: f32::NAN`（或 infinity）失败。
- `to_pdf` 冒烟即可（断言 `%PDF`），不断言压缩内容流数字。

### 4.5 文档与门禁

- 改 `CHANGELOG.md`（breaking + 新行为）。
- 改 `crates/pdfcraft-pdf/docs/guide.md` 图层表：`Rotated` 可含 Text/Image。
- 提交前跑 AGENTS.md「门禁」一节命令（至少 `pdfcraft-pdf` 的 fmt / clippy / test / doc）。
- 不擅自 commit / push，除非用户要求。

## 5. PR 切分建议

单主题一 PR 即可（API + Canvas transform + lower + 测试 + changelog）。若 diff 过大，可拆：

1. Canvas / RecordingCanvas / backend transform 栈（无公共行为变化）。
2. `LayerShape<K>` + Text/Image + lower/paint + 测试 + changelog。

不要把无关重构塞进本主题。

## 6. 验收清单

- [ ] 提案书 §2.12 与本文一致；实现未超出 §2.2「不做」
- [ ] 公共类型与 §3.1 一致；`LayerShape<K>` 已导出
- [ ] 斜向 Text 水印可经 `behind` + `Rotated` 生成 PDF
- [ ] Image 旋转可用
- [ ] path 旋转无回归
- [ ] `RecordingCanvas` 测试覆盖 §4.4
- [ ] CHANGELOG + guide 已更新
- [ ] 门禁命令通过

## 7. 权威与冲突

1. [`提案书.md`](提案书.md) §2.12  
2. 本文  
3. 设计稿未确认部分  

冲突时按上序。实现中若发现提案与 Canvas/krilla 事实冲突，停下来改提案或本文，不要 silently 歪实现。
