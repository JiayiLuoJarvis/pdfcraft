`Error` 仍是单条失败。公共 `Result<T>` 是 `Result<T, Error>`。没有 `Errors`。组装期用 `Option<Error>` 和 `Vec<Result<_>>` 的 `collect` 只留下第一条。

### Components Found

**公共类型（`src/error.rs`，crate 根再导出）**

`src/lib.rs` 10–13 行从 `error` 再导出：`Error`、`ErrorKind`、`Location`、`Region`、`CellIndex`、`GeometryKind`、`ColumnWidthsReason`、`CellSpanReason`、`Result`。模块本身是 `pub mod error`（`src/lib.rs` 7）。

`Result` 只有这一个别名（`src/error.rs` 589–590）：

```589:590:src/error.rs
/// Result of constructing a value or drawing to a canvas.
pub type Result<T> = std::result::Result<T, Error>;
```

`Error`（12–25，67–105）：

- `pub struct`，`Debug + Clone`。手写 `PartialEq` 只比 `kind` 与 `at`，不比 `source`（83–87）。没有 `Eq`。
- 公开字段：`kind: ErrorKind`，`at: Option<Location>`。
- `source: Option<Arc<dyn StdError + Send + Sync>>` 私有。`std::error::Error::source` 在 99–104 返回它。
- 公开转换：`From<ErrorKind>`（67–75）、`From<GeometryKind>` → `InvalidGeometry`（77–81）。
- 定位方法全是 `pub(crate)`：`with_source`（29）、`in_section`（35，已有 section 不覆盖）、`on_page_index`（43，0 起索引写成 1 起页码，溢出则不写）、`in_region`（51）、`in_cell`（57，已有 cell 不覆盖）。
- `Display`（89–96）打印 kind，location 非默认时追加 ` (…)`，不拼 source。

`ErrorKind`（115–237）`#[non_exhaustive]`，`Debug + Clone + PartialEq`，无 `Eq`（含 `f32`）。变体：`NonFiniteLength`、`NegativeLength`、`NonPositiveLength`、`EmptyLink`、`InvalidCanvasState`、`InvalidGeometry(GeometryKind)`、`Font { index, message }`、`MissingGlyph { font: u32, scalar }`、`UnknownFont { id: u32 }`、`BoldFaceRequired`、`FontMetricsMismatch`、`UnbreakableWiderThanFrame`、`RunAfterFillBlank`、`UnsplittableTallerThanFrame`、`ColumnWidths { reason }`、`InvalidCellContent`、`InvalidCellSpan(CellSpanReason)`、`DecorationTallerThanMargin`、`DecorationWiderThanFrame`、`Image { message }`、`UnknownImage`、`ContentOutsideSection`、`Backend { message }`。

伴随枚举同样 `#[non_exhaustive]`：`GeometryKind`（298–388）、`ColumnWidthsReason`（453–472）、`CellSpanReason`（493–502）、`Region`（555–566）。`Location`（521–531）`#[non_exhaustive]`，字段 `section/page/region/cell` 全公开。`CellIndex`（582–587）不是 `non_exhaustive`。

没有 `Errors`、`Vec<Error>`、`thiserror`、`anyhow`。`error.rs` 的 import 只有 `std`（8–10）。`MissingGlyph.font` 是 `u32`，文档指向 `FontId::as_u32`（153），类型上不依赖 `fonts`。

**入口签名（仍是单错 `Result`）**

- `document(...) -> Result<Rendered>`，闭包是 `FnOnce(&mut Document) -> Result<()>`（`src/lib.rs` 45–64）。文档写明延迟错误只交第一条（47–48）。
- `to_pdf(rendered) -> Result<Vec<u8>>`（67–69），`backend-krilla` 才有。
- 调用方直接返回 `Result` 的公开方法：`Document::load_image` / `font` / `bold_font` / `section`（`document_ty.rs` 31、40、52、118）；`LinkTarget::uri`（`canvas.rs` 40–44）；`geom` 的 `pt`/`mm`/`try_*`。
- `examples/bill.rs` 372–395 用 `quire_pdf::Result`，并用 `Error::from(ErrorKind::Font { ... })` 包 `std::io::Error` 的 `Display`（不调用 `with_source`，source 为 `None`）。

**内部：`Option<Error>` 槽（最多一条）**

| 持有者 | 位置 | 写入 |
| --- | --- | --- |
| `Document` | `document_ty.rs` 11 | `section` 闭包失败（137–140）；`fail_outside` 仅在 `is_none` 时写 `ContentOutsideSection`（155–158） |
| `Flow` | `flow.rs` 11 | `push` 已有错则直接 return（41–43）；否则记下第一条 `Err`（49–51）；`reject_pushes` 写成 `ContentOutsideSection`（45–47） |
| `Paragraph` | `paragraph.rs` 13 | `note` 仅 `is_none`（164–167） |
| `Table` | `table.rs` 41 | `note`（167–170）；`Cell::store` 把 `cell.error` 送进同一槽（109–111） |
| `Cell` | `table.rs` 250 | 只有 `Cell::spacer` 失败时 `Some`（349–356），一格一条 |
| `List` | `list.rs` 26 | `note`（84–87）；另外 `items: Vec<Result<ListItem>>`（22） |
| `ImageBox` | `image.rs` 37 | `note`（78–81） |
| `HLine` | `hline.rs` 18 | `note`（90–93） |
| `Band` | `band.rs` 20 | `note`（93–96） |
| `Layer` | `layer.rs` 10 | `note`（157–160）；`push_shape` 再判一次 `is_none`（170–173） |
| `TextProps` | `style_props.rs` 9 | `font_size` 失败时 `is_none` 才写入（17–20） |
| `KrillaCanvas.failure` | `krilla_canvas.rs` 103 | `fail` 只在 `is_none` 时保存，之后每次都返回已存的那条（187–198） |

`Columns` 没有 `error` 字段。失败推进 `cols: Vec<Result<(ColumnWidth, Vec<FlowNode>)>>`（`columns.rs` 7、26、42、51）。

`note` 实现共 7 处，语义相同：`table.rs` 167、`list.rs` 84、`paragraph.rs` 164、`image.rs` 78、`band.rs` 93、`layer.rs` 157、`hline.rs` 90。

**`Vec<Result<_>>` + `collect::<Result<_>>()`（短路在第一条 `Err`）**

- `document/columns.rs` 65
- `document/list.rs` 99（先被 `self.error` 短路，91–93）
- `document/table.rs` 195（`Track::into_width`）
- `layout/paragraph.rs` 554–561（每个粗体字形 tag 上 `BoldFaceRequired`）
- `geom.rs` 878–894（`Path::map_points`）

`std` 的 `collect` 遇到第一条 `Err` 就停，后面的 `Ok`/`Err` 丢弃。

### Flow

组装期 builder 方法大多返回 `()`，错误写进槽，不立刻从 `document()` 冒出。`paint`（`document_ty.rs` 161–203）顺序是：

1. `Document.error` 有值则立刻 `return Err`（170–172），不再 lower 各节。
2. 显式节时 `closed.into_nodes()?`（175–176）。拒绝流上的 `ContentOutsideSection` 在这里出来。
3. 每节 `section.lower`，错误 `in_section(index)`（183–186）。`section.rs` 88–114：`flow.into_nodes()?` 先于页眉页脚闭包；闭包要到分页时才跑。
4. `paginate_sections`。几何/分页用 `?` 立刻返回，不继续画页。`paginate.rs` 196–201 的 `StalledOnFreshPage` 是这种硬停。

`Flow::push`（`flow.rs` 41–52）在 `error.is_some()` 时连节点和后续错误都不看。`IntoFlowItem::add_to`（`flow_writer.rs` 22–92）一律 `flow.push(lower())`。因此 `columns` 失败之后的 `table` 根本不会 lower。

容器 `lower` 先交 `self.error`，子项错误排在后面，后写下的容器错会盖住更早的子项错：

- `Table::lower` 174–176，然后才 `lower_rows`（`?` 在单元格之间也是第一条停）。
- `List::lower` 91–99：`self.error` 优先于 `items.collect()`。
- `ImageBox::lower` 85–92：宽度错优先于 `UnknownImage`。
- `HLine::lower` 97–98、`Band::lower` 100–101、`TextProps::apply` 31–32。
- `layer_content`（`layer.rs` 282–285）和 `band_content`（`band.rs` 148–160）把槽变成单个 `Err`。

`Document::section`（118–140）在已有 `self.error` 时不跑新闭包；闭包 `Err` 会 `in_section` 后立刻返回，节不入列。

测量期同样第一条即停：

- `measure_text`（`text/measure.rs` 60–66）第一个缺 cmap 的标量就 `return Err(MissingGlyph)`。
- `measure_inline`（`layout/paragraph.rs` 546–561）先 `fonts.bold()?`（`fonts.rs` 141–142，`BoldFaceRequired`），再按字形 tag `collect`，再对每个同字体片段 `measure_text?`（575）。
- `fonts::face` / `face_mut`（147–154）未知 id → `UnknownFont`。

分页装饰：`decorate.rs` 176–185 对 header/footer/behind/front 用 `?`。`measure_band`（277–309）把 `UnsplittableTallerThanFrame` / `UnbreakableWiderThanFrame` 改写成 `DecorationTallerThanMargin` / `DecorationWiderThanFrame`，其它错误原样上抛。一条带错就不再量后面的带。

Backend / `to_pdf` 也是单错，并且画布被毒化：`KrillaCanvas::fail` 留下第一条（`krilla_canvas.rs` 187–191），`finish` 先交 `failure`（175–176）。`emit.rs` 把 krilla 失败收成 `Backend`/`Font` 并 `with_source`。这和刀 4 第 2 点「`to_pdf` / backend 仍单错」一致；若公共别名改成 `Result<T, Errors>`，类型仍会变，条数应仍为 1。

闭包被跳过、刀 4 第 6 点允许保持的路径：

- 空 URI：`Paragraph::link` 113–115、`Band::set_link` 77–79，不跑闭包。
- `Layer::rotated` 非有限角度 138–140，不跑 `draw`。
- `Columns::col_fixed` / `col_ratio` 失败只 `cols.push(Err)`，不跑列闭包（39–51）。`gap` 同样（23–27）。
- `FlowWriter::stack` 样式失败则不跑 `body`（`flow_writer.rs` 221–226）。`Layer::stack` 相同（34–37）。

同一次调用里两个 `Err` 只留一个：`Band::inset`（35–40）和 `Layer::line` / `Shapes::line`（93–97、219–223）的 `(Err(error), _) | (_, Err(error))`。

### Files Read

- `src/error.rs`（全文）
- `src/lib.rs`（全文）
- `docs/error-diagnosability.md`（全文；刀 4 为 46–88 行，要点 1–8）
- `docs/module-dependency.md`（1–39、70、83）
- `docs/code-quality-assessment.md`（87–106）
- `CHANGELOG.md`（20–28）
- `src/document/document_ty.rs`（1–216）
- `src/document/flow.rs`（全文）
- `src/document/flow_writer.rs`（180–230，以及 `add_to`）
- `src/document/paragraph.rs`、`list.rs`、`columns.rs`、`table.rs`（槽、`note`、`lower`、`Cell`）
- `src/document/band.rs`、`layer.rs`、`image.rs`、`hline.rs`、`style_props.rs`、`section.rs`（88–114）
- `src/document/tests.rs`（1–541 中与错误相关的测试）
- `src/text/measure.rs`（41–153）
- `src/layout/paragraph.rs`（539–587）
- `src/layout/decorate.rs`（277–310、713–838）
- `src/layout/paginate.rs`（170–214）
- `src/fonts.rs`（69–154）
- `src/backend/krilla_canvas.rs`（96–209）
- `src/geom.rs`（878–894）
- `src/canvas.rs`（36–116）
- `examples/bill.rs`（372–395）
- `tests/document.rs` 中所有对 `.kind` / `.err()` / `unwrap_err()` 的断言（见下）

### Boundaries

- `error` 不引用 `document`、`layout`、`fonts`、`geom`、`krilla`、`skrifa`。与 `module-dependency.md` 20、36 行一致。`FontId` 不进 `ErrorKind`。
- 定位填充在 document/layout：`in_section` / `on_page_index` / `in_region` / `in_cell`。`error` 不知道这些调用点。
- 刀 4 第 2 点把收集范围划在组装（`note` / `Flow::push(Err)` / `Vec<Result>` collect）和测量（至少 `MissingGlyph`、`BoldFaceRequired`、`UnknownFont`）。几何/分页可记入后硬停。`to_pdf` / backend 不纳入「一次跑多错」。
- 刀 4 第 8 点：不加源码行号、不引入 `thiserror`/`anyhow`、不泄漏 krilla/skrifa 类型、不并行收集、不产出部分页。这些在今天的公共面上本来就没有。
- 公共 `Result` 一旦改成 `Result<T, Errors>`，所有 `crate::error::Result` 一起变，包括 backend、geom、字体注册。单条构造要靠已确认的 `From<Error> for Errors`。今天的 `return Err(Error::from(...))` 不会自动转换。

### Non-Obvious Things

- 今天没有「多错收集了一半」的类型。最接近的是 `List.items` 和 `Columns.cols`：向量里可以有多条 `Err`，`lower` 的 `collect` 或更早的 `self.error` 只交出第一条。
- 优先级是反的，和刀 4 第 5 点要改的一致：`List`/`Table`/`ImageBox`/`Document::paint` 先返回容器槽，槽往往比子项更晚写入。例：先 `item` 失败，再 `marker_width` 失败，`List::lower` 返回的是 `marker_width` 的错，`items` 里更早的 `Err` 被丢掉。没有测试锁住这个顺序。
- `Flow::written`（`flow.rs` 33–34）把 `error.is_some()` 当成「已经写过」。隐式节因此算 written，随后 `Document::section` 会改报 `ContentOutsideSection`（`document_ty.rs` 122–126），盖住流里更早的错。也没有测试。
- `fill_blank` 的最后一组是 `blank_to_end` 两次（`document/tests.rs` 127–129）。`note` 只留第一条，断言仍是 `Some(单条 RunAfterFillBlank)`（137–140）。刀 4 若每次 `note` 都追加，这里会变成两条相同 kind。
- `first_builder_error_wins_and_later_calls_are_ignored`（`tests/document.rs` 568–584）是唯一按名字锁定「第一条胜、后面忽略」的集成测试：一列 `columns`（`ColumnWidthsReason::ColumnCount`）之后还有行列数不一致的 `table`。`Flow::push` 在 41–43 行丢掉 table。刀 4 要求已有错时仍 lower 兄弟并收入同一桶，这个断言的「只有 ColumnCount」会不成立。
- `Paragraph::link("")` 不跑闭包，由 `empty_uri_does_not_run_the_link_closure`（`document/tests.rs` 268–276）锁住。刀 4 第 6 点允许继续跳过，这条行为可以不变，但 `assert_fails_with_empty_canvas` 仍把 `Result` 的 `Err` 当成 `&Error`。
- `measure_text` 对多字缺字只报第一个。现有测试只用一个标量（`measure.rs` 145–152；`document/tests.rs` 的 `MISSING`）。行为要变，但没有测试在断言「第二个缺字不出现」。
- `decorate.rs` 的 `band_error_returns_before_any_page_starts`（817–837）用 `assert_eq!(result, Err(Error::from(EmptyLink).in_region(...)))`，把整个 `Result` 和单条 `Error` 比较。
- `Error` 的定位方法不是公开 API。调用方只能读 `kind`、`at`、`Display`、`source()`。`at` 在组装期拒绝单元格内容时仍是 `None`（`error-diagnosability.md` 44）。`to_pdf` 序列化失败同样经常没有 location。
- `PartialEq` 忽略 source，所以 `document/tests.rs` 211 行 `assert_eq!(result.err().as_ref(), Some(expected))` 可以不重建 skrifa/krilla 错误。`Errors` 的按条 `PartialEq` 若同样忽略 source，这条助手只要改成比列表。

### Open Questions

- `Errors` 的类型名、截断标记放在类型上还是 `Display` 里，文档写「名称实现时定」（刀 4 第 3、7 点）。今天没有占位类型。
- 公共别名改成 `Result<T, Errors>` 之后，backend 是「`Errors` 里恰好一条」，还是内部仍用 `Result<T, Error>` 只在 `to_pdf` 边界包一层。文档只说公共 `Result` 和 backend 不承诺多错。
- `Document::section` 在已有错时跳过后续闭包（`document_ty.rs` 119–120）。这是 `Result` 早退，不是 `note`。刀 4 第 2 点列的是 `note` / `push` / `collect`，没写 `section()` 是否也要跑完后续闭包。
- 容器槽压过更早子项（`List::lower` 91–93 等）没有对应测试。刀 4 要纠正它，但现有测试不会因此变红，除非新测试或 `first_builder_error_wins` 那种兄弟收集。
- 同一 `Band::inset` / `line` 调用的两个非法长度今天只 `note` 一个。刀 4 没单独说这种元组匹配算不算「一次调用里的两条」。

**明确断言「只有一条 / `Option<Error>`」的测试**

- `tests/document.rs` `first_builder_error_wins_and_later_calls_are_ignored`（568–584）：只断言 `ColumnWidths { ColumnCount }`，后面的 table 被忽略。
- `src/document/tests.rs` `fill_blank_coalesces_text_before_it_and_rejects_a_later_run`（92–142）：`section.flow.error == Some(Error)`；其中 `blank_to_end` 两次仍是一条。
- `src/document/tests.rs` `assert_fails_with_empty_canvas`（208–212）：`result.err().as_ref() == Some(&Error)`。调用方：`run_after_fill_blank_paints_nothing`（146）、`empty_uri_does_not_run_the_link_closure`（268）、`empty_band_uri_paints_nothing`（280）、`bold_and_indent_failures_happen_before_any_page`（294，三处）、`missing_glyph_in_header_fails_before_any_page`（351）、`degenerate_layer_line_fails_before_any_page`（369）、`missing_glyph_in_front_fails_before_any_page`（403）、`section_after_paragraph_fails_before_any_page`（468）、`paragraph_after_section_fails_before_any_page`（486）、`add_after_section_fails_before_any_page`（498）、`margins_after_section_fail_before_any_page`（533）。
- `src/layout/decorate.rs` `band_error_returns_before_any_page_starts`（817–837）：`assert_eq!(result, Err(单条 Error))`。

**把 `Err` 当成单条 `Error`（`.kind` / `unwrap_err()` / `== Error`），别名一改就要改；文档里大多仍是「这一次只有一条失败」**

- `tests/document.rs`：`header_with_zero_margin_is_rejected`（383）、`custom_marker_wider_than_marker_width_errors`（435）、`nesting_rules_come_from_layout`（549）、`closure_errors_propagate_unchanged`（587）、`text_without_a_font_fails`（599）、`bold_and_indent_configuration_errors_are_specific`（659）、`load_image_without_decoder_is_an_image_error`（764）、`garbage_empty_and_foreign_handles_are_image_errors`（861）、`list_item_and_overwide_image_fail`（895）、`damaged_pixels_fail_at_pdf_time`（933，`to_pdf`）、`paragraph_then_section_is_content_outside_section`（1025）、`hline_taller_than_body_fails_before_any_page`（1356）、`cell_spacer_taller_than_body_fails_before_any_page`（1478）、`empty_uri_fails_the_document`（2171）、`cell_paragraph_errors_record_no_pages`（2335）、`cell_paragraph_indent_uses_cell_width`（2367）、`cell_rich_taller_than_frame_does_not_continue`（2399）、`cell_list_item_rejects_forbidden_kids`（2514）、`cell_list_narrower_than_marker_is_invalid_geometry`（2563）、`auto_with_ratio_fails_on_columns_and_table_tracks`（2744）、`table_tracks_length_must_match_the_first_row`（2777）、`inch_preset_on_a_narrow_page_is_invalid_geometry`（2831）。助手 `location(error: &Error)` 在 30 行。
- `src/document/tests.rs`：`bold_font_requires_body_and_rebinds_to_the_latest_face`（184，`unwrap_err() == Error`）、`bold_and_indent_failures` 里 overwide 的 `.kind`（340）、`non_finite_rotation_does_not_draw`（396）、`header_without_margin_fails_before_any_page`（431）、`keep_together_taller_than_the_page_fails_before_any_page`（445，还比 `error.at`）、`section_after_paragraph` 的 `unwrap_err()`（476）。
- 单元测试同样取单条：`src/error.rs` 596–642；`src/geom.rs` 中 `map_err(|error| error.kind)` 与 `unwrap_err().to_string()`（约 999–1197）；`src/text/measure.rs` 145–206；`src/fonts.rs` 190–246；`src/text/line_break.rs` 579–876；`src/canvas.rs` 430–548；`src/layout/node_tests/{table,columns,image,hline,list}.rs` 的 `unwrap_err().kind`；`src/layout/decorate.rs` 452、500、727；`src/layout/paginate.rs` 694；`src/backend/emit.rs` 322–520；`src/backend/images.rs` 124–155；`src/backend/convert.rs` 122。

刀 4 第 6 点写明可以继续跳过闭包、因此「只有一条」语义可以不变的测试：`empty_uri_does_not_run_the_link_closure`（不跑闭包）、`non_finite_rotation_does_not_draw`（不跑 `draw`）。它们仍依赖 `Err` 是单条 `Error`。