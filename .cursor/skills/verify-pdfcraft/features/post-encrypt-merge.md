# Post encrypt and merge

`pdfcraft-post` examples take existing PDF bytes, encrypt with a printable read-only preset, and merge page lists into one file — the post-process path callers use after layout/forms.

## Sub-features

- `post-encrypt` runs `encrypt <in> <out> <owner-password>`.
- `post-merge` runs `merge <out> <in1> <in2…>` and sums page counts.

## How to get to it (user POV)

- `cargo run -p pdfcraft-post --example encrypt -- in.pdf out.pdf owner-secret`
- `cargo run -p pdfcraft-post --example merge -- out.pdf a.pdf b.pdf`

## Driving it with shell+Cargo+poppler

Preconditions:

- `doctor.sh` / `launch.sh` ok.
- Input PDF available (script uses `crates/pdfcraft-forms/tests/fixtures/basic_form.pdf`).

- **Encrypt.** Run `.cursor/skills/verify-pdfcraft/scripts/drive-post-encrypt-merge.sh` (or the encrypt example alone). Exit code `0`; stderr/stdout reports `wrote …`.
- **Observe encrypt.** Proof PDF starts with `%PDF-`; `pdfinfo` reports `Encrypted: yes`, or the bytes differ from the input.
- **Merge.** Same script merges fixture + encrypted into one file; `Pages` equals sum of input page counts.
- **Proof.** Keep `proof/post-encrypt-merge/encrypted.pdf`, `merged.pdf`, and `observe.txt`.

## Gotchas

- Encrypt requires an owner password argv; omitting usage text and exit 1 is expected.
- Encrypted PDFs may not yield useful `pdftotext` — rely on `pdfinfo` / byte inequality for encrypt proof.
- Do not use network or remote finalize; these examples are local file I/O only.
