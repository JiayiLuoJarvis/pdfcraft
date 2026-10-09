# Knife 12 implement status

**Branch:** `feat/hline`  
**Baseline:** `7cb1654` (`feat/layer-path`)  
**Date:** 2026-10-09

## Shipped

- `src/layout/hline.rs` — `HLineNode` + Image-style unbreakable `split`
- `FlowNode::HLine`; `check_content` allows HLine like Spacer
- Public `HLine` builder; `Flow::hline`; `Cell::hline` / `CellContent::HLine`
- DSL name **`hline`** (not `rule`)
- Defaults: full frame width, Center when short, 1 pt, black, zero spaces
- RecordingCanvas stroke asserts; backend-krilla `%PDF` smoke
- Failures before `start_page` (taller-than-body, wider-than-frame)

## Autonomy defaults

- Fixed default color black + 1 pt (not follow text color). Reverse: `follow-text-color`.

## Gates (parent re-verified)

Seven commands green after implement.

With `backend-krilla`: 161 lib + 1 ignored, 36 integration.

## Deferred

- style builders / `p.bold` / first-line indent
- §2.2 fill-in underscore
- §2.14 flow canvas
- dashed / double / vertical rules
- general `Cell::flow` stack
