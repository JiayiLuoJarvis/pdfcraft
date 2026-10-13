# Knife 15 data shape (§8 text external URI)

**Authority.** `docs/提案书.md` §8, §2.0 (empty URI refused at construct).  
**Baseline.** `feat/inline-blank` @ `a232ce1`.  
**Already shipped (reuse).** `LinkTarget` newtype, `Error::EmptyLink`, `Canvas::annotate_link`, backend queue → `LinkAnnotation` / `LinkAction`.

## Domain object

A text URI link is not a new flow node. It is a **property on linked ink** that survives break and becomes one **hit rectangle per visual-line slice**.

```text
Assemble:
  TextRun { text, face, link: Option<LinkTarget> }
  InlineItem::Text(TextRun) | InlineItem::Blank(...)

After break (per visual line):
  LinePiece::Glyphs(GlyphRun)           # paint ink
  LinePiece::Stroke(InlineStroke)       # blank (knife 14)
  LinePiece::Link { rect, target }      # hit target; no ink of its own
     OR FragmentCommand::Link { rect, target } emitted beside Glyphs

Paint:
  FragmentCommand::Link { rect: Rect, target: LinkTarget }
    → Canvas::annotate_link(rect, target)
```

## Organizing structure

| Concern | Structure | Why |
| --- | --- | --- |
| Non-empty URI | `LinkTarget` (existing newtype) | Illegal empty URI unrepresentable after `uri()` |
| Optional link on a run | `Option<LinkTarget>` on `TextRun` | Link is a property, not a sibling node type |
| Assemble coalesce | same face **and** same `Option<LinkTarget>` | Link change must break merge or URI is lost |
| Measure groups | split text groups on link boundary (like blanks) | `Seg::Glyphs` has no payload; URI must live on the group / side tape |
| Blank vs text | keep `InlineItem` discriminant | Blank cannot carry a link this knife |
| Per-line hit boxes | `FragmentCommand::Link` (new variant) | Design draft name; paint one match arm + `with_offset` |
| Geometry | `Rect` in fragment-local space at `place_line` / `fragment_from_lines` | x after align; y0 = index * line_height; height = line_height; width = seg advance; omit zero width |

## Layout insertion point (from explorer)

Do not compute rects in `pack_lines` (no align / baseline / y0). Do not recover from `GlyphRun` after the fact (font splits ≠ link spans). Record x0/x1 while `place_line` walks segs, stack y in `fragment_from_lines`, translate like `Image` in `with_offset` / `draw_fragment` → `annotate_link`.

## Invariants (proposal)

1. Empty URI fails at `LinkTarget::uri` / DSL construct. Never reaches layout.
2. Library writes the URI string only. No fetch, no scheme check.
3. After break, each visual-line slice of linked text gets its own rectangle (line-box height × that slice's advance).
4. Body and header/footer if they share the paragraph → fragment → paint path.
5. Out of knife: `linkToPage` / destination, image-only clickable, named styles, canvas / qr / svg / toc.

## Header / footer (from explorer)

Measured: bands lower `left` / `right` strings into `TextNode::new`, then the same `split` → `fragment_from_lines` → `draw_fragment` path as body. A `FragmentCommand::Link` arm plus `with_offset` / `draw_fragment` wiring would paint band links once those `TextNode`s carry linked runs.

Caller gap: public `Band` has no `Paragraph` / inline API. Only wiring `Paragraph::link` leaves header/footer unable to attach a URI this knife unless `Band` also gains a way to set run-level links (or whole-slot URI). Arena must pick that product call.

`Layer::text_at` is a different single-line path. Out of §8 "same pipeline".

## Open product calls (arena / autonomy)

Defaults if proposal silent. Reverse word in parentheses.

| Call | Default |
| --- | --- |
| DSL shape | `p.link(uri, \|inner\| { inner.text(..); })` closing over URI on pushed text runs (`builder-closure`) |
| Alt | `p.text(...).link(uri)` chaining on last run (`chain-last`) |
| Band URI | whole-slot `Band::{left,right}_link(uri, text)` helper this knife; no rich paragraph in band (`band-slot-link`) |
| Blank under link | reject / ignore; blanks stay unlinkable (`blank-no-link`) |
| Link visual style | no underline/color change this knife (`no-decoration`) |
| Hit rect height | full line box (ascent+descent used for line) (`line-box-height`) |
| Adjacent same-URI runs | coalesce like same-face text when faces match (`coalesce-same-uri`) |
| Zero-width / empty text under link | ignore empty strings as today; no zero-area annotate (`skip-empty`) |
| Layer `text_at` link | out (`layer-out`) |
