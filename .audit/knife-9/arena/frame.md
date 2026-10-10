# Knife 9 arena frame — image

## Artifact

Design package under `.audit/knife-9/arena/candidate-N/design.md`.

## Baseline

`feat/document-dsl` @ `7cbe823` (knife 8 tip). Branch: `feat/image` from that tip.

## Authority

`docs/提案书.md` §2.7 / §2.12 (`image_at`) / §8 / confirmation table >
`AGENTS.md` >
`docs/quire-design.md` (krilla API facts only; unconfirmed product choices do not bind).

## Confirmed product surface (must ship)

**Shared decode + draw.** Flow `image` and escape `image_at` share decode and canvas paint. APIs and pagination differ.

**Input.** Raw bytes + format enum (`Png` / `Jpeg` / `Gif` / `Webp`). GIF/WebP first frame only. No http(s), no path/Data-URI primary path. Named images dict deferred. No fit/cover.

**Size (pt).**
- width and height given → use both (no forced aspect)
- one side given → other from pixel aspect (needs decode)
- neither → `1 px = 1 pt`

**Flow pagination.** Atomic unsplittable block. Not enough remaining height → whole image next page. Taller than full body frame, or display width > current frame `W` → error, no auto-shrink. Align Start by default; Center/End allowed.

**Cells.** Image allowed in table cells (and list items if nesting rules permit; follow `check_content` patterns).

**Escape.** `behind` / `front` gain `image_at` (same source rules as §2.7). Page coordinates; no margin inset.

**Layout vs backend.** Layout holds `ImageId` + pt rect only. Decode and krilla handles live in backend (or a backend-gated decode path). `interpolate = false`.

**Limits (design facts; bind if already in crate or add minimally).** Reject oversized encoded bytes before decode; after decode reject excessive pixels. Prefer existing `Error` shapes / design-draft `Image` / `ImageTooLarge` only if they fit `Copy`/crate style without inventing a parallel enum.

## Out of scope this knife

- Named images dictionary
- fit / cover
- Flow canvas, qr, svg, toc
- Style builders / `p.bold` / `PageSize` enum
- URI links on images (§8 text links can wait if not already present)
- Multi-section
- Changing `publish = false`

## Rubric

1. Matches §2.7 size rules and atomic split semantics
2. Shared decode path for flow image and `image_at`; no krilla types in public API
3. `FragmentCommand::Image` (or equivalent) paints via `Canvas::draw_image`; RecordingCanvas tests pin rects
4. Table cell allows image; band still rejects tables/inappropriate nesting per existing rules
5. Laziness: thin DSL, one lower path into `FlowNode::Image`; backend draw is real under `backend-krilla`, not silent `Ok(())`
6. Gates: fmt / clippy×2 / test×2 / doc×2 green

## Defaults if signatures underspecified

- Public format: `ImageFormat { Png, Jpeg, Gif, Webp }`
- Register once outside page closures → opaque handle (not named dict). §1 `&seal` shape.
- Flow: thin `doc.image(...)` or builder; bytes+format required at register; optional width/height `Length`
- Escape: `layer.image_at(&handle, Rect)` (page coords)
- Alignment: reuse `TextAlign` against `avail.width`; default Start
- ListItem: reject image this knife (proposal silent); Band: reject image (header stays text/columns)
- TableCell + Column: allow
- `spacing_after`: 0
- No-backend + needs pixel size: error; both sides given: may record rect without decode
- Byte cap: crate-private constant before `from_*` (design 32 MiB as default unless synthesis argues otherwise); full `Limits` API deferred
- Errors: add `Error::Image { message }` (and size/limit variants as needed); do not silence into `Backend` for header decode failures
- Mark vs store: candidates must pick one coherent path; prefer no per-page re-decode
- Branch name: `feat/image`
- Grounding: `.audit/knife-9/how.md` (required reading)

## Structurally distinct directions (runners must diverge)

Do not converge on a safe middle. Prefer one of these whole shapes (or a third that is equally distinct):

1. **Glyph analogue.** Register stores encoded bytes+format+pixel size; `Mark::Image` carries what emit needs (bytes or cached handle material); `LayoutCtx` stays fonts-only; layout nodes hold `DecodedImage` + display size policy.
2. **Parallel store.** `ImageStore` mirrors `FontStore` through `Document` → `Rendered` → `KrillaCanvas` → `emit::render`; marks hold only `ImageId` + pt rect; decode/cache lives on the store at register or first paint.

Both must still satisfy the rubric and §2.7 / §2.12.
