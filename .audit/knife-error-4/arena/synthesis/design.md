# Knife 4 synthesized design (base C1 + C2 grafts)

## Synthesis decision

Base: candidate 1 (ErrorBag / LayoutCtx). Cross-judge 25 vs 21.

Grafts from candidate 2:

1. Soft set is typed. `LayoutCtx::soft(FontFault)` only. `FontFault` constructors are `MissingGlyph` / `BoldFaceRequired` / `UnknownFont`. No open `record` for soft in layout.
2. Soft yields no placeable glyphs. Skip the scalar (or zero-width omit). No `.notdef` stand-in. Soft means do not place that run's success path content for the failed scalars. Continue scanning later scalars / siblings.
3. `PartialEq` for `Errors` compares items by kind+at only. Truncation is a separate accessor, not part of equality (confirmed point 7).
4. Assembly still uses open `ErrorBag::record` / `absorb`. Cap and order live only in `ErrorBag`.

Rejected from C1: `.notdef` / body-font stand-in. Rejected from C2: threading `Measured` through every `Flowable::split`, `soft_scan` second walk as primary path.

## Public API

```rust
pub struct Errors { /* non-empty, <= 64 */ }
impl Errors {
    pub fn first(&self) -> &Error;
    pub fn as_slice(&self) -> &[Error];
    pub fn iter(&self) -> impl Iterator<Item = &Error>;
    pub fn truncated(&self) -> bool;
}
impl From<Error> for Errors;
impl Clone / PartialEq (items only) / Display (1 item = Error Display; else numbered) / std::error::Error;
pub type Result<T> = std::result::Result<T, Errors>;
```

`document` / `to_pdf` / build closures use `Result`. Geom / canvas / font register / load_image use `pub(crate)` or public `Fallible<T> = std::result::Result<T, Error>` spelled so single-cause APIs stay one `Error`. Prefer `pub(crate) type Fallible` plus geom keeping `use Fallible as Result` privately, OR geom returns `std::result::Result<T, Error>` explicitly. Callers of `document` see `Errors`.

## Internal

- `ErrorBag` with `record`, `absorb`, `mark`, `clean_since`, `locate_since`, `finish`, `fail`. Cap 64. `seen` for truncation.
- Builders: `errors: ErrorBag` replaces `Option<Error>`. `note` → `record`.
- `Flow::push`: on `Err`, record and do not push node; on `Ok`, push only if bag clean for that child path. Still run later sibling lowers; merge child bags in discovery order. Never prefer container slot over earlier children.
- `Document::section`: prefer return `()` and absorb inner `Err` into bag so later sections run (both candidates). Update call sites.
- `LayoutCtx { fonts, errors: &mut ErrorBag }`. Soft via `soft(FontFault)`. Hard via `Fallible` + `fail` at paint boundary.
- Any non-empty bag → no `paint_pages` / no `Rendered`. Backend / `to_pdf` still one `Error` lifted with `From`.

## Hard-stop after record

Geometry / stalled / illegal body / canvas lifecycle: record (if not already past cap) then stop further layout. Backend unchanged.

## Tests / docs

- Multi-error assembly (sibling + container order).
- Soft measure collects multiple MissingGlyph / BoldFaceRequired.
- Cap 64 + truncated.
- CHANGELOG breaking note. `docs/error-diagnosability.md` 落地记录. Update `code-quality-assessment.md` status lines.
