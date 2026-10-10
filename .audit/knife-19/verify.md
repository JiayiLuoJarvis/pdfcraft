# Knife 19 verify — crate name and paper enum

**Date.** 2026-10-10  
**Branch cut from.** `feat/margin-presets` @ tip after knife 18 feature commits.  
**Authority.** `docs/提案书.md` §9 (发布名 quire-pdf), §7 / §2.11 paper enum.

## Findings (measured)

| Check | Result | Evidence |
| --- | --- | --- |
| Package name | Already `quire-pdf` | `Cargo.toml:2` |
| `publish = false` | Kept | `Cargo.toml:11` |
| README crate name | Already `quire-pdf` | `README.md:1-3` |
| Rust import path | `quire_pdf` | `tests/document.rs` |
| Paper A0–A6 | Present | `src/geom.rs` `Paper` + `PageSize::A0`…`A6` |
| Size pins | Present | geom tests listing A0–A6 mm |

## Product decision

No production diff this knife. A-series gap is absent. Rename work was already landed in an earlier PR.

## Gates

Same green suite as knife 18 tip (`cargo fmt` / `clippy -D warnings` / `cargo test`). No new commits required for behavior.

## Parent gate re-run

Re-ran `cargo test` on branch tip `a97b18b6dde08e2c630536a2c908187d9d081731` after handoff close. Exit 0. Suite matches knife 18 counts (200 lib + 1 ignored, 78 document).
