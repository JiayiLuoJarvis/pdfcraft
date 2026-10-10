# Page margins today, and optional presets on top

## Overview

Today, a page margin is one public `Margins` value containing four non-negative `Length`s in CSS order: top, right, bottom, left. It is passed to either `Document::margins` for the implicit single section or `Section::margins` for an explicit section. Every section starts with `Margins::default()`, and that default is four zeros.

Pagination subtracts left/right from page width and top/bottom from page height. The body starts at `(left, top)`. Top and bottom additionally define the fixed header and footer band heights; decorations do not enlarge those bands or push the body.

§7 presets should therefore be a construction convenience above this model:

```text
preset selection -> Margins { top, right, bottom, left } -> existing margins(...) path
```

They should not add a second margin representation, change `Document`/`Section`, or become an implicit default. `Margins::default()` and every new section must remain four zeros. The proposal examples are uniform 72 pt (exactly one inch in PDF units; described as “约一寸”) and uniform 36 pt, but their public names and final catalog are an arena product call.

## Key Concepts

### `Margins` is the semantic value

`src/geom.rs` defines four private `Length<NonNegative>` fields and exposes:

- `Margins::new(top, right, bottom, left)` for independent sides;
- `Margins::uniform(value)` as four-side sugar;
- side accessors;
- `Default`, implemented as `uniform(NonNegative::ZERO)`.

Individual sides cannot be negative, non-finite, or untyped raw floats. `new` and `uniform` are `const` because they receive already-validated lengths.

A preset must resolve to this exact value shape: four `Length<NonNegative>` values. Even a uniform preset is not a scalar once it crosses into layout; it is a `Margins` with four equal sides. This keeps custom asymmetric margins and presets on one pipeline.

### Margins have two jobs

For a page of width `pw` and height `ph`, pagination derives:

```text
body x      = left
body y      = top
body width  = pw - left - right
body height = ph - top - bottom
```

The body width controls wrapping in the root flow. The body origin offsets painted flow fragments.

At the same time:

```text
header band = y 0 .. top
footer band = y ph-bottom .. ph
```

Top and bottom are therefore not merely whitespace around an independently sized header/footer. They are the header/footer band heights. A zero top or bottom means a zero-height band; non-empty decoration then fails with `DecorationTallerThanMargin`.

### Document sugar versus section ownership

`Document` owns an implicit lead `Section`. `Document::margins(m)` delegates to that section while the document remains in implicit-section mode. With explicit sections, each `Section` owns its own margins and may use a different page size.

Once explicit sections have started, `Document::margins` is invalid and records `ContentOutsideSection`; callers must use `section.margins(...)`. Conversely, setting non-default document margins counts as writing the implicit section, so it prevents replacing that lead with explicit sections.

Presets should not alter this ownership rule. They only make the argument:

```text
doc.margins(preset -> Margins)
section.margins(preset -> Margins)
```

## How It Works

1. `document(page_size, ...)` creates `Document::new`, whose implicit `Section::new` stores `Margins::default()`—four zeros.
2. The caller may replace that value with `Document::margins(Margins)` or, inside explicit-section mode, `Section::margins(Margins)`. A later call replaces the whole four-side value.
3. `Section::lower` combines the section page-size override (or document default) with its margins in `PaginateConfig`.
4. `PaginateConfig::validate` rejects a non-positive body width or height. Thus individually valid sides can still yield `InvalidGeometry` when left + right is at least page width or top + bottom is at least page height.
5. Pagination wraps content at `body_width`, lays it into `body_height`, and paints it from `(left, top)`.
6. The section margins are copied into each page's `PageInfo`, then exposed publicly as `Page::margins` to header, footer, behind, and front closures.
7. Decoration measurement uses top as the header capacity and bottom as the footer capacity. The header is placed at page y=0; the footer at `page_height - bottom`.
8. Presets should enter before step 2 by producing a normal `Margins`. All later validation, wrapping, section scoping, page metadata, and band behavior remain unchanged.

An implementation shape can be an enum that resolves/converts to `Margins`, associated preset constructors/constants on `Margins`, or another thin public namespace. The arena should choose that public spelling. The invariant is more important than the spelling: no preset state reaches layout, and neither `Document::margins` nor `Section::margins` needs a parallel overload.

## Where Things Live

| Location | Responsibility |
| --- | --- |
| `src/geom.rs` | `Length<NonNegative>`, `NonNegative::ZERO`, `Margins::{new, uniform}`, accessors, and zero `Default`. This is the natural value/conversion layer for presets. |
| `src/document.rs` | `Section` stores margins; `Document::margins` delegates to the implicit section; `Section::lower` sends the value to pagination; `Page::margins` exposes the effective value to page-time closures. |
| `src/layout/paginate.rs` | Computes body width, body height, and body origin; rejects margins that leave no positive body frame; preserves section-specific margins across pages. |
| `src/layout/decorate.rs` | Uses top/bottom as header/footer capacities and origins; copies margins into `PageInfo`. |
| `src/lib.rs` | Re-exports `Margins`, `Length`, and `NonNegative`. Any new public preset type or namespace needs an intentional export decision here. |
| `tests/document.rs` | Public-path coverage for uniform/asymmetric document and section margins, body and band coordinates, zero-band rejection, section scoping, and page metadata behavior. |
| `src/geom.rs` tests | Locks `Margins::default()` to four zeros. |
| `docs/提案书.md` §7 and §2.10 | Authority: default is zero; presets are optional sugar; top/bottom are band heights. |

## Gotchas

- **Do not implement a preset as the new default.** The older design draft still says 72 pt by default, but the confirmed proposal overrides it: default is four zeros.
- **Do not store a preset in `Document`, `Section`, or `PaginateConfig`.** Resolve it to `Margins` at the public geometry boundary so custom and preset values cannot drift semantically.
- **Keep all four sides.** Uniform 72 pt and 36 pt are examples, not permission to narrow the model to one scalar or a horizontal/vertical pair.
- **Names are unresolved product language.** §7 confirms optional presets but explicitly leaves the concrete set and names for implementation-time discussion. “72 pt”, “one inch”, “约一寸”, “wide”, and “normal” communicate different promises; the arena must choose deliberately.
- **72 pt is exactly one international inch in PDF units.** The proposal's “约一寸” wording should not silently become a claim about a Chinese `寸`, nor should a marketing name obscure the exact numeric contract.
- **Top/bottom affect decoration capacity.** Choosing a 36 pt or 72 pt preset also chooses header/footer band heights of 36 pt or 72 pt, even when no decoration closure is installed.
- **Preset validity is page-size dependent.** A preset made of valid non-negative lengths can still leave a custom page with no positive body frame and fail with `InvalidGeometry`.
- **Explicit sections do not inherit document margins.** Each starts at zero and must receive its own custom or preset `Margins`.
- **Current `Band` behavior differs from the confirmed prose.** The proposal says header/footer layout spans the full page width and callers manually use side margins when desired. The current public `Band::left/right` lowering automatically inserts left/right margin tracks, and tests lock that inset. Presets will make that existing inset larger or smaller; they should not be used to settle this separate contract mismatch.
- **Layers remain page-coordinate escapes.** `behind` and `front` receive margins through `Page`, but their drawing coordinates are not automatically inset.

## Arena frame: five takeaways

1. The invariant is already clear: effective margins are one `Margins` value with four non-negative lengths in top/right/bottom/left order.
2. The zero default is contractual and implemented twice—`Margins::default()` and `Section::new`; optional presets must never enter either default path.
3. Presets should terminate immediately in `Margins`, then reuse unchanged `Document::margins` / `Section::margins`, pagination validation, and page metadata.
4. A preset controls both the body frame and the header/footer band heights because top and bottom margins are those band heights.
5. Arena decisions are the public namespace, names, exact preset catalog, and conversion spelling—especially how to label the proposed uniform 72 pt and 36 pt choices without changing their exact four-length semantics.
