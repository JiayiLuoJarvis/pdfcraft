# Synthesis — quire-pdf slice 1 geom+error

## Base

**C2** (branded `Length<C>` + consuming `PathBuilder`).

Cross-judge score 12/12. Parent agrees after reading scores. C4's naming clarity loses to C2's smaller public surface and infallible aggregate constructors. C1 is the same lattice with weaker composition (`Result` on already-proven values).

## Grafts

| From | Keep |
| --- | --- |
| C1 | Optional: `pub(crate)` helpers `Rect::pt_xywh()`, `Margins::pt_trbl()` for later canvas/backend. Prefer one `geom.rs` file this slice (Laziness); split only if the file becomes unwieldy. |
| C4 | `Point::ZERO`. Diagnostic values stay on `Error` variants (already in C2). |

## Rejected

- C4 `Error::Geom(GeomError)`, `Rect::deflate`, named colors, `to_rgba_f32`, public dual getter sprawl
- C1 fallible constructors on already-typed aggregates; `Margins::try_new` always-Ok
- Zero-width `Stroke` as "no stroke"
- Empty / `LineTo`-first public `Path`

## Dropouts

- C3: empty after ~7m
- C4 first attempt: provider error; retry succeeded and was judged

## Implementation contract

Implement C2 `SKETCH.md` API in the repo. Gates:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps
```

Package name stays `quire-pdf`, `publish = false`. No new deps. No limits/DSL/SizeEnum/krilla.
