# Forms fill

The `fill_basic` example loads `tests/fixtures/basic_form.pdf`, fills named fields, flattens, and writes a PDF whose text extract shows the filled values.

## Sub-features

- `fill-text` writes text into `employee_name` (`Ada`).
- `fill-checkbox` checks `agree`.
- `fill-flatten` produces a non-interactive flattened PDF.

## How to get to it (user POV)

- Set `PDFCRAFT_FORMS_FONT` to a TrueType face with `glyf`.
- Run `cargo run -p pdfcraft-forms --example fill_basic` (writes `out/fill_basic.pdf` relative to cwd).

## Driving it with shell+Cargo+poppler

Preconditions:

- `doctor.sh` / `launch.sh` ok.
- Fixture `crates/pdfcraft-forms/tests/fixtures/basic_form.pdf` present.
- `PDFCRAFT_FORMS_FONT` resolves (DejaVuSans is enough for `Ada`).

- **Run example.** Execute `.cursor/skills/verify-pdfcraft/scripts/drive-forms-fill.sh` (uses an isolated scratch cwd). Exit code `0`.
- **Observe text.** `pdftotext -layout` on the proof PDF contains `Ada`.
- **Proof.** Keep `proof/forms-fill/fill_basic.pdf` and `observe.txt`.

## Gotchas

- The example hardcodes `out/fill_basic.pdf` relative to **cwd** — always drive from a scratch directory (the script does) so developer `out/` is untouched.
- Chinese fixture journeys need a `glyf` CJK face via `PDFCRAFT_FORMS_FONT`; missing font is a failure, not a skip.
- Needs `pdftotext` for the observable end state used here.
