# Orgchart equity

The `equity_chart` example draws a small equity/outbound chart (`甲股东` → `主体公司` → `乙投资`) and writes a PDF.

## Sub-features

- `equity-generate` runs `pdfcraft-orgchart`'s `equity_chart` example.
- `equity-labels` embeds extractable node labels in Chinese.

## How to get to it (user POV)

- Set `PDFCRAFT_ORGCHART_FONT` or `PDFCRAFT_CJK_FONT` to a **single-face** TTF/OTF.
- Run `cargo run -p pdfcraft-orgchart --example equity_chart` (always writes workspace `out/equity_chart.pdf`).

## Driving it with shell+Cargo+poppler

Preconditions:

- `doctor.sh` / `launch.sh` ok.
- CJK-capable single-face font (TTC-only faces are wrong for this example).
- No other verify run holds `out/.verify-equity.lock`.

- **Run example.** Execute `.cursor/skills/verify-pdfcraft/scripts/drive-orgchart-equity.sh`. Exit code `0`.
- **Observe text.** `pdftotext` contains `主体公司` (and ideally `甲股东` / `乙投资`).
- **Proof.** Keep `proof/orgchart-equity/equity_chart.pdf` (script copies then deletes the shared `out/` file).

## Gotchas

- Output path is **not** isolatable (hardcoded via `CARGO_MANIFEST_DIR`). Concurrent drives are refused via flock; do not bypass the script without coordinating.
- Font must be single-face; TTC without a usable index will fail generation.
- Missing Chinese glyphs → failure; do not rewrite the journey to ASCII-only nodes.
