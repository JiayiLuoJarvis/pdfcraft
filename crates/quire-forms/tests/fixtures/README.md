# Fixtures

`basic_form.pdf`, `chinese_form.pdf`, `signature_form.pdf`, and `xfa_form.pdf` are copied from `/mnt/data/dev/hcsw-pdfcraft/backend/packages/pdfcraft-forms/tests/fixtures/`. They are test inputs. This crate does not depend on that package.

`tax_refund_guarantee_id.pdf` is copied from `/mnt/data/dev/hcsw-pdfcraft/backend/assets/forms/tax_refund_guarantee_id.pdf` (偿还税款保证书 · 身份证号). The passport and BR templates in that directory are not checked in here. Example: `examples/fill_tax_refund_guarantee.rs`.

`DejaVuSans.ttf` is copied from `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf` in Debian package `fonts-dejavu-core`. `DejaVuSans.LICENSE` is the copyright file that ships with that package. CJK production fills use Source Han Serif HK from outside this repo (`QUIRE_FORMS_FONT`).
