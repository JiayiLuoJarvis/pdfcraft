# Candidate 2 rationale

## Problem

Knife 13 同时跨越两个轴：段落级样式要等实际局部框宽 `W` 才能落实首行缩进，而 `p.bold` 又必须在同一段落内保留 run 边界，并从 skrifa 测量起就选择另一个 face。现状是一段一个 `String`、一个 `Style.font`、一行一个 `GlyphRun`；简单增加 setter 会在 lowering 时丢掉 bold 边界，简单增加 draw flag 又会让测量与绘制宽度分叉。设计还必须保持缺字整份失败、所有错误早于 `start_page`，且不把未确认的 named styles、italics、blank 或 justify 顺带做进来。

## Usage (caller's view)

调用方只需注册 regular/bold 两个真实 face，再在同一个 paragraph builder 上写段落属性与 run：

```rust
let size = Positive::pt(12.0)?;
let leading = LineHeight::factor(1.5)?;
let indent = NonNegative::mm(7.4)?;

let rendered = document(PageSize::A4, |doc| {
    doc.font(REGULAR.to_vec())?;
    doc.bold_font(BOLD.to_vec())?;
    doc.paragraph(|p| {
        p.font_size(size);
        p.line_height(leading);
        p.first_line_indent(indent);
        p.text("合同编号 ");
        p.bold(&contract.id);
    });
    Ok(())
})?;
```

栏、列表项或 keep-together 中仍使用相同调用方式；indent 自动基于所在框的 `W`，而不是页面宽。Cell、Band、Layer 本 knife 仍是 plain regular text。

## Shape

`Paragraph` 在创建时拿到 `Flow.style` 的已解析快照，setter 直接修改快照，`text`/`bold` 累积私有 typed runs，最后仍 lower 成一个 `TextNode`。这避免引入第二套 `StyleOverride`/cascade，样式只有一个所有者，per single-source-of-truth。

`Document` 保存 regular/bold 两个显式字体槽；正常注册顺序得到 `FontId(0/1)`。run 只保存 `FontRole`，不让调用方或 document DSL 手写 id。布局时角色才 resolve；缺槽、缺字和字体解析失败沿现有 `Result` 返回，仍位于全局 plan/measure 阶段，per boundary-discipline。

所有 source runs 一次性进入 tagged cluster tape。CRLF 归一化、grapheme segmentation 和 BK 去除只发生一次，每个非 BK scalar 同时保留字体角色。每个 grapheme 仍是一个断行 atom，即使 run 边界切进 combining cluster。paragraph 把各 homogeneous role 交给现有单-face `measure_text`，再扁平化为唯一 measured glyph tape；断行定价和 GlyphRun 生成都消费该 tape，保证 charmap → advance → break → draw 同源。

`break_lines` 接收 `LineWidths { first: W-I, rest: W }`。首行 x 为 `I + align_x(W-I, ink)`，后续行为 `align_x(W, ink)`；完整 `W` 继续存入 `ParagraphNode`。断好的 `LineBox` 内含多个 homogeneous `GlyphRun`，分页只切 line boxes，不会在续页重施首行缩进。

混合 face 使用实际参与 face 的 metrics envelope，保留一个稳定的段落 line-height 和 baseline。它隐藏了不同 ascent/descent 的复杂度，同时避免把分页器扩大成可变行高重构。公开接口相对能力是深的：一个字体槽方法和 Paragraph 上的直接语义 setter，隐藏了 tagged normalization、多 face 测量、按行切 run、对齐及失败时序；调用方无需知道内部阶段。

刻意不做：公开 Style、任意 FontId run、named style map、inline font-size/color、italics、decoration、blank、flow canvas、justify，以及 Cell/Band/Layer rich text。

## Synthesis decision

（由 arena synthesis 填写。）

## Tradeoffs accepted

- 我们接受为 line-height 增加一个小型 `LineHeight` newtype，以换取非法 factor 在 DSL 边界不可进入 layout。
- 我们接受 mixed-face 段落所有行共享 metrics envelope，以换取不裁碰 glyph 且不扩张为可变行高分页。
- 我们接受 `Paragraph` builder 持有 crate-private resolved `Style`，以换取没有公开 override/cascade 和重复 merge 规则。
- 我们接受本 knife 的 bold 只有一个文档级 face 槽，以换取 `p.bold` 调用简单且绝不暗中 fallback/synthesize。
- 我们接受新增可复现的小型 bold 测试 fixture，以换取能验证真实 bold advance 参与断行，而非只验证 `FontId` 管线。
- 我们接受 Cell、Band、Layer 暂时不能混排，以换取不为一个正文段落需求扩散三套 builder。

## Alternatives considered

- **最小补丁：`p.bold` 直接写 `FontId(1)`，Paragraph 保存 `Vec<(bool, String)>`。** 公共面虽小，却把注册顺序、CRLF/run 映射和 metrics 规则泄漏到多个模块；下一种 run 属性会复制 bool 管线，接口深度较差。
- **公开 `Style`/`StyleOverride` + named style map。** 它能隐藏未来的 cascade，却要求调用方现在理解 style 名、override 优先级和 unknown-style 错误；这些能力不是 `p.bold` 或首行缩进的前置条件，属于浅而过宽的当前接口。
- **把 bold run 降成相邻 `TextNode`。** 实现最简单，但暴露块级断行、段后距和分页差异，无法满足调用方“一段混排”的语义，因此不可行。
- **给 Canvas 增加 `bold: bool` 或描边合成。** 它隐藏少量 backend 代码，却破坏真实 face 的 cmap/advance 与绘制同源，并违反 no synthetic bold。
- **每个视觉行独立选择可变 line-height。** 可容纳任意 inline size，但会扩散到 split、高度求和和分页测试；本 knife 所有 run 同字号，metrics envelope 已隐藏所需复杂度。

## Open questions and risks

- 能否在仓库许可与体积约束内加入一份可复现的 OFL bold subset，并让至少一个测试字符的 advance 与 regular 不同？
- 是否需要在实现前把 `Document::font` 的重复调用行为明确冻结，以免与“bold 必须紧随 regular 注册”的配置错误语义冲突？
- metrics envelope 遇到异常字体的负 leading 时，应保持现有原值公式，还是只取非负 leading，才能最忠实兼容当前 `line_metrics`？

## Next implementation step

先落 `LineHeight`、`FontSlots`、`Paragraph { style, runs }` 和 lowering 契约，并用 document 单元测试固定一个 paragraph 仍只生成一个 typed-run `TextNode`。
