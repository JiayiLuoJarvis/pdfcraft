# Knife 14 data shape

**Feature.** §2.2 fill-in underscore (inline blank).
**Baseline.** `feat/style-builders` @ `5deb0df` (feature tip `aaf14bf`).
**Out of knife.** canvas / qr / svg / toc / named styles. §8 URI link and `Cell::spacer` are later knives.

## Domain structure

Paragraph content is an ordered sequence of inline items, not a flat `String` and not only `TextRun`.

```text
InlineItem =
  | Text { text, face: Body|Bold }     # existing
  | Blank { width, stroke, baseline_offset }
```

`Blank.width` is a closed enum (mutually exclusive):

```text
BlankWidth =
  | Fixed(Length<NonNegative>)         # or em-sugar → Fixed(N * font_size)
  | ToEndOfLine
```

Illegal states to reject at the boundary or assemble time:

- Fixed and ToEndOfLine both set
- Any run after a `ToEndOfLine` blank in the same paragraph
- Fixed blank wider than current frame `W` → `UnbreakableWiderThanFrame` at layout
- Overlay text on the blank (proposal: v1 no)

## Organizing choice

Discriminated union on the paragraph run list (`InlineItem` / rename of today's `TextRun` vector), not a parallel `FlowNode` and not a text `decoration: underline`. Blank width participates in line breaking as a non-glyph advance. Draw path emits one horizontal `Stroke` per placed blank; line height stays glyph-metric driven.

## Later (not this knife)

| Item | Why deferred |
| --- | --- |
| §8 external URI on text | Needs PDF link annotations; orthogonal to blank width |
| `Cell::spacer` | Table cell helper; flow `spacer` already exists |
| Named styles / italics / text underline decoration | Explicitly out |
