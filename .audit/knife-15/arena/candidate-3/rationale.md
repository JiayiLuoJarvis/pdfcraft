# Knife 15 取舍

提案书 §8 只规定：文字可挂外部 URI、断行后每行一个矩形、只写字符串、空串在构造期拒绝、范围是正文以及同管线的页眉页脚。下面这些产品选择提案没有写死。本设计全部沿用 arena 的默认，**没有反转任何一条**。

未使用的反转词：`chain-last`、`band-paragraph`、`blank-no-link`、`no-decoration`、`line-box-height`、`coalesce-same-uri`、`skip-empty`、`layer-out`、`cell-link-in`。

| 默认 | 本设计 |
| --- | --- |
| `p.link(uri, \|inner\| …)` 把 URI 设到闭包里的 text/bold | `Paragraph::link` + 只有 `text`/`bold` 的 `Linked` |
| 页眉页脚整槽 `left_link` / `right_link` | `Slot { text, link }`，一个槽一个 `Option<LinkTarget>` |
| blank 不能带链接，blank 结束一组 | `Linked` 没有 blank；`prepare_inline` 遇 `Blank` 仍 flush |
| 不改下划线、不改颜色 | 不追加 `Stroke`，glyph 颜色仍是 `Style.color` |
| 命中矩形高度为行盒 `line_height` | `y0 = index * line_height`，高度为正文 `line_metrics` |
| 相邻且 Face 与 URI 都相同则合并 | `append` 的键是 `face` 与 `Option<LinkTarget>` |
| 空文本忽略，零宽不发注解 | `append` 仍丢弃空串；`advance` 非正就不推 `LinePiece::Link` |
| `Layer::text_at`、图片链接不做 | `single_line_run` 与 `ImageNode` 不动 |
| 单元格字符串本刀不做 | `Flow::text` / `TextNode::new` 仍是 `link: None` |

## 调用形状

**链式 `text(…).link(uri)`（`chain-last`）。** `text` 今天返回 `()`。改成返回句柄，只能标记最后一次 `text` 或 `bold`。Body 加 Bold 要打成一个矩形时，调用方得链式两次，合并规则会绑在“上一次返回值”上，空 URI 也会出现在文字已经推进 `items` 之后。闭包和 `paragraph` / `header` / `columns` 一样，是 §1 已经在用的写法：URI 的作用域就是闭包，空串在进入闭包前就被 `LinkTarget::uri` 拒绝。

**闭包收到 `&mut Paragraph`，用一个 `active_link` 字段。** 这样少一个公开类型，但闭包里能调用 `blank`、`blank_to_end`、`first_line_indent`，也能再调用 `link`。blank 带不带 URI 就变成运行时约定。`Linked` 借走 `Paragraph`，这些方法在闭包里写不出来。公开面只多一个两方法的类型，并从 `lib.rs` 与 `Blank` 并列导出。字段私有，调用方造不出 `Linked`。

**公开签名直接收 `LinkTarget`。** 空串在类型上更靠前，但 `Paragraph` 与 `Band` 的方法都不返回 `Result`，错误记在构建器里，到 `paint` 才失败。让调用方先 `LinkTarget::uri(...)?` 会把 `?` 引进 `document()` 闭包，和 `text` / `left` 的风格不一致。存储形态仍然是 `LinkTarget`：`append` 与 `Slot` 放不进空串。调用方已经能单独使用 `LinkTarget::uri`，DSL 不强迫他们绕一圈。

**嵌套链接。** `Linked` 没有 `link`。重叠区间要定“内层覆盖外层”还是“禁止”，§8 没有这张图。连续两次 `p.link` 就能换 URI。

**内层类型的名字。** 用 `Linked`，短，并且和 `Blank` 一样是构建期的临时柄。`LinkText` 更像一个节点名，链接并不是节点。

## 页眉页脚

**槽里放 `Paragraph`（`band-paragraph`）。** 测量上带已经走 `TextNode::split`。缺的是入口，不是第二套管线。整槽助手让 `Band::lower` 造出一个带 `link` 的 `Face::Body` run，后面的 flush、矩形、`with_offset`、`draw_fragment` 与正文共用。

把 `Paragraph` 放进槽会带上 bold、blank、首行缩进。带的内容矩阵今天只承诺字符串槽；bold 还会在测量期要 `bold_font`，行盒变高会撞上现成的 `DecorationTallerThanMargin`。这些都超出“页眉页脚若同管线”。整槽 URI 已经让钩子每页能写出不同字符串（页码可以拼进 URI）。富文本页眉可以以后再开。

**在 `document()` 闭包里提前解析带的 URI。** 钩子在 `measure_bands` 才运行，参数里有当页页码。提前解析看不到这一页的字符串。空 URI 仍在 `left_link` 构造 `LinkTarget` 时拒绝，并且发生在 `paint_pages` / `start_page` 之前。

**`left_link` 失败时清空另一槽。** `note` 只保留第一个错误，`band_content` 在 `lower` 之前返回它。失败路径不画页，槽里残留的内容不会变成 PDF。合法 URI 仍按“后写覆盖”更新本槽，与 `left` / `right` 今天的覆盖一致。

## 数据放在哪

**新的 `InlineItem::Link` 或设计稿里的 `Span::Link`。** 设计稿未实现，提案书也把链接写成文字上的属性。做成兄弟节点会和 `Text` / `Blank` 抢同一层，blank 与文字的判别也要重写。`Option<LinkTarget>` 放在 `TextRun` 上，`InlineItem` 的判别保持 `Text | Blank`。

**只改 `append` 的合并键，不在 `prepare_inline` flush。** `prepare_inline` 会把相邻的 Body 与 Bold 收成一个测量组。run 上的 URI 若在这里被丢掉，`Seg::Glyphs` 没有 payload，`place_line` 就分不清哪一段 advance 属于哪个 URI。

**Face 一变就 flush。** 那样同一次 `link` 里的 `text` + `bold` 会变成两个组、每行两个矩形。§8 的矩形盖的是“该段文字”，Face 只影响用哪一个字体画。flush 只看 `link`。同一组里 Bold 与 Body 仍由现有 `glyph_runs_for_line` 按 `FontId` 切开墨迹，命中矩形按整个 seg 的 advance 只发一条。

**给 `cluster_tagged` 加 URI 标签，或改 `Seg` 让它带 payload。** `cluster_tagged` 与 `Span` 都要求 `T: Copy`。`LinkTarget` 里面是 `String`。用 `Copy` 的下标去查一张表，才能在按字体切开之后再把 x 范围拼回来，等于把 blank 已经有的“边界”又实现一遍。组旁边放一个 `Option<LinkTarget>` 就够：换行切开的每个 `Seg::Glyphs` 用同一个 `group` 找回 URI。`line_break.rs` 与 `cluster.rs` 不用动，断点位置也不变。

**从 `GlyphRun.origin` 反推矩形。** 字体切分不等于链接切分。Body+Bold 是多个 origin、一个链接。反推会得到多条矩形，或者要再扫一遍 run 把相邻同 URI 粘起来。`place_line` 遍历 seg 时 `x` 已经是对齐、缩进、行内累加之后的值，当时记下 `x0`/`x1` 即可。

**在 `pack_lines` 里算矩形。** 那里没有 `align_x`、没有首行 `frame_x`、没有 `y0`。矩形的 y 要等 `fragment_from_lines` 知道这一行在片段里的序号。所以 `LinePiece` 只存 `x0`/`x1`/`target`，`Rect` 在叠上 `y0` 和 `line_height` 时才构造。

**把页坐标写进 `LineBox`。** tail 跨页时 `fragment_from_lines` 从 index 0 重算 y，父级还要 `with_offset`，`draw_fragment` 还要加 origin。页坐标会加两次。片段局部矩形只加一次。

**平行的注解通道，不进 `FragmentCommand`。** 栏、栈、列表缩进都只平移 `FragmentCommand`。链接若走旁路，双槽页眉和带左右 margin 的矩形会停在栏内坐标。`FragmentCommand::Link` 用与 `Image` 相同的 `rect.translated`。`list.rs` 的 `first_baseline` 必须把新变体当成“没有基线”，否则列表标记会对齐到一个不存在的字形 origin。

**在 `measure_band` 或 `paint_pages` 里特判页眉。** 带和正文在 `TextNode::split` 之后是同一条路。特判会把 URI 再实现一遍，也覆盖不了列表和栏里的正文链接。

## 几何与外观

**用 glyph origin 当矩形顶边。** origin 的 y 是 `y0 + ascent`。顶边若取 origin，行盒在基线以上的部分不在命中区内。高度取 `line_height` 之后，基线落在矩形内部；换行用例用两行 origin 的差来核对这个高度，不必把夹具字体的 ascent 写死。

**按墨迹 bbox 收紧矩形。** 测量管线只有 advance，没有字形 bbox。收紧要另读 skrifa 的轮廓，和“行上落在该段文字上的矩形”不是同一件事，也会让 Bold 的 bbox 改变高度。行盒高度与正文 face 的 `line_metrics` 一致，Bold 不参加行高。

**自动下划线或改成链接色（若把 `no-decoration` 理解成要推翻默认）。** §8 要的是 PDF 注解，不是一段新样式。颜色今天是整段 `Style`，没有 per-run 颜色。下划线会和填空的 `Stroke` 混在同一条命令里。无装饰用“去掉 `AnnotateLink` 后命令序列与无链接段落相同”来锁。

**跨 blank 把同一 URI 合成一个大矩形。** blank 是笔画，不是文字。合成的矩形会盖住下划线，也和“blank 结束一组”相反。两次 `link` 中间夹 `blank`，得到两枚矩形和一条没有 `Link` 的 `Stroke`。

**空文本配上合法 URI 就报错。** `append` 已经丢弃空串，而且发生在 `sealed` 检查之前。新错误会让 `text("")` 与 `link(..., text(""))` 行为不一致，也会让 `left("")` 与 `left_link(uri, "")` 分叉。空串不产生 run，也就没有零面积 `Rect`。`Rect::try_pt` 拒绝非正宽高，所以 advance 不大于 0 时直接不推 `LinePiece::Link`，而不是构造后再丢。

**行尾空格从矩形里裁掉。** 断行已经决定哪些空格留在 seg 里。矩形宽等于这段 `cluster_width_pt`。再裁一刀会让命中区短于已经画出来的 advance。行首 ASCII 空格本来就不进 seg，范围为空时 `continue`，自然没有矩形。

## 明确留在刀外

**单元格。** `Cell` 的字符串走 `Flow::text` → `TextNode::new`，没有 `Paragraph`，也没有粗体或行内分段。要链接就得给单元格另做一套行内 DSL。§8 的“正文”没有点名单元格；本刀把 `TextNode::new` 保持为无链接，并用同一文档里的单元格加层文字来断言注解条数不会多出来。

**`Layer::text_at` 与列表标记。** 它们走 `single_line_run`，不经 `prepare_inline`。§8 的同管线指的是页眉页脚那条 `TextNode` 路。层上的水印、电子章不在本刀挂 URI。

**图片链接、`linkToPage`、命名目标。** `LinkTarget` 只有一个 URI 字符串。`emit` 只写 `Target::Action(LinkAction)`。加目标枚举或 `ImageNode.link` 是另一条产品，不在 §8 这一句里。

**注解与 `front` 层的点击裁剪。** 绘制顺序是 behind → header → body → footer → front。注解在内容流之外，后画的层不会删掉已经 `annotate_link` 的矩形。提案没有定义“盖住就不可点”。本刀不在 layout 里做命中测试。

**另一条 PDF 后端，或在 `layout/` 里使用 krilla。** 注解队列和坐标映射已经在 `backend/`。本刀的绘制点是 `Canvas::annotate_link`。内容流里的 URI 字节已由 `emit` 的 observable probe 覆盖；文档级测试只对 `RecordingCanvas` 断言矩形，`to_pdf` 至多检查 `%PDF` 前缀。
