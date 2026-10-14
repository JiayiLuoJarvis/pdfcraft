# Fixtures

These files are test inputs. `pdfcraft-forms` does not depend on pdfcraft.

## Font

`DejaVuSans.ttf` is copied from `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf` in Debian package `fonts-dejavu-core`. `DejaVuSans.LICENSE` is the copyright file that ships with that package.

That license grants permission to reproduce and distribute the font, including inside a larger software package, when the copyright and permission notice stay with the font. It does not allow selling the font by itself. The crate `include` list ships `DejaVuSans.ttf` and `DejaVuSans.LICENSE`.

CJK fills use a Source Han Serif HK face from outside this repo. Set `PDFCRAFT_FORMS_FONT` to that file.

## PDF files

`basic_form.pdf`, `chinese_form.pdf`, `signature_form.pdf`, and `xfa_form.pdf` are copied from `/mnt/data/dev/hcsw-pdfcraft/backend/packages/pdfcraft-forms/tests/fixtures/`.

`tax_refund_guarantee_id.pdf` is copied from `/mnt/data/dev/hcsw-pdfcraft/backend/assets/forms/tax_refund_guarantee_id.pdf`. The form is 偿还税款保证书, the ID number template. The passport and BR templates in that directory are not checked in here. `examples/fill_tax_refund_guarantee.rs` reads this PDF from the git tree.

No license file accompanies these PDFs, and the internal pdfcraft paths do not state a right to redistribute them to third parties. They stay in git so local builds and CI can run the tests and examples. The crate `include` list does not name them, so a packaged crate does not ship them.

Integration tests under `tests/` stay in git with the PDFs. The crate `include` list does not ship those test sources. Examples are included. `fill_basic` and `fill_tax_refund_guarantee` still need the git tree, because they read the omitted PDFs.
