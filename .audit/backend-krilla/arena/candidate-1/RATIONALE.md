# Candidate 1 rationale: plan-then-serialize krilla backend

## Problem

Knife 3 puts a krilla adapter behind the `Canvas` trait that shipped at
`20d1dca`. `Canvas` is a push interface: layout calls `start_page`,
`stroke_path`, `annotate_link`, `finish_page` as separate `&mut self` calls with
no scope around them. krilla is the opposite shape. `Document::start_page`
returns a `Page<'_>` that mutably borrows the document, `Page::surface` returns a
`Surface<'_>` that borrows the page, and annotations may only be added after the
surface drops. Under `#![forbid(unsafe_code)]` no struct can hold two links of
that chain, so the design draft's `KrillaCanvas { doc, surface, ... }` sketch
does not compile. Four more constraints came out of Phase A. Geometry is branded
now (`Length<Finite|NonNegative|Positive>`), so every design-draft snippet that
reads `p.x` or `paint.color.a` has to be rewritten against `.as_pt()` and
`Color::channels()`. `Error` is `Copy` today and `krilla::Document::finish`
returns an error whose text we must carry, which forces `Backend(String)` and
costs `Copy`. Glyph runs and images have no font or image store this knife and
must refuse without panicking. And `docs/quire-design.md` makes two
content-stream experiments mandatory, which means the serializer settings have
to be swappable without any of that reaching the public API.

## Usage (caller's view)

Written first; the types in `SKETCH.md` were derived from it. Full version with
three call sites is in `SKETCH.md`. The core of it:

```rust
#[cfg(feature = "backend-krilla")]
pub fn to_pdf(&self) -> Result<Vec<u8>> {
    let mut canvas = crate::backend::KrillaCanvas::new();
    self.write_to(&mut canvas, &WriteOptions::default())?;
    canvas.finish()
}
```

The caller constructs, draws through the trait it already uses, and takes bytes.
It never names a page, a surface, a serialize setting, a link queue, or a
krilla type. The ordering rules krilla imposes are absent from the call site:
metadata can be set at any point, links can be recorded mid-page, and a failed
call leaves the canvas usable.

## Shape

**The load-bearing decision is that `KrillaCanvas` owns no krilla `Document`.**
Every other candidate shape I considered keeps a live `krilla::Document` in the
struct and streams pages into it at `finish_page`. This one converts each
`Canvas` call into an owned krilla value and files it into a `Plan`. `Document`,
`Page` and `Surface` come into existence inside `Plan::serialize` and die there.
The borrow chain never crosses a function boundary, so there is nothing to make
self-referential. The problem is dissolved rather than worked around, per
redesign-from-first-principles: the constraint "the three cannot co-reside" was
treated as a day-one assumption about where krilla is allowed to live, not as an
obstacle to route around inside the draft's shape.

**Data structures.** `Plan { metadata: Option<Metadata>, pages: Vec<PlannedPage> }`
and `PlannedPage { settings, ops: Vec<PageOp>, links: Vec<Link> }`. Everything
stored is an owned krilla value with no lifetime parameter. Three access
patterns run through it and none of them wants an index or a cache. Recording a
draw op is a push onto `open.ops`. Recording a link is a push onto `open.links`.
Serializing is one forward pass in insertion order. Nothing is ever looked up by
key, so no map appears later.

**`ops` and `links` are separate fields, not one ordered list.** That is the
whole link queue, and it is the point where this shape earns the most. krilla's
rule is that annotations go on after the surface drops. In the draft's shape
that rule is procedural: a `pending_links` buffer, a flush at `finish_page`, and
a clear afterward that must not be forgotten. Here the surface loop iterates
`ops` and physically cannot reach `links`, so the rule is a property of the type
rather than of the code that runs, per encode-lessons-in-structure. There is no
buffer to clear, so there is no half-flushed state to get wrong.

**`PageOp` has exactly two variants, `Fill` and `Stroke`.** The enum is this
knife's scope statement. Because `draw_glyph_run` and `draw_image` return `Err`
at the `Canvas` call, no glyph or image op can exist, so `Plan::serialize` has
no arm that could panic, `todo!()`, or silently skip. The deferral is visible in
the data model instead of hiding in a match arm. Adding fonts later adds a
variant, and the compiler names every place that must change.

**Validation at the boundary, trust inside**, per boundary-discipline. Each
`Canvas` method converts before it records, so a bad path fails at
`stroke_path`, with the caller's stack still pointing at the offending call.
`Plan::serialize` is then infallible except for `Document::finish`. Conversion
runs even when no page is open, because whether a shape is drawable does not
depend on page state.

**What the brands already prove.** Walking the conversions against the branded
types was the most useful thing the sketch did. `Point` is `Finite` on both
axes, `Size` and `Stroke::width` are `Positive`, and `Color` channels are `u8`,
so `krilla::geom::Rect::from_xywh`, `Size::from_wh` and
`NormalizedF32::new(a as f32 / 255.0)` cannot return `None` for anything built
through the public API. Exactly one conversion can genuinely fail: a `Path`
whose only verb is `MoveTo` has no drawable segment. So the design keeps a
single `InvalidGeometry` variant and drops the draft's `InvalidPageGeometry`,
which existed only because page sizes used to be raw `f32`. Subtracting it is
subtract-before-you-add applied to the error enum before it grows.

**Metadata is hoisted, not sequenced.** krilla wants `set_metadata` before the
first page. `set_document_info` writes a field on the `Plan`; `serialize`
applies it before the first `start_page_with`. Last write wins, so calling it
twice or calling it after a page is closed both converge on the same document.
That is make-operations-idempotent, and it deletes a rule the caller would
otherwise have to obey.

**Idempotence and the failure path.** `finish(self)` consumes the canvas, so a
half-written PDF is not representable. A failure mid-run drops a `Plan`, which
is inert data, rather than a partly-serialized krilla `Document`. Because no
creation date is set, the same `Plan` serializes to the same bytes every time,
which is what makes the pinned content-stream assertions stable.

**Dropped commands, stated once.** Ops issued with no page open, and a trailing
page never closed, are discarded. That is exactly what `RecordingCanvas::pages()`
already does with the same command sequence. Making the two implementations
agree turns an edge case into one testable sentence ("`KrillaCanvas` renders
precisely the pages `RecordingCanvas::pages()` reports") and saves an error
variant. Test 9 in `SKETCH.md` drives both canvases from one script and asserts
they agree, per test-behavior-not-implementation.

**Interface depth.** The public surface grows by four `Error` variants, one
Cargo feature, and one dependency pin. No new public type, no new `pub use`, and
`lib.rs` is untouched. Behind that, the backend hides the borrow choreography,
the metadata ordering rule, the annotation ordering rule, the identity-geometry
decision, the two-pen fill/stroke discipline, the serialize settings, and
krilla's error type. The crate-internal surface is one name, `KrillaCanvas`,
with two inherent methods. Learning `KrillaCanvas::new` and `finish` saves the
caller from learning all of it, which is the test `design-red-flags.md` sets for
a deep module. What stays exposed is only what the caller genuinely decides:
when to open a page, what to draw, and when to stop.

**Module count.** No new files. The two existing placeholders, `convert.rs` and
`krilla_canvas.rs`, take the code. `Plan` and its serializer share a file with
`KrillaCanvas` on purpose: splitting "build the plan" from "render the plan"
into separate modules is temporal decomposition, and both halves protect the
same decision about what a page is made of. Tracing a draw call from `Canvas`
method to PDF operator reads two files, per minimize-reader-load.

**What it deliberately does not do.** No `FontStore`, no skrifa, no image
decoding, no `Document::to_pdf`, no DSL, no second backend, no y-flip, no
`draw_text`, no `simple-text` or `rayon`, no full-PDF snapshot test.

## Synthesis decision

TBD.

## Tradeoffs accepted

- We accept holding every page's draw ops in memory before a single PDF byte is
  written, in exchange for a `finish` that is a pure, order-independent,
  idempotent function of inert data. The cost is bounded: `quire-design.md`
  already paginates the whole body into a `Vec<PageBody>` in memory before the
  first page is drawn, so peak memory is not changed in kind.
- We accept that `Plan` stores krilla types, so the backend cannot be exercised
  with the feature off, in exchange for every conversion error surfacing at the
  `Canvas` call that caused it instead of at `finish`.
- We accept dropping `Copy` from `Error`, because `Backend(String)` has to carry
  krilla's message text and there is no way to keep both. `Clone` and
  `PartialEq` stay, so existing `matches!` and `to_string` assertions are
  unaffected. I compiled the two patterns `canvas.rs` uses against a non-`Copy`
  enum this session and both build (measured).
- We accept silently dropping ops issued outside a page and a trailing unclosed
  page, rather than adding an error variant, because that is already
  `RecordingCanvas::pages()` behaviour and divergence between the two `Canvas`
  implementations would be the worse bug.
- We accept a single `InvalidGeometry` variant covering several branches the
  brands prove unreachable, rather than one variant per branch. This looks like
  premature simplification and is not: four distinguishable error cases that
  cannot occur carry no information.
- We accept keeping `default = []` this knife rather than following the design
  doc's `default = ["backend-krilla"]`. With no public entry point reaching the
  backend, defaulting it on costs compile time and buys nothing. Reversible in
  the `to_pdf` PR.

## Alternatives considered

**Live `krilla::Document` in the struct, page-scoped op buffer** (the design
draft's shape, minimally adapted). `KrillaCanvas { doc, pending_ops,
pending_links }`, flushing each page at `finish_page`. It hides the same borrow
problem but keeps three rules alive that this candidate deletes: metadata must
be set before the first `start_page`, `pending_links` must be flushed and then
cleared, and a mid-run failure leaves a partly built `Document` that must be
dropped without `finish`. Lower depth for the same interface size, because those
rules leak into the sequencing of `Canvas` calls.

**Buffer `CanvasCommand` and convert during serialization.** Reuse the enum
already in `canvas.rs` as the backend's internal representation, and call
`convert::*` inside the render loop. It loses on error locality: an invalid path
would surface as `Err` from `finish()`, with nothing telling the caller which
call produced it. It is also information leakage in the sense
`design-red-flags.md` names. `CanvasCommand` is a public, `#[non_exhaustive]`
type that grows for layout's reasons, and making it the backend's storage format
couples two modules to one representation decision.

**Self-referential struct via `ouroboros` or `self_cell`.** Would let the draft's
literal shape compile. Rejected on three counts: a new dependency that AGENTS.md
does not plan, both crates rely on `unsafe` internally, and the resulting struct
makes the borrow chain a permanent feature of the type rather than a detail of
one function.

**Render on a dedicated thread, keeping the borrow chain alive on its stack, fed
by a channel.** This is the other way to invert a push interface into a scoped
one without buffering. Rejected because krilla's `Page` and `Surface` hold
`Rc<RefCell<_>>` and are `!Send` in practice (`quire-design.md`), because the
whole document would then have to be driven from that thread, and because
AGENTS.md forbids parallel page work.

**Change `Canvas` to a scoped callback form** (`with_page(|page| ...)`). Removes
the impedance mismatch at the source. Rejected because it pushes krilla's borrow
shape up into `layout`, which is precisely the coupling AGENTS.md's dependency
rule exists to prevent, and because the trait shipped at `20d1dca` and
`RecordingCanvas` is built against it.

**Write PDF content streams ourselves and use krilla only for assembly.**
Rejected outright. That is re-implementing `pdf-writer`, which the proposal
already refused.

## Open questions and risks

- Should an unfinished trailing page be rendered instead of dropped at `finish`?
  I matched `RecordingCanvas::pages()`, but for a real PDF the alternative
  reading is that dropping a page of content silently is worse than emitting it.
  Which is the intended contract?
- Should `default` flip to `["backend-krilla"]` now, as `quire-design.md` says,
  or in the `to_pdf` PR, as I sketched?
- Does `krilla::page::PageSettings::from_wh` return `PageSettings` or
  `Option<PageSettings>` in 0.8.2? It decides whether `to_page_settings` is
  fallible at all. krilla is not vendored and not in the local cargo registry,
  so I could not check this session. Unresolved, not a guess dressed as a fact.
- What is the exact 0.8.2 link annotation constructor? I expect
  `LinkAnnotation::new(rect, Target::Action(LinkAction::new(uri).into()))`, and
  that is a guess, inferred from the design doc's prose rather than read from
  krilla's source.
- Does krilla's `PathBuilder::finish` reject a lone `MoveTo`? If it accepts it,
  `InvalidGeometry` has no reachable producer this knife, and the variant should
  either wait for the image knife or the test should assert the accepting
  behaviour instead. This is the one assertion in the sketch most likely to move
  during implementation.
- The content-stream and `/Rect` numbers in tests 6 and 7 are placeholders until
  the experiment runs. Risk worth naming: the first instinct on seeing numbers
  that differ from `(10, 20)` is to add a flip to `to_krilla_point`. The test
  comments say not to, and `dump_experiment_pdf` exists so the next person
  re-derives them instead of guessing.

## Next implementation step

Add the `krilla =0.8.2` pin and the `dep:krilla` feature wiring, then write
`convert.rs` end to end with its identity test, because it compiles and proves
the pin without touching the borrow choreography at all.
