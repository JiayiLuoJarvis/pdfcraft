---
name: verify-pdfcraft
description: Drive pdfcraft (Rust PDF library workspace) via Cargo examples and poppler observers. Use when proving layout, AcroForm fill, orgchart, or post-process PDF behavior end-to-end the way a caller would.
---

# Verify pdfcraft

pdfcraft is a **Cargo workspace of libraries**. Callers do not open a web UI; they depend on crates and run documented examples that emit PDF bytes. This skill treats those examples as the primary user surface. `cargo test` is secondary (implementation proof), not a substitute for the example path.

Surfaces (primary first):

| Surface | What the user touches |
| --- | --- |
| Cargo examples | `hello`, `bill`, `fill_basic`, `equity_chart`, `encrypt`, `merge`, `stamp` |
| Library API | `DocumentSpec` / `fill_and_flatten` / `generate_pdf` / `Encrypt` — exercised *through* examples |
| Integration tests | Assert `RecordingCanvas` / fixtures; not the default drive path |

## Launch

From the workspace root (`Cargo.toml` members include `pdfcraft-pdf`):

```bash
export VERIFY_RUN_ID="manual-$(date +%Y%m%dT%H%M%S)"   # optional; scripts invent one
.cursor/skills/verify-pdfcraft/scripts/doctor.sh
.cursor/skills/verify-pdfcraft/scripts/launch.sh
```

`launch.sh` builds the example binaries and writes `VERIFY_RUN_ID` / `VERIFY_RUN_DIR`. Ready means those binaries exist under `target/debug/examples/` and fonts resolve (see Doctor). There is **no long-lived server**; each drive is a short CLI process.

Fonts (required for text-emitting examples):

- Prefer env already set: `PDFCRAFT_PDF_FONT`, `PDFCRAFT_CJK_FONT`, `PDFCRAFT_FORMS_FONT`, `PDFCRAFT_ORGCHART_FONT`.
- Else scripts fall back to `.fonts/body.ttf` (local, gitignored) then `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf`.
- `hello` needs glyphs for ASCII plus `第` / `页` / `共` — use a CJK-capable face (CI uses Noto CJK; local `.fonts/body.ttf` is Source Han Serif HK when present).

Teardown after a run:

```bash
VERIFY_RUN_ID=<same-id> .cursor/skills/verify-pdfcraft/scripts/cleanup.sh
```

## Doctor

```bash
.cursor/skills/verify-pdfcraft/scripts/doctor.sh
```

Read-only checks: workspace layout, `cargo`/`rustc`, `pdftotext`/`pdfinfo`, layout+forms fonts, `basic_form.pdf` fixture. Exit non-zero → do not drive.

## Drive

Read `features/README.md`, pick a feature file, then run the matching script (or the exact `cargo run` lines in that file). Prefer scripts so evidence lands under a run directory.

```bash
# After launch.sh in the same shell, or re-export VERIFY_RUN_ID / VERIFY_RUN_DIR:
.cursor/skills/verify-pdfcraft/scripts/drive-layout-hello.sh
.cursor/skills/verify-pdfcraft/scripts/drive-forms-fill.sh
.cursor/skills/verify-pdfcraft/scripts/drive-orgchart-equity.sh
.cursor/skills/verify-pdfcraft/scripts/drive-post-encrypt-merge.sh
```

Harness: **shell + Cargo examples + poppler** (`pdfinfo`, `pdftotext`). Stable handles are example names, argv paths, env font vars, and expected PDF text/page counts — not DOM selectors.

### Isolation

- Most drives write under `.cursor/skills/verify-pdfcraft/evidence/<RUN_ID>/scratch/` and copy proofs to `…/proof/`.
- `fill_basic` uses a scratch cwd so `out/fill_basic.pdf` does not collide with the developer's `out/`.
- `equity_chart` **always** writes `out/equity_chart.pdf` at the workspace root. Scripts take `out/.verify-equity.lock` with `flock` and refuse a second concurrent drive; they copy then delete the shared file. Do not double-drive that example without the lock.

## Evidence

Proof root (survives cleanup):

```text
.cursor/skills/verify-pdfcraft/evidence/<RUN_ID>/proof/
```

Per feature, expect at least:

- `*.action.log` — command + stderr/stdout from the example
- `<feature>/observe.txt` — `pdfinfo` / `pdftotext` excerpt
- `<feature>/*.pdf` — the produced bytes

Proof standards:

- Exercise the **documented example** path (`cargo run -p … --example …`), not crate-private setters or test-only hooks.
- Capture the **action** (log) and the **resulting PDF state** (magic `%PDF-`, pages, extracted text or encryption flag).
- Side effects: file written at the declared path; for encrypt, output differs / reports encrypted; for merge, page count equals sum of inputs.
- Do not treat `cargo test` green alone as user-path proof for a mapped feature.

## Cleanup

```bash
VERIFY_RUN_ID=<id> .cursor/skills/verify-pdfcraft/scripts/cleanup.sh
```

Removes that run's `scratch/` and the idle equity lock. **Never** deletes `evidence/<id>/proof/`. Never `pkill` by binary name; these are short-lived processes only.

## Helpers

All under `.cursor/skills/verify-pdfcraft/scripts/` (executable):

| Script | Role |
| --- | --- |
| `env.sh` | Shared paths / font resolution (sourced) |
| `doctor.sh` | Readiness |
| `launch.sh` | Build examples once |
| `drive-layout-hello.sh` | Feature `layout-hello` |
| `drive-forms-fill.sh` | Feature `forms-fill` |
| `drive-orgchart-equity.sh` | Feature `orgchart-equity` |
| `drive-post-encrypt-merge.sh` | Feature `post-encrypt-merge` |
| `cleanup.sh` | Scratch teardown |

Always invoke from the workspace root, or rely on `env.sh` to `cd` there.

## Feature map

Maintained recipes: [`features/README.md`](features/README.md).
