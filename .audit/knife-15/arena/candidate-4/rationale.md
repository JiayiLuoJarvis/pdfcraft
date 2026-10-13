# Knife 15 candidate-4 — rationale

本文档记录 Knife 15（§8 文字外链 URI）的技术权衡、对备选方案的考量与否决理由，以及对自治默认项（Autonomy Defaults）的采纳说明。

---

## 1. 提案与框架对齐

### 1.1 权威基准

- **`docs/提案书.md` §8（612–619 行）**：明确「文字上可挂 URI：断行后为行上落在该段文字上的矩形写入 PDF 链接注解。库只写入 URI 字符串，不去访问、不校验 scheme 是否可打开。空 URI 在类型/构造期拒绝。第一版范围：正文（及页眉页脚若同管线）里的外链 URI。文内跳转、仅图可点等后补」。
- **`docs/提案书.md` §2.0**：非空 URI 作为强类型 newtype 拦截，空串非法状态不可表示。
- **Seam 复用**：完全复用下游已完备的 `LinkTarget`、`Canvas::annotate_link`、`RecordingCanvas` 以及 `backend-krilla` 的 `QueuedLink` 机制，绝不发明平行的注解管线，绝不在 `backend/` 之外触碰 `krilla`。

### 1.2 自治默认项（Autonomy Defaults）核准

Candidate 4 **完全采纳** `frame.md` 中定义的所有自治默认项，**无任何 reverse word**：

| 自治项 | 采纳的默认项 | 状态 | 简析 |
|---|---|---|---|
| **DSL shape** | `p.link(uri, \|inner\| { … })` | **保持默认**（未采用 `chain-last`） | 作用域明确，天然支持混合 bold/body，与 §1 闭包风格统一。 |
| **Band URI** | `Band::{left,right}_link(uri, text)` | **保持默认**（未采用 `band-paragraph`） | 页眉真实需求是槽级标题/跳转，避免在带中引入复杂富文本造成膨胀。 |
| **Blank under link** | blanks stay unlinkable | **保持默认**（未采用 `blank-is-linked`） | blank 无 ink，属于排版填空下划线，作为测量切分边界。 |
| **Link visual style** | no underline/color change | **保持默认**（未采用 `auto-style`） | 提案书未要求自动修饰，避免与调用方设计的版面冲突。 |
| **Hit rect height** | full `line_height` | **保持默认**（未采用 `ink-bbox`） | 覆盖整个行盒顶到底，提供标准的悬停与可点击区域。 |
| **Adjacent same-URI runs** | coalesce when face & URI match | **保持默认**（未采用 `no-coalesce`） | 组装期合并相邻相同文本，防止碎片化。 |
| **Zero-width / empty text** | omit annotate | **保持默认**（未采用 `error-on-empty`） | 沿用现有 `text("")` 吞入规则，不构造非法零面积 `Rect`。 |
| **Layer `text_at` link** | out this knife | **保持默认**（未采用 `layer-in`） | `text_at` 走 `single_line_run`，不在流式正文管线中。 |
| **Cell string link** | out this knife | **保持默认**（未采用 `cell-link-in`） | 表格单元格富文本超出了本刀的正文+页眉范围。 |

---

## 2. 深入剖析：DSL 错误传播与签名设计

在设计 `Paragraph::link` 与 `Band::left_link` 时，一个核心的设计抉择是：**方法应该返回 `Result<()>` 还是像 `Paragraph::text` / `Paragraph::blank_to_end` 一样返回 `()` 并延期通过 `self.error` 报告？**

### 2.1 为什么拒绝在闭包内部返回 `Result<()>`？

观察现有 `src/document.rs` 的宏观闭包设计：
```rust
pub fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph))
```
闭包 `f` 的类型是 `FnOnce(&mut Paragraph) -> ()`，**不返回 `Result`**。类似地：
- `type BandHook<'a> = Box<dyn Fn(&mut Band) + 'a>;`
- `Section::header` 接收 `Fn(&mut Band) -> ()`。

如果设计成：
```rust
pub fn link(&mut self, uri: &str, f: impl FnOnce(&mut Paragraph)) -> Result<()>
```
当调用方写出如下代码时：
```rust
doc.paragraph(|p| {
    p.text("Click ");
    p.link("https://example.com", |p| {
        p.text("here");
    })?; // ❌ 编译失败！
    p.text(" to view.");
});
```
Rust 编译器会产生致命编译错误：
> `error[E0277]: the '?' operator can only be used in a closure that returns 'Result' or 'Option' (or another type that implements 'FromResidual')`  
> `the trait 'FromResidual<Result<Infallible, Error>>' is not implemented for '()'`

为了让这行编译通过，调用方必须被迫写出怪异的代码：
- 要么忽略返回值：`let _ = p.link(...);`（产生 `unused Result` 警告，或掩盖错误）；
- 要么在闭包外定义 `let mut link_res = Ok(());` 然后在外侧匹配；
- 要么彻底打破现有的 `Flow::paragraph` 签名，强行将其改为 `paragraph<R>(&mut self, f: impl FnOnce(&mut Paragraph) -> Result<R>)`，导致整个代码库已有测试与用例全部遭受破坏。

### 2.2 quire 的既有规范：构建器内部错误记录（Deferred Builder Error）

事实上，quire 内部有一套成熟、统一、且优雅的闭包错误处理模式：
1. **`Paragraph` 密封违规**：`blank_to_end()` 密封段落后，后续若调用 `p.text("foo")`，`text` 方法返回 `()`，但会在内部设置：
   ```rust
   if self.error.is_none() {
       self.error = Some(Error::RunAfterFillBlank);
   }
   ```
   随后在 `Flow::paragraph` 结束时：
   ```rust
   if let Some(error) = paragraph.error {
       self.push(Err(error));
       return;
   }
   ```
2. **`Layer` 未知图片**：`Layer::image_at` 遇到非法图片时不返回 `Result`，而是设置 `self.error = Some(...)`，最后在 `Layer::lower` 时统一返回 `Err`。
3. **`Shapes` 路径翻转**：`Layer::shape` 闭包中的非法变换直接记录在 `Layer.error` 中。

### 2.3 Candidate 4 的解决方案：`IntoLinkTarget` + 内部记录

Candidate 4 严格遵守这一设计模式：
1. `Paragraph::link` 与 `Band::left_link` 均通过 `target: impl IntoLinkTarget` 接收参数。
2. 当传入 `&str` 且为空串时，`into_link_target()` 返回 `Err(Error::EmptyLink)`，构建器捕获该错误并存入 `self.error`，同时提前跳过闭包逻辑。
3. 若传入的是外部已经 `LinkTarget::uri(s)?` 校验过的 `LinkTarget`，则在类型系统层面 100% 保证非空。
4. `Flow::paragraph` 与 `Band::lower` 在退出时检查 `error`，将错误向上传导给 `Document`，导致渲染在 `start_page` 之前直接终止。

这既保证了「空 URI 在构造期/DSL 边界即被拒绝」，又完全吻合 Rust 闭包的签名限制，给调用方带来最符合直觉的书写体验。

---

## 3. 被拒绝的备选方案（Alternatives Rejected）

### 3.1 备选 1：链式调用 `chain-last`（如 `p.text("abc").link("url")`）
- **方案**：让 `p.text` 返回一个类似 `Blank` 的句柄 `Text<'a>`，在其上提供 `.link(uri)` 方法绑定最后添加的 run。
- **拒绝理由**：
  1. 破坏混合样式支持：如果链接是一段包含 Body 与 Bold 混排的文本（如 `Click **here** now`），链式句柄极难直观表达多段文字共享同一外链；
  2. 易碎的内部索引：`Paragraph` 内部频繁发生 `push_str` 合并，返回针对末尾 run 的句柄容易与后续合并逻辑产生生命周期冲突；
  3. 违反自治默认项：frame.md 明确指出默认项为 `builder-closure`。

### 3.2 备选 2：Band 槽位接受完整富文本 `band-paragraph`
- **方案**：让 `Band::left` / `Band::right` 接受 `impl FnOnce(&mut Paragraph)` 闭包。
- **拒绝理由**：
  1. 范围膨胀：页眉和页脚在真实生产环境中的定位是单行槽位（公司名称、章节名、URL、页码）；
  2. 测量违规风险：Band 的可用高度被严格限定在 `margins.top` 与 `margins.bottom` 之间，且超出即报 `DecorationTallerThanMargin` 失败。引入段落的复杂换行和 blank 会极大增加排版出错面；
  3. 简单整槽 `Band::left_link(url, text)` 已经能 100% 满足 §8「页眉页脚若同管线」的要求。

### 3.3 备选 3：在 `cluster_tagged` 中将 URI 存入每个标量 tag
- **方案**：给 `cluster_tagged<T: Copy>` 增加一个标量级的 tag 索引（例如 `u16`），指向一个并行的 URI 查找表。
- **拒绝理由**：
  1. 过度复杂化：`LinkTarget` 包含堆分配的 `String`，不是 `Copy`，不能直接作为标量 tag；
  2. 间接查找代价：为了让其符合 `Copy`，必须维护一张段落级的 URI side-table并在断行后重建映射；
  3. 破坏 Seam：现有机制中，`prepare_inline` 已经具备在 `Blank` 边界 flush 测量组的成熟逻辑。**在 link 变更处 flush 文本组**，使得每个 `MeasuredInline` 自带一个统一的 `Option<LinkTarget>`，断行器输出的 `Seg::Glyphs` 天然保持链接属性统一，改动最小且零额外间接层。

### 3.4 备选 4：在 `pack_lines` / 断行器中计算命中矩形
- **方案**：在断行器输出 `SpanLine` 时直接生成 `Rect`。
- **拒绝理由**：
  1. 信息缺失：断行阶段仅根据累加 advance 进行贪心断行，该阶段根本不知道该行的首行缩进 `frame_x`、段落文本对齐偏移 `align_x`（居中或靠右对齐）、以及 fill blank 拉伸后的剩余空间；
  2. y 坐标缺失：断行器根本没有行高 `line_height` 与行盒基线数据；
  3. 只有在 `place_line` 里，所有水平排布全部就绪；在 `fragment_from_lines` 里，所有垂直叠加全部就绪。

### 3.5 备选 5：事后在 `draw_fragment` 中根据 `GlyphRun` 反推链接范围
- **方案**：不增加 `FragmentCommand::Link`，仅在 `draw_fragment` 阶段遍历 `GlyphRun` 并猜测其外链矩形。
- **拒绝理由**：
  1. 割裂字体段与链接段：`glyph_runs_for_line` 会在字体改变时（如 Regular 到 Bold）切分出多个 `GlyphRun`。如果在 `GlyphRun` 级绑定链接，一个 Bold/Body 混排的链接会在同一视觉行上被分裂成多个狭窄的命中矩形，直接违反 rubric #3（"Bold/body inside one link stays one rect per line"）；
  2. 信息丢失：`GlyphRun` 是底层画布绘制结构，不包含也不应包含逻辑外链数据。

### 3.6 备选 6：自动为链接添加下划线与蓝色修饰（`auto-style`）
- **方案**：当检测到文字带外链时，自动发出下划线 `Stroke` 并强制改写文字颜色为 `#0000EE`。
- **拒绝理由**：
  1. 违反提案书边界：提案书 §8 只明确要求「断行后为行上落在该段文字上的矩形写入 PDF 链接注解」，未要求任何排版视觉改写；
  2. 侵害调用方设计自由度：专业 PDF 文档中，页眉链接或标题链接通常保持纯黑或与主题色一致。自动修饰会导致调用方无法定制视觉外观；
  3. 符合 `no-decoration` 默认项。

### 3.7 备选 7：文内跳转（`linkToPage` / 书签跳转）与图片链接
- **方案**：在本次修改中一并实现指向文档某页的 `Target::Destination` 或给 `Image` 增加 URI 属性。
- **拒绝理由**：
  1. 提案书 §8 明确声明：「文内跳转（链到某页/锚点）、仅图可点等可后补；不阻塞本条」；
  2. 保持 Knife 15 最小 diff 与高内聚，避免 scope creep。

---

## 4. 极端边界条件分析与防御

### 4.1 空白字符与零宽边界
- **行首丢弃的空格**：断行器在 `pack_lines` 阶段会直接丢弃行首的 ASCII 空格（`Atom::Ink { pair: PairClass::Sp }`），它们绝不会进入 `Seg::Glyphs`，因此行首空格自然不会产生链接矩形；
- **零宽文本与空 Seg**：若链接内的文字计算所得 advance 为 0，`place_line` 与 `fragment_from_lines` 均显式守卫 `advance > 0.0 && advance.is_finite()`，跳过该命中矩形的生成，防止构造出非法 `Rect`（`geom.rs` 严禁宽高非正）。

### 4.2 跨页段落重排安全
- 当一个很长的多行段落跨越页面时，`ParagraphNode::split` 仅切分整行 `LineBox`，不会触发重新断行；
- 切分出的 tail 片段在进入下一页调用 `fragment_from_lines` 时，行索引 `index` 重新从 0 计数，因此片段局部的 `y0` 重新对齐到 `0, line_height, 2 * line_height ...`；
- 随后由父级 `plan_pages` 将片段 origin 设为下一页的 `(margin.left, margin.top)`，并通过 `draw_fragment` 叠加。整个过程中链接矩形与文字 glyph 完全使用同一套变换体系，绝对不会产生跨页错位。

---

## 5. 总结

Candidate 4 实现了：
1. **最小切入点（Minimal Seam）**：两处合并键增强 + `prepare_inline` link flush + `LinePiece::Link` + `FragmentCommand::Link`；
2. **零编译隐患的 DSL**：基于 `IntoLinkTarget` 与构建器延迟错误机制，100% 适配已有的 `paragraph` 与 `BandHook` 闭包，彻底规避 `?` 编译失败；
3. **严格遵循全部自治默认项**：无任何反转，行为确定，可测性强，完全忠于提案书 §8 与 §2.0。
