# Rationale

## Problem

提案书 §1 的 `p.text` / `p.bold` 是同一段里的两个 run，§2.1 要求首行可用宽 `W' = W − indent`，§2.0 要求缩进是非负长度。当前 `Paragraph` 把闭包拼成一个 `String`，`TextNode` 只有一个 `FontId`，`break_lines` 每一行共用一个 `avail`，每个 `LineBox` 只有一个 `GlyphRun`。`Document::font` 把注册到的 id 丢掉，DSL 没有第二条 face 的绑定。粗体 advance 必须参与断行，缩进必须同时改断行宽和第一行的 x，而 `W` 要到 `TextNode::split` 才知道。单元格、页眉和 layer 文字不经过 `Paragraph`。具名样式和 `StyleOverride` 没有被提案书确认成这一刀的前置。

## Usage (caller's view)

调用方继续写 `document` 闭包。正文 face 用现有的 `doc.font`。粗体 face 用 `doc.bold_font`，并且写在至少一次成功的 `font` 之后。段落闭包里 `p.text` 与 `p.bold` 按出现顺序交替；`p.first_line_indent` 接受 `Length<NonNegative>`，默认是 0。下面三段就是这一刀的全部公共用法，类型草图从它们推导。

```rust
doc.font(regular_bytes)?;
doc.bold_font(bold_bytes)?;

doc.paragraph(|p| {
    p.text("合同编号 ");
    p.bold(&contract.id);
});

doc.paragraph(|p| {
    p.first_line_indent(NonNegative::pt(24.0)?);
    p.text("正文从第一行缩进，折行后顶到框的左缘。");
});

doc.ul(|list| {
    list.item(|flow| {
        flow.paragraph(|p| {
            p.text("条款 ");
            p.bold("甲");
        });
    });
});
```

返回值仍是 `document(...) -> Result<Rendered>`。闭包里 `?` 用来构造长度和注册字体。`p.bold` 与 `p.first_line_indent` 本身返回 `()`。

## Shape

建造器就是现有的 `Paragraph`。它私有地保存有序的 `TextRun { text, face: Body | Bold }` 和一份 `Length<NonNegative>` 缩进。降低时克隆当前 `Flow` 的已解析 `Style`，只把缩进写进这一份拷贝，再包成一个 `TextNode`。`Flow.style` 不被写回，所以缩进不会漏进兄弟段落、页眉或后建的栏。按 encode-lessons-in-structure，负缩进无法表示；按 boundary-discipline，`indent >= W` 这种要等框宽才知道的关系留在 `ParagraphNode::layout`，返回现有的 `InvalidGeometry`。

粗体槽只有 `FontStore` 能写。`Document::bold_font` 在 store 非空时 `register` 并 `set_bold`。空 store 时立刻返回 `Error::Font`，避免粗体字节占据正文约定的 `FontId(0)`。run 上存的是 `Face::Bold` 而不是提前解析的 id，因此 `bold_font` 可以写在段落闭包之后，只要写在 `document` 闭包返回、`paint` 开始排版之前。排版发现粗体 run 时调用一次 `FontStore::bold()`；槽空则 `UnknownFont { id: 1 }`。这是「约定的第二张 face 缺失」，发生在 `plan_pages` 内、`start_page` 之前。测量把该 id 交给现有的 `measure_text`，缺字自然是 `MissingGlyph { font: 该 id }`。Canvas 上的 `GlyphRun.font` 就是这张 face。没有合成粗体，也没有新的绘制指令。

断行仍消费一条 cluster 磁带和每个簇的 pt 宽。`break_lines` 的宽度参数改为 `LineBudget { first, rest }`：还没有 `LineRecord` 时用 `first`，其后用 `rest`。`W' = W − indent` 只在段落布局里算一次。溢出错误里的 `avail` 是当前这一行正在用的那个宽度。首行对齐框是 `[I, W]`：`x = I + align_x(W − I, line_width, align)`；其后各行 `x = align_x(W, line_width, align)`。这样 Start 落在 `I`，End 仍贴原框右缘，Center 在缩进后的区间里居中。x 写进 `LineBox` 之后分页只切片。`ParagraphNode.width` 继续保存完整 `W`。

同一行按连续相同 `FontId` 切成多个 `GlyphRun`，后一个的 `origin.x` 是前面 advance 的和。字号和颜色仍取段落 `Style`。行高只取 `style.font` 的 metrics，一段仍是统一的 `line_height`。字素簇若被 run 切开，整簇归第一个标量的 face，断行原子不被拆开。CRLF 规范化只走 `ClusteredText::new`；角色按原始字节对齐，跨 run 的 `\r` + `\n` 合成的 LF 属于持有 `\n` 的那个 run。

`single_line_run` 继续只用 `style.font`，并把原点 x 设为 0。列表标记和 layer 文字因此不吃首行缩进。单元格与页眉仍通过 `TextNode::new` 变成一个正文 run。

公共面增加三个方法：`Document::bold_font`、`Paragraph::bold`、`Paragraph::first_line_indent`。藏在后面的是粗体槽、run 合并、簇计价、两档行宽、多 run 原点。调用方不接触 `FontId`、`Style`、`LineBudget`。按 laziness-protocol，这个面刚好让 §1 模板和缩进排版成立。`font` 的返回类型保持 `Result<()>`，避免再给调用方一条「自己拿着 id 选字体」的路。

自治默认（提案书没写死的部分）：

- 粗体 face 由 `bold_font` 绑定；README 顺序是先 `font` 再 `bold_font`，此时 id 为 1。反向词：`bold-is-same-face`。
- 首行缩进默认 0，不自动二字。反向词：`default-two-em`。
- 具名样式 / `defaultStyle` 不进这一刀。反向词：`named-styles-in`。

## Synthesis decision

待 arena 合成填写。本文件是 candidate-3 的独立方案，不记录其他候选的取舍。

## Tradeoffs accepted

- 接受粗体 face 的 ascent 大于正文时字形可能伸出统一行盒，以换取分页仍用一个 `line_height`、不按行重算行高。反向若要改，用 `max-run-metrics`。
- 接受再次 `bold_font` 会在 store 里留下不再被槽指向的 face，以换取不新增「重复绑定」错误变体。槽的语义是最后一次成功注册。
- 接受未绑定粗体时报 `UnknownFont { id: 1 }` 这个约定 id，以换取不新增错误变体；id 1 表示「第二张 face 没绑上」，不是一次真实查找。
- 接受被切开的字素簇整簇跟随第一个标量的字体，以换取不新增错误、断行原子规则保持一条。
- 接受 Center / End 在首行使用区间 `[I, W]`。提案书只钉了 `W'`；这个解释写进测试，避免实现时出现第二种对齐。
- 接受单元格、页眉、`text_at` 这一刀没有粗体和首行缩进，以换取不把 `Paragraph` 建造器复制到三条字符串 API 上。列表项因为本来就是 `Flow`，自然可用。
- 接受同一 fixture 注册两次只能证明 `FontId` 与坐标，不能证明视觉粗细。真实粗体外观要另备 bold 字体文件，不在这一刀的正确性条件里。

## Alternatives considered

- 第二趟断行：先用 `W'` 切开第一行，再用 `W` 断剩余字符串。调用方表面一样小，但 cluster 游标、行首丢空格和硬断被拆成两套，宽度政策有两个主人。`LineBudget` 把两档宽度藏进现有的一次 `pack_lines`。
- 在 cluster 磁带开头插入宽度为 `I` 的假空格来收窄第一行。断行函数可以不改签名，但缩进泄漏成一种假字符，End / Center 的行宽会把 `I` 算进墨水，绘制还得再把这段假宽翻译回 x。段落布局同时拥有预算和原点，假字符就多余了。
- 公共 `StyleOverride` 或具名样式 map，再让 `bold` 成为一个可选字段。调用方要先构造覆盖、再挂到段落上，§1 的 `p.bold(&id)` 还得另开一条路，否则模板对不上。两种写法会同时留在仓库里。三个方法覆盖这一刀的全部调用。
- `font` 改为返回 `FontId`，由调用方把 id 传进 run。表面看起来更通用，实际把「哪张脸是粗体」推给每一个调用点，和 §1 的无参 `p.bold` 冲突。槽放在 `FontStore`，段落只写 `Face::Bold`。
- 每个 run 单独断行再拼接成段。接口小，但段后距、跨页和 `W` 会按 run 各算一次，混排行无法在 run 中间断开。统一磁带计价之后只断一次。
- `bold_font` 不检查 store 是否已有正文，靠注册顺序默认第二张是粗体。少一个前置条件，但先调用 `bold_font` 会把粗体字节安成 `FontId(0)`，正文样式却仍然指向 0。前置检查把这个顺序挡在注册边界。

## Open questions and risks

- 首行 Center / End 使用 `[I, W]` 是否就是要固定进测试的解释？提案书只写了 `W'`。
- 未绑定粗体用 `UnknownFont { id: 1 }` 是否可接受？若希望错误文本明确说「没有粗体槽」，需要新的 `Error` 变体。
- 再次 `bold_font` 用最后一次写入，旧 face 仍留在 store。若要第二次调用直接失败，也需要新的错误变体。
- 统一行高取正文 metrics。若样张里的粗体 ascent 明显更大，是否在这一刀就改成 `max-run-metrics`？
- 字素簇跨 run 时跟随第一个标量。若要在排版期直接拒绝这种切分，错误用现有的 `InvalidGeometry` 还是新变体？

## Next implementation step

先把 `break_lines` 改成接收 `LineBudget`，现有测试全部走 `LineBudget::uniform`，再让 `ParagraphNode::layout` 按 run 的 face 给簇计价。
