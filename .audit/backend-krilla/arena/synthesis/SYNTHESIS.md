# Knife 3 backend-krilla — synthesized design

## Synthesis decision

**Base:** Candidate 2 (buffered page plans, lexical render at `finish`).

**Agreed with** [CROSS_JUDGE.md](CROSS_JUDGE.md): C2 29/30; C1 26; C3 24.

**Grafts**

| From | Take | Skip |
| --- | --- | --- |
| C1 | `Error::InvalidGeometry` for brand-unreachable path failures (lone `MoveTo`); `#[ignore] dump_experiment_pdf` lever; no `InvalidPageGeometry` | Early convert-to-krilla-at-Canvas-call; `UnknownFont`/`UnknownImage` as "not implemented"; `default = []` |
| C3 | Split `emit.rs` so plan/session code imports no krilla; trap comments (`surface()` twice clears page; tagging must be off; push/pop balance) | `Ok(())` for glyphs/images/lifecycle; private `EmitError` instead of `crate::Error`; single probe serialize profile as production |

**Hard rejects**

- C3 silent `Ok(())` on deferred draws or bad page order.
- C3 pretty/uncompressed as the only serialize profile.
- C1 `UnknownFont`/`UnknownImage` for "this knife does not implement".
- Live `Document`/`Page`/`Surface` on `KrillaCanvas`.

## Shape (implement this)

### Modules

```text
src/error.rs              + InvalidCanvasState, UnsupportedCanvasOperation, InvalidGeometry, Backend(String)
                          drop Copy on Error
src/backend/mod.rs        convert, krilla_canvas, emit
src/backend/convert.rs    identity to_krilla_*; paint/path helpers used by emit
src/backend/krilla_canvas.rs
                          KrillaCanvas + PagePlan/OpenPage/Mark/Link + Canvas impl
                          NO `use krilla::...`
src/backend/emit.rs       only module that constructs Document/Page/Surface
Cargo.toml                default = ["backend-krilla"]; krilla =0.8.2 optional
.github/workflows/ci.yml  base --no-default-features + backend --features backend-krilla
```

### Data

`KrillaCanvas` holds `info`, `closed_pages: Vec<PagePlan>`, `open_page: Option<OpenPage>`, `failure: Option<Error>`, `serialization`.

`Mark` = Fill | Stroke (crate types). `Link` = Rect + LinkTarget. Links separate from marks.

`finish(self)` refuses if open page or prior failure; else `emit::render(...)`.

Poison: first `Err` stored; later calls return it; `finish` returns it. Glyph/image → `UnsupportedCanvasOperation` after open-page check.

### Errors (this knife only)

```rust
InvalidCanvasState { operation: &'static str, state: &'static str },
UnsupportedCanvasOperation { operation: &'static str },
InvalidGeometry,
Backend(String),
```

### Cargo / CI

`default = ["backend-krilla"]`, `backend-krilla = ["dep:krilla"]`.
CI: fmt once; clippy/test/doc for `--no-default-features` and `--features backend-krilla`.

### Tests

Under `src/backend/` `#[cfg(test)]`:

1. `%PDF` smoke
2. identity convert bit/equality
3. observed path operands (fill after first green run; guess from C3: `10 180` etc. on 200pt page — replace with observed)
4. observed Annot rect for public `(10, 30, 80, 44)`
5. fill then stroke: pens do not leak
6. two pages different sizes
7. InvalidCanvasState cases
8. UnsupportedCanvasOperation poisons finish
9. `#[ignore] dump_experiment_pdf` lever

### Out of scope

Fonts, images, DSL, `InvalidPageGeometry`, full design Error enum, `draw_glyphs`.
