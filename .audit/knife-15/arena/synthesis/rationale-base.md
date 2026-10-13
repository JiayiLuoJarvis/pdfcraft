# Knife 15 取舍说明

## 结论

采用“链接是文字 run 的属性”：

- 调用方使用 `p.link(uri, |linked| { linked.text(...); linked.bold(...); })`。
- 页眉/页脚使用 `Band::{left,right}_link(uri, text)` 给整个槽位设置 URI。
- URI 以现有 `LinkTarget` 进入排版。
- assemble 同时按 face 和 link 合并；`prepare_inline` 在 link 变化时 flush。
- `place_line` 取得最终 x 区间，`fragment_from_lines` 配上行盒 y/height。
- `FragmentCommand::Link` 经过既有 fragment 平移和 paint 路径调用 `Canvas::annotate_link`。

这使链接只增加一条从文字属性到现有 Canvas 注解能力的通路，不改变断行规则、字形管线或 PDF backend 分层。

## 采用的 autonomy defaults

没有反转任何 autonomy default，因此没有 reverse word。

具体采用：

- 闭包式 `p.link(uri, |inner| ...)`，不采用 chain-last。
- Band 使用 whole-slot `left_link` / `right_link`，不把 Band 扩成富段落。
- blank 不带链接，并结束当前链接测量组。
- 不自动增加下划线或颜色。
- 命中高度使用完整 `line_height`。
- face 与 URI 都相同时相邻 run 合并；face 不同但 URI 相同时在测量组内保持连续。
- 空文字及零宽片段不产生注解。
- Layer、图片、Cell/table string 都不纳入本刀。

## 备选方案与拒绝原因

### 1. `p.text("...").link(uri)` / 修改最后一个 run

拒绝。现有 `text`、`bold` 返回 `()`，为 chain-last 改返回值会扩大公共 API diff；“最后一个 run”还会与现有同 face 合并、空字符串忽略、blank、`blank_to_end` 封口产生不直观语义。闭包天然圈定链接范围，也能让 body/bold 共享一个 URI。

### 2. 要求调用方先构造 `LinkTarget`

形如：

```rust
let target = LinkTarget::uri(uri)?;
p.link(target, |p| p.text("..."));
```

该方案在类型上很严格，但让最常见 DSL 多一个临时值，并把构造细节推给调用方。`Paragraph` 已有 builder 错误累计模式，内部调用现成 `LinkTarget::uri` 同样保证非法空 URI 不会进入 layout，而且能在文档组装阶段返回 `EmptyLink`。

### 3. 增加公开 `LinkedText` builder

只暴露 `text` / `bold` 的专用 builder 能限制闭包能力，但会增加一个只为单个方法存在的公开类型，并使 blank 在 link scope 中的语义无法自然表达。复用 `&mut Paragraph` 后，现有方法无需复制；临时 `active_link` 只影响文字 append，blank 仍明确不可链接。

代价是调用方可以在 link 闭包里设置段落级属性或嵌套 link。段落属性本来就是同一个 Paragraph 的属性；嵌套时采用内层临时覆盖、返回后恢复外层的普通 scope 语义，不需要新的节点或 layout 特例。

### 4. 把链接建成新的 `InlineItem::Link` 或流节点

拒绝。提案中的 link 是挂在文字上的属性，不是可独立排版的正文节点。新节点会重复文字测量/断行逻辑，或要求递归 inline tree；它也无法直接表达“同一链接内 body + bold 仍是一个视觉行命中区”。

### 5. 把 `LinkTarget` 塞进 `cluster_tagged` / `Span` payload

拒绝。现有 tagged cluster 和断行 span 的标签约束围绕 `Copy` 数据，而 `LinkTarget` 拥有 `String`、不是 `Copy`。为链接改造通用断行类型会扩大影响面。按 link 边界 flush，并把 `Option<LinkTarget>` 放在 `MeasuredInline` 旁边，既符合现有 blank 分组方式，又能通过 `Seg::Glyphs.group` 找回 URI。

### 6. 不在 `prepare_inline` flush，只给每个 glyph 标 link

拒绝。后续 `glyph_runs_for_line` 按 font 切分，不按链接切分；要从 glyph 标签重新拼 URI 区间，需要新增平行索引和合并算法，容易把 bold 边界误当成链接边界。link flush 后，每个 glyph seg 天然只有一个 target，断行仍可在组内正常切开。

### 7. 从最终 `GlyphRun` 推导命中矩形

拒绝。一个链接可跨 body/bold，因而对应多个 GlyphRun；相反，链接边界也不等于字体边界。若按 GlyphRun 发注解，会把一个视觉行上的连续链接拆成多个矩形，不符合“一段链接在每个视觉行一个矩形”的目标。`place_line` 遍历 `Seg::Glyphs` 时同时拥有链接组、最终对齐 x 和整个 seg advance，是正确的汇合点。

### 8. 在 `pack_lines` / 断行器里直接生成 Rect

拒绝。断行器不知道首行 `frame_x`、Start/Center/End 对齐、fragment 行 y、父容器偏移和页面 origin。在那里生成的只能是未放置宽度，称不上最终命中矩形，也会把排版几何职责泄漏到通用断行模块。

### 9. 用 glyph baseline 作为矩形顶边或仅包 glyph ink

拒绝。baseline 不是行盒顶边；从 baseline 开始会漏掉 ascent。第一版没有 glyph ink bounds 管线，按 ink bounds 还会造成同一行不同字形的可点击高度不一致。使用 `y0` 和完整 `line_height` 与排版行盒一致，稳定且可测试。

### 10. 在 `place_line` 或 paragraph paint 中直接调用 Canvas

拒绝。layout planning 发生在页面打开前，且 fragment 还会经过 stack、column、list、table、margin 和分页平移。提前注解既拿不到页坐标，也破坏“先规划全部页面和装饰、再 start_page”的失败原子性。`FragmentCommand::Link` 让链接和其他已定位命令共用 `with_offset` / `draw_fragment`。

### 11. 另建 annotation side tape，不加入 `FragmentCommand`

拒绝。每种容器目前通过 `Fragment.commands` 汇总和平移内容。平行 tape 要让 stack、columns、list、table、keep_together、分页片段都同步维护第二份集合，最容易漏掉 offset。新增一个穷尽匹配变体能让编译器暴露所有必须处理的位置。

### 12. 每个 linked run 一个矩形

拒绝。它会令一个 URI 内的 body/bold 在同一行产生多个矩形，也可能在 assemble 调用边界制造无意义切片。目标单位是“连续链接文字在一个视觉行上的 slice”：同 URI、无 blank 间隔时合并；换行才自然拆成下一矩形。

### 13. 跨 blank 合并同 URI 矩形

拒绝。blank 是可见下划线占位，不是 linked text，也没有 URI 属性。跨 blank 合并会让空白区域可点击，与“文字上可挂 URI”不符。blank 两侧即使 URI 相同也产生两个矩形。

### 14. 自动套用蓝色和下划线

拒绝。§8 只要求 PDF 链接注解，没有规定视觉样式。自动装饰会改变现有排版外观，还可能改变 stroke 命令和测试结果；链接与视觉样式应保持正交，日后若有 decoration 能力再由调用方显式选择。

### 15. 把 Band 槽位改成 Paragraph 闭包

拒绝。富 Band 槽会同时引入更大的公开 DSL、段落设置与槽位替换规则，超出外链所需。当前 Band 只有 whole-slot 字符串，增加 whole-slot URI 是覆盖页眉页脚同管线缺口的最小改动。若以后 Band 需要富文本，可再把 `BandSlot` 的内部表示迁移到 paragraph items，而不改变本刀的 annotation 管线。

### 16. 只做 Paragraph，不给 Band URI 入口

拒绝。layout 下半段虽然已让 header/footer 和正文共用 `TextNode::split`，但现有 Band 公共 API 只能提供纯字符串；只增加 `Paragraph::link` 会让 §8 的“页眉页脚若同管线”在调用方层面不可达。whole-slot helper 补齐该入口，成本小且边界明确。

### 17. 顺带支持 Cell、图片或 `Layer::text_at`

拒绝。Cell string 目前走 `Flow::text → TextNode::new`，图片和 Layer 又是独立路径；纳入它们会增加新的公开入口与产品语义。当前刀明确聚焦 Paragraph 与同文字排版管线的 Band。文内 destination、图片热区也不是 URI 文字注解的必要部分。

### 18. 修改 backend 或增加 PDF 字节断言

拒绝。Canvas、RecordingCanvas、KrillaCanvas 队列和 `emit_page` 的 LinkAnnotation/LinkAction 已完成。layout 只需调用现有 `annotate_link`。压缩 PDF 字节不是稳定的排版断言；行为几何由 RecordingCanvas 钉住，backend 仅保留 `%PDF` 冒烟和现有 emit 级探针。
