# Knife 9 implement status

Branch `feat/image` on top of `7cbe823`. One commit lands the image path. This file stays untracked.

## Result

All seven gates passed on 2026-10-09.

| Gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | ok |
| `cargo clippy --all-targets --no-default-features -- -D warnings` | ok |
| `cargo clippy --all-targets --features backend-krilla -- -D warnings` | ok |
| `cargo test --no-default-features` | 117 lib + 18 integration |
| `cargo test --features backend-krilla` | 139 lib + 1 ignored, 24 integration |
| `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --no-default-features` | ok |
| `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --features backend-krilla` | ok |

The baseline at `7cbe823` matches knife 8. 129 lib tests and 1 ignored with `backend-krilla`, 18 integration tests. This knife adds 10 lib tests and 6 integration tests under that feature. Without the feature the additions are 6 lib tests and 1 integration test (`load_image` reports `Error::Image`).

`rg "Image::from_"` under `src/` and `tests/` hits only `src/backend/images.rs`.

## What landed

`Document::load_image` decodes once into `ImageTable` and returns a `Copy` `Image` (`DecodedImage` plus a per-document token). `Flow::image` and `Cell::image` share `ImageBox::lower`. `Layer::image_at` paints a page rectangle. Layout holds `ImageNode` (`DecodedImage`, resolved `Size`, `TextAlign`). `FragmentCommand::Image` and `Mark::Image { raster, rect }` carry the draw. `finish` drops the table after each mark already owns its `Raster`. Emit translates, draws, and pops, with `interpolate` false.

Column and table cell allow an image. List item and header/footer band reject it. `spacing_after` is 0. An image wider than the frame is `UnbreakableWiderThanFrame`. Taller than the page body is `UnsplittableTallerThanFrame`. A short remainder moves the whole image.

Three pages painting the same 1×1 PNG produce one `/Subtype /Image`. That count is 1 in the observable backend test and in `Rendered::to_pdf` (compressed streams). A PNG with a valid header and a bad IDAT passes `document()` and fails `to_pdf` as `Error::Backend`. Empty bytes and a failed decode leave the table empty. The byte cap runs before `Image::from_*`.

## Deviations from the synthesis

- `draw_mark` maps the rectangle with `to_krilla_size` from `convert.rs` instead of calling `krilla::geom::Size::from_wh` inline. That helper is already the identity map. `None` is still `Error::InvalidGeometry`, and the check still happens before `push_transform`.
- `Raster`'s `Debug` text is `Raster({width}x{height})` with an ASCII `x`. The C1 sketch used a multiplication sign. The text is crate-private and is not a PDF field.

No public `Align`. `ImageBox::center` and `ImageBox::end` map to the existing `TextAlign`. Width uses a strict `>`. The 101 pt image in a 100 pt frame takes that path. No ulp case showed up.

## Public API

`ImageFormat`, `Image`, `ImageBox`, `Document::load_image`, `Flow::image`, `Cell::image`, `Layer::image_at`. `Error::Image` and `Error::UnknownImage`. `lib.rs` does not export `Align`. `publish = false` is unchanged.
