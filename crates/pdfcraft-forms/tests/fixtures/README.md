# Test fixtures

PDF fixtures (`basic_form.pdf`, `chinese_form.pdf`, `signature_form.pdf`,
`xfa_form.pdf`, `tax_refund_guarantee_id.pdf`) stay in git.

Font bytes are **not** vendored. Set `PDFCRAFT_FORMS_FONT` to a TrueType face
with a `glyf` table (for example DejaVu Sans from `fonts-dejavu-core`, or a
Source Han / Noto CJK face for Chinese fills). Tests that need a font skip when
the variable is unset. The Chinese fill test also requires that face to cover
the characters under test.
