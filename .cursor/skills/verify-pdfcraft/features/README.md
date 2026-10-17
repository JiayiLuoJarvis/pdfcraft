# pdfcraft verification map

This directory is the maintained source for verifying user-facing pdfcraft behavior. Read the index before driving, then use the matching feature file as the recipe.

## Baseline preconditions

- Workspace root has `Cargo.toml` with members `pdfcraft-pdf`, `pdfcraft-forms`, `pdfcraft-orgchart`, `pdfcraft-post`.
- `scripts/doctor.sh` exits 0 (cargo, rustc, pdftotext, pdfinfo, fonts, fixtures).
- `scripts/launch.sh` has built example binaries for this run (`VERIFY_RUN_ID` set).
- Fonts resolve for the feature under test (`PDFCRAFT_*_FONT` or script fallbacks).
- Never treat another developer's leftover `out/*.pdf` as proof from this run — copy into `evidence/<RUN_ID>/proof/`.

## Driving conventions

- Prefer the `scripts/drive-*.sh` helpers so evidence lands under the run directory.
- Treat example names, argv, and env vars as literal.
- Observe with `pdfinfo` and `pdftotext -layout` on the proof PDF after the example exits 0.
- Restore shared paths after `equity_chart` (script deletes `out/equity_chart.pdf` after copy).

## Proof and skip reporting

- Capture the example command output and the resulting PDF metrics/text.
- Mutation proof for fill/encrypt/merge includes a second observer (`pdftotext` / `pdfinfo` / page math).
- Record the feature ID with every artifact under `evidence/<RUN_ID>/proof/`.
- If a font or fixture precondition is missing, report failure with the reason in that feature's Gotchas — do not weaken the journey.

## Feature entry contract

Each feature file: H1 + one paragraph, then exactly four H2s — `Sub-features`, `How to get to it (user POV)`, `Driving it with shell+Cargo+poppler`, `Gotchas`.

## Features

- [Layout hello](./layout-hello.md) — `DocumentSpec` layout tour → multi-page PDF.
- [Forms fill](./forms-fill.md) — AcroForm fill + flatten on `basic_form.pdf`.
- [Orgchart equity](./orgchart-equity.md) — small equity chart PDF.
- [Post encrypt and merge](./post-encrypt-merge.md) — encrypt + merge CLIs.
