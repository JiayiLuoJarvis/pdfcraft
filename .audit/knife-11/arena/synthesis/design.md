# Knife 11 synthesis: Layer path primitives

**Base:** candidate-1 (cross-judge 26–18; parent agrees).  
**Grafts from candidate-2:** ellipse semantic test helper; skip rotate when angle is 0 (bit-exact).  
**Rejected grafts:** branded `Angle` (keep bare `f32` + runtime check); Canvas ellipse methods; `Figure`/`Layer::draw`.

Autonomy default for the open §2.12 question: **path-only rotation this knife.** Reverse word: `text-rotate` (would reopen Canvas transform).

## Synthesis decision

Ship C1's bake-geometry shape.

1. `PathVerb::CubicTo` + `Path::ellipse` / `Path::rect` / `Path::rotated` in geom.
2. `Pen` + `Ink::{Fill, Stroke, FillStroke}` public.
3. `Layer::{line, rect, ellipse, rotated}` and path-only `Shapes` scope.
4. `LayerItem::Shape { path, ink }` → `measure_layer` maps to `FragmentCommand::{Fill, Stroke}`.
5. Canvas / Mark / emit unchanged. Opacity stays on `Color` alpha.

Do not add `#[non_exhaustive]` on `PathVerb` this knife (match existing enum style). `0.x` + `publish = false`.

## Usage (caller)

```rust
use quire::{document, Color, Ink, PageSize, Pen, Point, Positive, Rect};

doc.behind(|layer| {
    let info = layer.page();
    let (w, h) = (info.size.width().as_pt(), info.size.height().as_pt());
    let faint = Pen::new(Color::rgba(200, 0, 0, 64), Positive::pt(0.8).unwrap());
    layer.line(Point::try_pt(0.0, 0.0).unwrap(), Point::try_pt(w, h).unwrap(), faint);
    layer.rect(
        Rect::try_pt(36.0, 36.0, w - 72.0, h - 72.0).unwrap(),
        Ink::Stroke(Pen::new(Color::rgb(0, 0, 0), Positive::pt(0.5).unwrap())),
    );
});

doc.front(|layer| {
    if !layer.is_last() {
        return;
    }
    let seal = Rect::try_pt(420.0, 660.0, 120.0, 120.0).unwrap();
    layer.ellipse(
        seal,
        Ink::FillStroke(
            Color::rgba(200, 0, 0, 40),
            Pen::new(Color::rgb(200, 0, 0), Positive::pt(2.0).unwrap()),
        ),
    );
    // Positive degrees = clockwise on the page (y down).
    layer.rotated(-15.0, Point::try_pt(480.0, 720.0).unwrap(), |r| {
        r.rect(
            Rect::try_pt(440.0, 710.0, 80.0, 20.0).unwrap(),
            Ink::Stroke(Pen::new(Color::rgb(200, 0, 0), Positive::pt(1.0).unwrap())),
        );
    });
});
```

## Shape (contract)

See candidate-1 `design.md` Shape section for full signatures. Implementation must honor:

| Rule | Where |
| --- | --- |
| Degenerate line / non-finite angle → `Layer.error` | Layer methods, first error wins |
| Angle `0.0` skips `Path::rotated` | `push_shape` |
| `FillStroke` → Fill then Stroke | `measure_layer` |
| Fail before `start_page` | existing `measure_bands` |
| Stack order unchanged | `paint_pages` |

## Graft details

1. **Ellipse assert helper** in `layout/test_util` (or decorate/document tests): recover center / rx / ry from MoveTo + four CubicTo endpoints. Layer/integration tests use that. Pin κ control points only in `geom` unit tests for `Path::ellipse`.
2. **Zero-angle fast path** preserves bit patterns for unrotated shapes.
3. No branded `Angle`.

## Tradeoffs accepted

- Path-only rotation (no slanted `text_at` this knife).
- RecordingCanvas sees baked vertices; rotated asserts use `pt_near`.
- Public `PathVerb::CubicTo` (breaking for external exhaustive matches; acceptable at `0.x`).

## Next implementation step

1. geom: `CubicTo`, `Pen`, `Ink`, `Path::{rect,ellipse,rotated}`, update `translated`.
2. backend `to_krilla_path` + geom unit tests.
3. `FragmentCommand::Fill` + list/test_util match arms.
4. `LayerItem::Shape` + `measure_layer` + Layer/Shapes API.
5. decorate + document integration tests; backend-krilla `%PDF` smoke if a path layer is easy to add.
6. Parent re-runs seven gates; update `.audit/handoff` + `implement-status`.
