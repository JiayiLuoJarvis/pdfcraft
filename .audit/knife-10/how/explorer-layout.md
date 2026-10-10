### Components Found

文档组装的入口在 `src/document.rs`，分页与装饰在 `src/layout/paginate.rs` 与 `src/layout/decorate.rs`。纸张与边距类型在 `src/geom.rs`。画布按页接收尺寸；后端按页写 PDF 页设置。

- **`document` / `Document::paint`**（`src/document.rs:39-53`，`147-177`）
  - `document(page_size: Size, build)` 建一份 `Document`，闭包只跑一遍，把正文推进 `Flow`。
  - `paint` 把正文收成 `FlowNode::Stack`，把 header/footer/behind/front 各包成一个闭包，配上**一份** `PaginateConfig { page_size, margins }`，调用 `paginate_root`。
  - `Document` 只有一个 `page_size: Size` 和一个 `margins: Margins`（`59-71`，`123-125`）。默认边距是四边 0（`geom.rs:305-362`，`Margins::default` = `uniform(ZERO)`）。
  - 没有 `section` 方法，没有 `PageSize` 枚举，没有方向枚举。预置纸只有 `geom::A4` 与 `geom::LETTER`（`geom.rs:251-255`）。`document` 的参数就是已经算好的 `Size`。

- **`PaginateConfig`**（`src/layout/paginate.rs:11-38`）
  - 字段：`page_size: Size`，`margins: Margins`。注释：页眉页脚带落在 margin 里面。
  - `body_width` = 页宽 − left − right。`body_height` = 页高 − top − bottom。`body_origin` = `(left, top)`。
  - `validate`：正文宽或高非有限或 `<= 0` → `Error::InvalidGeometry`。边距把纸吃光会在 `plan_pages` 开头失败，画布不动。

- **`PageCursor` / `PagePlan`**（`paginate.rs:41-52`）
  - 游标三态：`Fresh`（还没开出带正文的页）、`HasBody`（当前页已放入正文）、`PendingBlank`（刚因 `PageBreak` 提交了上一页，下一页还没建）。
  - `PagePlan` 只有 `size: Size` 和 `placed: Vec<PlacedFragment>`。没有 section id、没有每页 margin、没有每页装饰。

- **`PageInfo` / `PageNumber` / `PageCount`**（`decorate.rs:16-43`）
  - `PageNumber(NonZeroU32)`：全文从 1 起的物理页序号。`PageCount(NonZeroU32)`：计划页数，空白正文页算进去。
  - `PageInfo` 字段：`number`，`count`，`size`，`margins`。**没有** `section_number` / `section_count`。
  - `BandContent` = `Fn(&PageInfo) -> Result<Vec<FlowNode>>`。`LayerContent` = `Fn(&PageInfo) -> Result<Vec<LayerItem>>`。

- **`Decorations` / `PageBands`**（`decorate.rs:68-88`）
  - 一份文档一套：`header` / `footer` / `behind` / `front` 各一个 `Option`。
  - `PageBands` 是测完的四层，每层 `Option<PlacedFragment>`。正文不在这里，画的时候再插进去。

- **公共 `Page`**（`document.rs:671-709`）
  - 字段：`number`，`count`，`section_number`，`section_count`，`size`，`margins`。`#[non_exhaustive]`。
  - 文档注释：文档是一个隐式 section，所以 section 计数等于全文计数。
  - `From<&PageInfo> for Page` 是**唯一**给 `section_*` 赋值的地方：`section_number: info.number.get()`，`section_count: info.count.get()`（`699-708`）。
  - `Page::is_last` 比较的是全文 `number == count`（`691-696`）。`Layer::is_last` 转调它（`807-811`）。

- **DSL `Band` 缩进**（`document.rs:712-777`）与 **layout 带**是两层。
  - Layout 带横跨整页宽，原点 x = 0。
  - `Band::lower` 才在左右 margin 上插入定宽空栏，让 `left` / `right` 槽与正文对齐。

- **画布 / 后端已经按页吃尺寸**
  - `Canvas::start_page(&mut self, size: Size)`（`canvas.rs:187-188`）。`RecordingCanvas` 把 `StartPage(Size)` 记进日志；`RecordedPage.size` 来自对应的 `StartPage`（`canvas.rs:213-224`，`261-293`，`312-314`）。
  - `KrillaCanvas::start_page` 把 `size` 放进当前 `OpenPage`；`finish_page` 推进 `closed_pages`（`krilla_canvas.rs:220-248`）。
  - `emit_page` 用**这一页**的 `page_plan.size` 调 `PageSettings::from_wh`（`emit.rs:89-98`）。

### Flow

1. **组装（测量之前）**  
   `document` 闭包写正文节点和四条页面闭包（`document.rs:39-44`，`128-145`）。正文方法经 `Deref` 到 `Flow`（`275-287`）。`page_break` 只是 `FlowNode::PageBreak`（`405-407`）。此阶段不断行、不分页、不跑页眉闭包。

2. **`paint` → `paginate_root`**（`document.rs:147-171`，`paginate.rs:54-68`）  
   顺序固定：
   - `plan_pages` 只排正文，得到 `Vec<PagePlan>`。
   - 抽出 `sizes: Vec<Size>`。
   - `measure_bands(&sizes, config.margins, decorations, ctx)`。
   - `paint_pages`。  
   注释写明：带在正文计划之后测量，装饰不能把正文挪走（`paginate.rs:54-56`）。

3. **`plan_pages`**（`paginate.rs:70-153`）  
   - `config.validate()`。
   - 根若是 `FlowNode::Stack`，拆开子节点；否则包成单元素列表（`76-79`）。`Document::paint` 总是包一层 `Stack`（`document.rs:162`）。
   - 正文框只算一次：`body_w` / `body_h` / `body_origin` 来自那一份 `PaginateConfig`（`81-83`）。循环里每一页都用这组数。
   - 游标起点 `Fresh`，`pages` 空，`current = None`，`y = 0`。
   - 遇到 `FlowNode::PageBreak`：走 `apply_page_break`，`y` 归零。不调用 `FlowNode::split`。
   - 其它节点：`ensure_content_page` 在 `current` 为空时新建 `PagePlan { size: config.page_size, placed: [] }`（`155-161`）。可用框是 `Frame { width: body_w, height: max(body_h - y, 0), full_height: body_h }`（`99-103`）。
   - `Split::Fit`：片段原点 `(body_origin.x, body_origin.y + y)`，`y += height`。`spacing_after > 0` 且剩余高度够，才把段后距加进 `y`；不够就丢掉，不推到下页（`105-118`）。游标变 `HasBody`。
   - `Split::Partial`：头放进当前页，`commit_page`，尾巴插回队列头，`y = 0`，游标 `Fresh`（`120-136`）。
   - `Split::MoveToNextPage`：若 `y == 0`（本页还没放过东西）→ `Error::UnsplittableTallerThanFrame`，画布仍空。否则提交当前页，尾巴插回，`y = 0`，游标 `Fresh`（`137-145`）。
   - 流结束走 `finalize_eof`（`151`，`192-214`）。

4. **空白页游标**（`apply_page_break` `164-184`，`finalize_eof` `192-214`）

   | 状态 + 事件 | 结果 |
   | --- | --- |
   | `Fresh` + `PageBreak` | 仍是 `Fresh`。开头的 break 是 no-op，不建页。 |
   | `HasBody` + `PageBreak` | `commit_page` 当前页，变成 `PendingBlank`。挂起页还没分配 `PagePlan`。 |
   | `PendingBlank` + `PageBreak` | 直接 `pages.push` 一页 `placed` 为空、`size = config.page_size`。仍是 `PendingBlank`。 |
   | EOF + `Fresh` 且 `pages` 空 | 推一页空正文。空文档、或只有开头 break，都是这一页。 |
   | EOF + `Fresh` 且 `pages` 非空 | 不再补页（上一次 Partial / Move 已经提交）。 |
   | EOF + `HasBody` | `commit_page`。 |
   | EOF + `PendingBlank` | `current.take()` 丢掉。挂起的那一页从未创建，所以「内容 + 一个 break」不会多一页空白。 |

   空白页仍在 `pages` 里，后面照样编号、照样画装饰。`FlowNode::PageBreak` 若掉进 `split`（嵌在内层 stack / 栏 / keep-together 里，没被 `plan_pages` 的顶层 match 截住）会 `Error::InvalidGeometry`（`flow.rs:147-150`）。`plan_pages` 只剥最外一层 `Stack`。

5. **`measure_bands`：页码在这里填**（`decorate.rs:91-137`）  
   - `count = NonZeroU32::new(sizes.len() as u32)`，失败 → `InvalidGeometry`（`97-101`）。`plan_pages` 至少返回一页，所以正常路径 count ≥ 1。空白页算在 `sizes.len()` 里。
   - 对 `(1u32..).zip(sizes)`：`PageInfo { number: PageNumber(number), count, size: *size, margins }`。`margins` 是函数参数，**每一页同一份**（`103-109`）。
   - header：`measure_band`，放置原点 `(0, 0)`，带高比较值 = `margins.top`（`127`）。
   - footer：原点 `(0, page_h - bottom)`，带高比较值 = `margins.bottom`（`128`）。`page_h` 用的是**这一页**的 `size.height()`。
   - behind / front：`measure_layer`，原点 `(0, 0)`（`130-133`，`170-176`）。
   - 闭包在这一步执行。`band_content` / `layer_content` 用 `Page::from(info)` 把 `PageInfo` 变成公共 `Page`（`document.rs:779-788`，`841-855`）。section 计数的别名发生在这次转换里，发生在画页之前。
   - 失败则 `paginate_root` 不调用 `paint_pages`。测试断言 `RecordingCanvas` 命令为空。

6. **带高 / 带宽检查**（`measure_band`，`decorate.rs:180-207`）  
   - 节点空 → `Ok(None)`，不占一层。
   - `check_content(Container::Band, &nodes)`（`lower.rs:30-64`）。带内允许：text、paragraph、spacer、stack、columns（递归检查栏内）。拒绝：table、list、image、page break、keep-together。错误是 `InvalidCellContent { container: "header/footer band", node }`。
   - 测量框：`Frame { width: page_width, height: page_height, full_height: page_height }`（`190-195`）。宽是**整页宽**，不是正文宽。左右 margin 不缩小这个框。高是整页高，不是 margin。
   - `Split::Fit` 且 `height <= margin` → 收下片段。
   - `Split::Fit` 但更高 → `DecorationTallerThanMargin { height, margin }`，`height` 是实测高度。
   - `Partial`、`MoveToNextPage`、或 `UnsplittableTallerThanFrame` → `DecorationTallerThanMargin { height: page_h, margin }`。这里的 `height` 是整页高，不是内容自然高（`200-201`）。
   - `UnbreakableWiderThanFrame { width, avail }` → `DecorationWiderThanFrame { width, avail }`。`avail` 是页宽（`202-204`）。
   - 其它错误原样返回。

7. **层测量**（`measure_layer`，`decorate.rs:139-178`）  
   - 文字：样式强制 `TextAlign::Start`，`single_line_run(..., f32::INFINITY)`，再把 `item.at` 加到 run 原点。对齐被丢掉，也不做断行。超宽在 `INFINITY` 下不会触发带宽错误。
   - `single_line_run` 把原点设成 `(0, baseline)`（`paragraph.rs:142-168`），所以画出的 y = `at.y + baseline`。
   - 图：`FragmentCommand::Image { image, rect }`，不按页裁、不按 margin 缩进（类型注释 `decorate.rs:57-63`）。
   - 闭包返回空列表 → `Ok(None)`。

8. **`paint_pages` 绘制顺序**（`paginate.rs:216-229`）  
   每一页：`canvas.start_page(page.size)`，然后按这条链画，每段 `draw_fragment(..., placed.origin)`：
   `behind` → `header` → `page.placed`（正文）→ `footer` → `front`。  
   `Option` 的 `iter` 在 `None` 时跳过。空白正文页的 `placed` 为空，四层装饰仍画。然后 `finish_page`。  
   `draw_fragment` 把片段内坐标加上 `origin`（`flow.rs:338-363`）。正文原点是 `(margin.left, margin.top + y)`；header 原点是 `(0, 0)`；footer 原点是 `(0, page_h - margin.bottom)`；层原点是 `(0, 0)`，文字自己的 `at` 已在测量时加进 glyph origin。

9. **DSL 槽缩进（layout 不缩进）**（`document.rs:740-776`）  
   `left` → `TextAlign::Start`，`right` → `TextAlign::End`。左右 margin 若 `> 0`，各插一条 `ColumnWidth::Fixed(margin)` 的空栏，夹住槽位，栏间距 0。只有一条轨道时直接返回那段文字，不再包 columns。  
   这发生在 `BandContent` 闭包里，喂给 `measure_band` 的仍是整页宽的框。缩进是内容里的空栏，不是带框变窄。

10. **section 计数今天在哪填**  
    分页完成后、画页之前，在 `measure_bands` 构造 `PageInfo` 时只有全文 `number` / `count`（`decorate.rs:103-109`）。公共字段在 `Page::from` 里抄这两份（`document.rs:699-708`）。`PagePlan` 上没有 section 边界，`measure_bands` 无法从现有计划算出「本段第几页 / 本段共几页」。单节文档上两者相等，是因为别名，不是因为有过 section 遍历。

### Files Read

- `src/layout/paginate.rs`（全文，含测试）
- `src/layout/decorate.rs`（全文，含测试）
- `src/layout/mod.rs`
- `src/layout/lower.rs`（`Container::Band` / `check_content`）
- `src/layout/flow.rs`（`PageBreak` 的 `split`、`split_stack`、`draw_fragment`）
- `src/layout/paragraph.rs`（`single_line_run` 原点）
- `src/layout/test_util.rs`（`render` / `page`）
- `src/document.rs`（`document`、`paint`、`Page`、`Band::lower`、`band_content`、`layer_content`）
- `src/geom.rs`（`A4`、`LETTER`、`Margins`）
- `src/canvas.rs`（`start_page`、`RecordedPage`、`RecordingCanvas::pages`）
- `src/backend/krilla_canvas.rs`（`start_page` / `finish_page` 保存每页 `Size`）
- `src/backend/emit.rs`（`emit_page` 用该页尺寸；`two_pages_with_different_sizes`）
- `src/error.rs`（`DecorationTallerThanMargin`、`DecorationWiderThanFrame`、`UnsplittableTallerThanFrame`）
- `tests/document.rs`（页眉槽缩进、section 计数别名、叠层顺序、`is_last`）
- `docs/提案书.md` §2.10–§2.12、§7（对照用：section 换纸 / 双页码尚未进 layout）

### Boundaries

- **页尺寸在计划列表里不能变。** 每个 `PagePlan.size` 都写成 `config.page_size`：开内容页（`paginate.rs:157-159`）、空白页（`177-179`）、EOF 补的空页（`201-203`）。`plan_pages` 的断行框也只用一开始算出的 `body_w` / `body_h` / `body_origin`。`PagePlan.size` 字段是按页存的，生产者从不写第二个值。
- **`measure_bands` 已经按页读 `size`，但不按页读 margin。** 页脚的 y、带框的宽高用 `sizes[i]`（`decorate.rs:110-128`，`180-195`）。`margins` 是一个参数，写进每一页的 `PageInfo.margins`。换 section 边距现有签名表达不了。
- **装饰闭包是文档级一份。** `Document` 上各一个 `Option`（`document.rs:65-68`）。`measure_bands` 对每一页调用同一组闭包。没有 per-page 或 per-section 的 header/footer/behind/front 切换。
- **画布和 krilla 已经支持一份 PDF 里不同页尺寸。** `start_page` 每次收一个 `Size`。`emit.rs:301-308` `two_pages_with_different_sizes` 先 `A4` 再 `LETTER`，只断言字节以 `%PDF` 开头。这条能力停在 canvas/backend，layout 没喂过不同尺寸。
- **纸张 API 停在 `Size`。** 没有 A3/A5、没有方向、没有 `PageSize` 枚举、没有 section 覆盖文档默认。提案书 §2.11 / §7 描述的是目标，不是当前代码。
- **section 计数没有独立存储。** 扩展点是分页之后、闭包运行之时。今天就是 `Page::from` 的两行别名。若要真段内页码，`PageInfo`（或 `PagePlan`）得在 `measure_bands` 之前带上 section 下标或段内总数；现有 `PagePlan { size, placed }` 不够。
- **带不能改正文框。** 正文框在 `plan_pages` 里由四边 margin 决定，与装饰内容无关。带超高是错误，不是把正文往下推。`body_coordinates_are_identical_with_and_without_decorations` 把这一点钉死。
- **页眉「缩进」不在 `decorate.rs`。** Layout 测试里侧边距 20 时，页眉文字 x 仍是 0，正文 x 是 20（`decorate.rs:249-272`）。槽位对齐正文是 `Band::lower` 的空栏（`document.rs:756-775`），集成测试 `header_slots_inset_by_side_margins_and_body_sits_below_the_band`（`tests/document.rs:187-197`，四边 36 pt）钉的是 DSL 行为。
- **`PageBreak` 只在根栈的直接子节点上生效。** 嵌套进会 `split` 的节点会变成 `InvalidGeometry`。带内 `check_content` 直接拒绝 page break。
- **层不做流式排版，不检查出页。** `text_at` 单行、对齐忽略、可画出页外（`decorate.rs:553-577`）。`image_at` 不裁切（`document.rs:822-825`）。

### Non-Obvious Things

- 空白正文页计入 `PageCount`，并且跑同一套装饰。`bands_span_the_page_and_number_every_page_including_blank_ones`：内容 + 两个 break + 内容 → 3 页；中间页文字只有 `"Page 2 / 3"` 和 `"F"`（`decorate.rs:249-272`）。分页侧 `content_break_break_content_is_three_pages` 断言中间页 `ops()` 为空（`paginate.rs:444-468`，那次没挂装饰）。
- 「内容 + 一个 break」在 EOF `PendingBlank` 时丢掉挂起页，仍然 1 页（`paginate.rs:380-395`，`210-212`）。连续两个 break 才提交一页空正文（`398-417`）。开头无论几个 break，EOF `Fresh` 且 `pages` 空，只出 1 页空白（`347-377`，`199-205`）。
- 页脚锚在带的**上沿**，不是页底。原点 y = `page_h - margin.bottom`（`decorate.rs:128`）。页高 400、下边距 30 时，`"F"` 的基线在 `370 + baseline`（`decorate.rs:268`）。带内文字矮于 margin 时，空隙在文字下面。页眉锚在 y = 0，空隙在页眉下面、正文上面。正文始终从 `margin.top` 起（`paginate.rs:27-28`，`660-685`）。
- 带测量框的高度是整页，不是 margin。装得下整页但高于 margin：报实测高度。连整页都 `Fit` 不了：报 `height: page_h`（`decorate.rs:196-201`）。零 margin 加一行页眉：`DecorationTallerThanMargin { height: line_height, margin: 0 }`，且在任何 `StartPage` 之前失败（`decorate.rs:316-337`）。
- 装饰失败与「高过正文框」的失败都不画页。`keep_together_taller_than_frame_errors` 断言 `rec.commands()` 为空（`paginate.rs:603-625`）。缺字在层里同样在 `measure_bands` 失败，画布为空（`decorate.rs:581-600`）。
- `paint_pages` 用 `zip`（`paginate.rs:217`）。长度不一致时多出来的页或带会被静默丢掉。当前两者等长，因为 `sizes` 来自 `pages`，`measure_bands` 一页一条 `PageBands`。
- 段后距放不进剩余正文高度就丢，不变成下页开头的空隙（`paginate.rs:112-117`，测试 `536-562`）。spacer 相反，可拆，余量可以是下页第一项（`472-500`）。
- 公共 `section_*` 与全文页码是同一次读取的两份拷贝。`footer_sees_section_counters_equal_to_document_totals`（`tests/document.rs:222-240`）两页正文得到 `"1/2"` 与 `"2/2"`。没有第二套计数器。
- `is_last` 是全文最后一页。`full_template` 的 `front` 只在最后一页画 `"SEAL"`（`tests/document.rs:156-159`，`166-184`）。单节时这和「本节最后一页」重合。
- 叠层顺序有两处测试钉死，顺序一致：`layers_paint_behind_header_body_footer_front`（`decorate.rs:524-549`）与集成测试 `header_numbers_every_page_and_layers_stack_behind_header_body_footer_front`（`tests/document.rs:166-184`，第 0 页文字序 `DRAFT`、`ACME`、页码、正文）。
- 带内两栏平分整页宽、间距 0：页宽 200 时 `"L"` 在 x=0、`"R"` 在 x=100（`decorate.rs:452-479`）。这是 layout 直接塞 `Columns`，不是 DSL 的 margin 空栏。栏内再套 table 仍被拒绝（`483-509`）。
- 层文字忽略 `TextAlign::Center`，x 等于调用方给的 `at.x`，即使 `at` 在页边附近（`decorate.rs:553-577`，x=190，页宽 200）。

### Open Questions

- 多节强制换页时，段界是新的流节点，还是复用 `PageBreak` 的 `PendingBlank`？现有游标会在 EOF 丢掉单个挂起页，并且不记录「这一页属于哪一节」。段末未满也要换页，和「一个 break 不留空页」不是同一条规则。
- 段内 `section_count` 要等该节页数已知。现在总数是 `sizes.len()` 一个数（`decorate.rs:97-101`）。按节计数需要在调用装饰闭包之前，先有每页的 section 跨度。`Page::from` 的别名（`document.rs:704-705`）是现成的填入点，但它只能看到 `PageInfo` 上已有的全文计数。
- 换纸若只改 `PagePlan.size` 而不改断行用的 `body_w` / `body_h`，正文框仍是文档第一份 `PaginateConfig`（`paginate.rs:81-83`）。`measure_bands` 会按新尺寸放页脚，正文却已按旧框排完。两处必须一起变，当前没有这个接口。
- 每节 margin / header / footer 今天没有槽位。`PaginateConfig.margins` 与 `Decorations` 都是文档级一份。闭包已经能读 `PageInfo.margins` 和 `Page.size`（`decorate.rs:38-43`），所以「当页四边」的数据形状在，生产者只有一份值。
- 带框高度用整页、失败时有时上报 `page_h`（`decorate.rs:200-201`）。多节若某节纸高不同，这个错误里的 `height` 会跟着该页的 `size` 走；现有测试只覆盖单页高。
- `Page::is_last` 没有「本节最后一页」。提案书 §2.12 说闭包可以用 `is_last` 决定画不画。当前实现是全文末页（`document.rs:691-696`）。多节是否要另给一个谓词，代码里没有字段。
