# Candidate 1 rationale — bag-centric collection

## Problem

Knife 4 turns quire's "first error wins" into "every error found in one run, in discovery
order, capped at 64". Today first-wins is not one rule but eleven: each builder (`Paragraph`,
`Table`, `List`, `Band`, `Layer`, `HLine`, `ImageBox`, `TextProps`, `Flow`, `Document`, …)
carries `error: Option<Error>` and its own `note()`. `Flow::push` drops everything after the
first `Err`. `List` and `Columns` `collect()` a `Vec<Result>`. Containers return `self.error`
before looking at their children, so a later container mistake hides an earlier child mistake.
Measurement and layout use `?`, so the first missing glyph ends the run. The shape is
non-obvious because of four constraints from Phase A: `Error` must stay singular with
`Location` set inner-first by outer layers on the way up; `error` may not depend on
`document`/`layout`; public builders (`Flow`, `Table`, …) are handed to caller closures as
`&mut`, so they can't gain lifetimes without changing every signature; and soft measure errors
happen deep in `text::measure`, where page and region aren't known yet.

## Usage (caller's view)

See `design.md` §1. In short:

```rust
match quire::document(A4, build) {
    Ok(rendered) => quire::to_pdf(rendered)?,
    Err(errors) => { eprintln!("{errors}"); for e in &errors { /* e: &Error */ } }
}
```

`Errors` is non-empty, `Clone`, compares items by `kind` + `at`, prints one item exactly like
`Error` and several as a numbered list, and has `first()`, `as_slice()`, `iter()`,
`truncated()`. `document`, `to_pdf` and the build/section closures use
`quire::Result<T> = Result<T, Errors>`. Single-cause calls (geom constructors, `Canvas`,
`Document::font`, `load_image`) return `Result<T, Error>`; `?` lifts them through
`From<Error>`. `Document::section` returns `()`. An error goes into a bag or back to the
caller, never both.

## Shape

**Data structure: one private `ErrorBag` in `error`.** It is an append-only `Vec<Error>` capped
at 64 plus a `seen` counter. `truncated` is derived from `seen > items.len()`, so it is never
stored twice (per single source of truth). All policy lives there: cap, order, counting,
merge, location filling. The eleven `note()` copies are deleted (per encode-lessons-in-structure:
the first-wins rule was a hand-synced list of behaviors).

**Ownership: one owner per bag, merge at the lower boundary.** Every builder value owns a bag.
A child bag is moved into its parent's (`absorb` consumes it, so a child can't be merged twice,
per make-operations-idempotent). Containers lower with `lower(self, into: &mut ErrorBag) ->
Option<FlowNode>`: absorb own bag, lower every child even after failures (point 6: lower
siblings), and return `None` if anything was recorded since the mark. The container never
chooses between its own error and a child's, so the override bug can't come back. Per-owner
bags with merge at the read boundary is the separate-before-serializing-shared-state answer to
"Flow and its parent both write": nested `Flow`s are handed to caller closures and can't borrow
the parent's bag without `Flow<'b>` leaking into the public API.

**Layout borrows one bag.** `Document::paint` owns the paint-time bag and lends it as
`LayoutCtx { fonts, errors: &mut ErrorBag }`. Soft measure sites (`MissingGlyph`,
`UnknownFont` in `text::measure`; `BoldFaceRequired` and line metrics in `layout::paragraph`)
record and continue with a `.notdef` / body-font stand-in. Every other failure stays on the
`Err(Error)` channel (`Fallible<T>`) and hard-stops via `?`. Softness is a property of the
recording site, not of `ErrorKind`.

**Location: one boundary helper.** `Mark` plus `locate_since` let `LayoutCtx::located(at, f)`
fill location both on errors recorded inside `f` and on the hard error `f` returns. Innermost
scopes exit first, so inner-wins holds without a location stack. This replaces the
`map_err(in_region/on_page_index/in_cell)` chains in `paginate`, `decorate` and `layout/table`.

**Single conversion point.** Only `Document::render` turns a bag into `Errors`: assembly
errors don't stop layout, a caller-returned `Err` does, a hard layout error is recorded last and
stops, and paint only runs on a clean bag (per boundary-discipline: validate everything, then
do the one irreversible step). `paginate_sections` splits into `layout_pages` and `paint_pages`
to give that check a seam.

**Interface depth.** Public additions are `Errors` (four methods plus std traits) and the
alias change. Hidden behind it: cap and truncation accounting, ordering across nested owners,
sibling continuation, soft-versus-hard policy, location filling for errors recorded far from
their page. The caller learns nothing about any of it.

**DAG.** `error` gains no dependency. `ErrorBag`, `Mark`, `Fallible` are `pub(crate)`.

## Synthesis decision

TBD.

## Tradeoffs accepted

- We accept a mechanical crate-wide `Result` → `Fallible` rename in exchange for keeping the
  public alias meaning "the list" and keeping one-error internals honest.
- We accept that `Document::section` loses its `Result` return in exchange for closures that
  always finish and no double reporting via `?`.
- We accept one bag field on every builder (an empty `Vec` and a `u64`) in exchange for
  deleting eleven copies of the first-wins rule.
- We accept that page numbers in errors reflect the tree **after** failed nodes were dropped,
  in exchange for collecting measure errors from the rest of the document. A missing table can
  shift later content up a page.
- We accept repeated items (the same footer glyph missing on every page) in exchange for a
  simple contract: discovery order, no filtering, cap 64.
- We accept that a `.notdef` stand-in changes line breaking after a missing glyph, in exchange
  for continuing. Nothing measured that way is ever painted.
- We accept that `Error::in_*` builders survive as wrappers for bag-less hard paths (backend,
  span tiling). Two spellings, one implementation (`fill_missing`).

## Alternatives considered

- **Result-threaded collection (`lower` returns `Result<FlowNode, Errors>`, containers merge
  child `Errors`).** Lost on depth and ordering: every container must merge lists by hand in the
  right order, which is the same per-type policy the bag removes. Soft measure errors would
  have to be returned alongside successful values (`(value, Vec<Error>)`) through every split
  path in layout, widening every `Flowable::split` signature.
- **One shared bag borrowed by every builder (`Flow<'b>` holding `&'b mut ErrorBag`, or
  `Rc<RefCell<ErrorBag>>`).** Lost because `Flow`, `Table`, `List` are public types handed to
  closures: a lifetime parameter leaks into every public signature, and `Rc<RefCell>` makes
  standalone values (`Paragraph::build`, `Table::build`, `Cell::paragraph`) either share a hidden
  global or need a bag injected later. It also turns "who writes" into "anyone holding the Rc".
- **Thread-local / document-global error sink.** Lowest diff at call sites, but invisible data
  flow, breaks with two documents built on one thread, and makes "excluded from the success
  tree" impossible to decide locally.
- **Make `Errors` itself the accumulator (private `push`).** Lost because `Errors` is non-empty
  by contract; an accumulator starts empty. Keeping two types lets `Errors` encode the
  invariant and `ErrorBag` own the mutation.

## Open questions and risks

- Should single-cause public calls (geom, `Canvas`, `Document::font`, `load_image`) return
  `Result<T, Error>` as proposed, or should every public fallible call return `Errors` so there
  is one error type on the surface, at the cost of `Positive::pt(-1.0).unwrap_err().first()`?
- Is `Document::section` returning `()` acceptable, given `publish = false` and 0.x?
- Should `Errors::source()` be `None` (proposed), or delegate to the first item's source so a
  single backend failure keeps its krilla chain for `anyhow`-style reporters?
- Should `PartialEq` on `Errors` include `truncated`? Proposed yes; point 7 only names items.
- Should `truncated()` become `omitted() -> u64`? The count is free (`seen - len`), but it is
  a lower bound once a hard stop ends the run.
- When no font is registered at all, every paragraph records `UnknownFont { id: 0 }` until the
  cap. Should that case hard-stop after the first record instead?
- Should `ContentOutsideSection` be recorded once per document (proposed, matching today) or
  once per offending call?
- Risk: the "measured once" guarantee for soft errors rests on nodes moving by value through
  `split`. A future clone-and-remeasure (for example to try two widths) would double-record.
  A debug assertion or a test that counts items for a split-across-pages paragraph would guard it.

## Next implementation step

Add `Errors`, `ErrorBag`, `Mark`, `Fallible` to `src/error.rs` with unit tests for cap,
`seen`-based `clean_since`, absorb order and inner-wins `locate_since`, plus the mechanical
`Result` → `Fallible` rename, with no behavior change yet.
