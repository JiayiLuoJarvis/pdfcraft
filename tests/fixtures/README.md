# Test fixtures

`quire-measure.ttf` is an OFL subset of Adobe Source Han Serif ExtraLight.
It covers printable ASCII, digits, U+2022, and 第 / 页 / 共.
Family name is QuireMeasure so it does not use Adobe's reserved name Source.
See `LICENSE-OFL.txt`.

Rebuild with `.audit/fonts-skrifa/subset-fixture.sh` plus a local rename step.
Do not vendor a full CJK font.

`one-pixel.png` is a 1 by 1 opaque red PNG for image tests.

Integration tests load fixtures by path from `tests/`, not from `src/`.
