# Knife 18 rationale

Chosen shape: a two-variant `MarginPreset` in `geom`. `into_margins` and `From<MarginPreset> for Margins` both call `Margins::uniform` with a private trusted length, 72 pt or 36 pt. `Document::margins` and `Section::margins` stay `fn margins(&mut self, margins: Margins)`. Nothing after that boundary knows a preset existed. `Margins::default()` and `Section::new` stay four zeros.

Frame reverse words that were available and not taken: `assoc-consts`, `mm-round`, `Normal`/`Narrow`, `add-quarter`, `default-inch`, `doc.margin_preset(...)`.

## Enum plus `From` (`assoc-consts`)

§7 asks for an optional preset enum, and the confirmation table says the same. `MarginPreset::{Inch, HalfInch}` is that enum. `into_margins` is `const` and names the resolution; `From` is what `.into()` and `Margins::from` call. Both end in `Margins::uniform`, so a uniform preset is four `Length<NonNegative>` before layout sees it.

Associated constants (`Margins::INCH`, `Margins::HALF_INCH`) would also produce a `Margins`, and they would avoid a new type. They are a closed set of values with no shared name for “a preset”, which is the thing §7 is adding. A free `const` in `geom` has the same problem: callers cannot tell a preset from any other `Margins`. The enum is the catalog. `#[non_exhaustive]` matches `Paper`, so a later variant is source-compatible for callers who only convert.

`Length::from_trusted_pt(72.0)` is legal inside `geom` and already how `LETTER` embeds 612 and 792. `NonNegative::pt` returns `Result` and is not `const`. Routing the preset through it would make `into_margins` fallible for numbers that cannot fail. Tests still build the oracle with `NonNegative::pt(72.0)` so the public checked factory and the trusted literal agree.

## Exact 72 and 36 (`mm-round`)

72 pt is one PDF inch because the point is defined as 1/72 in. 36 pt is half of that. `MM_TO_PT` is `72.0 / 25.4`. `25.4 * MM_TO_PT` is not a promise of the bit pattern `72.0` after the multiply. The preset stores `72.0` and `36.0` directly, so `as_pt()` and `PartialEq` against `NonNegative::pt(72.0)` hold by the same bits.

`NonNegative::mm` remains the way to write a custom side in millimeters. This knife does not add `MarginPreset::from_mm` or a “25.4 mm” alias. The proposal’s「约」belongs to the prose name, not to the stored number.

## `Inch` / `HalfInch` (`Normal`/`Narrow`)

`Normal` and `Narrow` do not say 72 or 36. `Wide` has the same gap. `Pt72` says the number and hides the inch the proposal used as the example. A public variant `约一寸` would read as a Chinese 寸 (about one-thirtieth of a metre), which is not 72 pt.

`Inch` is the PDF inch: rustdoc says exactly 72 pt, ≈1 in, and that「约一寸」is that inch. `HalfInch` is exactly 36 pt, ≈0.5 in. The four accessors stay equal; the name does not collapse the value to a scalar.

## Two presets (`add-quarter`)

§7’s examples are the 72 pt set and the 36 pt set. The open implementation note is which rungs to ship, and the frame default is those two. 18 pt (`QuarterInch`) is another product choice, not required to make `margins(...)` usable. Asymmetric sets (72 vertical, 36 horizontal, and the like) are `Margins::new` for the caller who wants them. Adding them as variants would freeze a house style the proposal did not pick.

## Zero default (`default-inch`)

`docs/quire-design.md` still says 72 pt by default. §7 overrides that: default is four zeros so the library does not guess, and a zero top or bottom means a zero-height band. `default_margins_are_zero` and `Section::new` → `Margins::default()` are that contract. A preset used as `Default` would make every new section an inch margin and would make a header-less page fail `DecorationTallerThanMargin` or, with no header, shrink the body. Callers who want the inch write `MarginPreset::Inch.into()`.

Explicit sections still do not inherit. Each starts at zero until its own `section.margins(...)`.

## Convert at the call (`doc.margin_preset(...)`)

`doc.margins(MarginPreset::Inch.into())` and `section.margins(Margins::from(MarginPreset::HalfInch))` reuse the two methods that already exist. A `margin_preset` method would be a second setter that must repeat the implicit-section check and the `ContentOutsideSection` path. `impl Into<Margins>` on those setters would accept a preset without `.into()`, and it would also accept every future `Into<Margins>`. The smaller change is to leave the parameter type as `Margins`. Conversion stays in `geom`, which layout already depends on.

After conversion, pagination, decoration, and `Page.margins` are unchanged. Top and bottom of the `Margins` are the band heights: `Inch` makes both bands 72 pt even when no header closure is set. That is the existing rule, not a second length.

## Band inset and paper

Current `Band` lowering inserts left and right margin tracks. §2.10 says the header spans the full page width and the caller applies side margins. Tests lock the inset (`header_slots_inset_by_side_margins_and_body_sits_below_the_band` expects header `x = 36` under a 36 pt uniform margin). A preset of 72 pt makes that inset 72 pt. Changing the inset is a decoration change, not a preset change. This knife does not edit `decorate.rs`.

Paper sizes, orientation, and `Custom` are knife 19. `MarginPreset` does not mention `PageSize`. A preset that does not leave a positive body is `InvalidGeometry`, same as an equal `Margins::uniform`.
