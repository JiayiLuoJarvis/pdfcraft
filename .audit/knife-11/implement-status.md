# Knife 11 implementation status

**Branch:** `feat/layer-path` @ `7cb1654`  
**Base tip before work:** `9eda8da`  
**Synthesis:** C1 base + C2 grafts (ellipse semantic asserts; zero-angle skip). Cross-judge 26–18.

## Files

Nine tracked files changed. `git diff --stat` reports 789 insertions and 30 deletions.

- `src/geom.rs` adds `Pen`, `Ink`, `PathVerb::CubicTo`, and `Path::{rect, ellipse, rotated}`. `translated` moves cubic control points too.
- `src/backend/convert.rs` maps `CubicTo` to krilla `PathBuilder::cubic_to`.
- `src/layout/flow.rs` adds `FragmentCommand::Fill` and the matching arms in `with_offset` and `draw_fragment`.
- `src/layout/list.rs` treats `Fill` like `Stroke` in `first_baseline`.
- `src/layout/decorate.rs` adds `LayerItem::Shape` and maps `Ink` in `measure_layer`. `FillStroke` pushes fill, then stroke.
- `src/layout/test_util.rs` teaches `strokes` about cubic end points, and adds `fills` plus `ellipse_center_radii`.
- `src/document.rs` adds `Layer::{line, rect, ellipse, rotated}`, `Shapes`, and `push_shape`.
- `src/lib.rs` re-exports `Ink`, `Pen`, and `Shapes`.
- `tests/document.rs` covers line, rect, ellipse, rotation, alpha, stack order, and a `%PDF` smoke test.

`canvas.rs`, `Mark`, and `emit` are unchanged. `table::rect_path` is unchanged. `text_at` and `image_at` are not rotated.

## Behavior that landed

Callers draw on `behind` and `front` with `line`, `rect`, and `ellipse`. `line` takes a `Pen`. Closed shapes take `Ink::{Fill, Stroke, FillStroke}`. `rotated` bakes a clockwise page rotation into path points. Angle `0.0` and `-0.0` skip that bake, so the vertices stay bit-identical. A degenerate line or a non-finite angle sets `Layer.error` to `Error::InvalidGeometry` and `paint` returns before `start_page`. A non-finite angle does not run the `Shapes` closure. Opacity is the existing `Color` alpha.

## Deviations

No public Layer signature change from synthesis.

Parent follow-up: `Path::rect` and `Path::ellipse` return `Result` (not `Self` + `expect`). Overflow of `origin + extent` becomes `Layer.error` via `push_shape` before `start_page`.

`strokes` and `fills` store a cubic's end point, not its control points. Kappa is pinned in `geom` tests. Layer tests recover center, `rx`, and `ry` from the four end points.

`tests/document.rs` has its own `ellipse_center_radii`. Integration tests cannot call the `pub(crate)` helper in `layout/test_util`. The two functions match the same six-verb shape.

## Gates

Parent re-ran all seven after the `Result` follow-up (2026-10-09). Exit code 0.

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --no-default-features -- -D warnings`
- `cargo clippy --all-targets --features backend-krilla -- -D warnings`
- `cargo test --no-default-features`. Library 130 passed. `tests/document.rs` 24 passed.
- `cargo test --features backend-krilla`. Library 153 passed + 1 ignored. `tests/document.rs` 32 passed.
- `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --no-default-features`
- `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --features backend-krilla`
