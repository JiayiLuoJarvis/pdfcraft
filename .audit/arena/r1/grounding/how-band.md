# How Band lowering (grounding for R1)

See parent how explainer summary:

- Engine `measure_band` already uses full page width, origin x=0. Correct per §2.10.
- Bug is only in `Band::lower` (`src/document.rs`): inserts Fixed empty tracks from `page.margins.left/right`, forcing slots into body column.
- R1 target from `docs/element-repair.md`: add `inset_left`/`inset_right` (default 0), `Band::inset(l,r)`; stop reading side margins for tracks; slots lay out in page_width minus insets; keep left/right/left_link/right_link; do not change measure_band; do not rename layout; keep lowering in document.rs.
- Tests that lock auto-inset must flip to x=0 without inset, or call inset then assert margin.
