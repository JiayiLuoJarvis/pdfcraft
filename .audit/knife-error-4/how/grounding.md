# Knife 4 grounding (from how explorers)

## Today

- Public `Result<T> = Result<T, Error>` (`src/error.rs`). Single error only.
- Assembly stores `error: Option<Error>` with first-wins `note()`. `Flow::push` ignores further Ok/Err once set.
- `List`/`Columns` use `Vec<Result<_>>` then `.collect()` (first Err wins).
- Container `self.error` / `Document.error` returned before scanning child results, so later container mistakes bury earlier child mistakes.
- Measure/layout use `?`. First `MissingGlyph` / `BoldFaceRequired` / geometry stops the path. No continue-and-collect.
- `to_pdf` / backend keep a single failure. Out of knife 4 multi-collect promise.
- Any error means no `Rendered` / PDF.

## Confirmed knife 4 (docs/error-diagnosability.md)

1. Do this milestone.
2. Collect assembly fully. Soft measure at least `MissingGlyph` / `BoldFaceRequired` / `UnknownFont`. Geometry may record then hard-stop. Backend stays single-error.
3. `Error` stays singular. Non-empty `Errors`. Public `Result<T, Errors>`. `From<Error>`. Err is the list. No second API.
4. Any error blocks `Rendered`/PDF.
5. Cap 64 + truncated marker. Discovery order. Fix container/Document override of earlier children.
6. Builder closures finish after notes. With errors still lower siblings to collect, exclude from success tree. Hard-stop for canvas break / backend / stalled / illegal body.
7. `Errors`: Clone, PartialEq by kind+at per item, multi-line Display.
8. No source lines, anyhow/thiserror public, leaked dep types, parallel collect, partial PDF.

## Constraint

`error` must not depend on document/layout (`docs/module-dependency.md`).
