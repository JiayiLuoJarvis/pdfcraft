# Cross-judge (parent, incomplete N)

Parent model family is Composer. Arena runners used claude-opus-5-5-medium, gpt-5.6-sol-high, grok-4.7-high. Only candidate-2 produced files before the implementation unit started.

## Rubric scores (candidate-2 only)

| Criterion | Score /5 | Note |
| --- | --- | --- |
| Layering | 5 | fonts/text/krilla_canvas stay krilla-free; emit builds Font |
| Glyph pipeline | 5 | advance_norm once, KrillaGlyph unchanged |
| Errors | 5 | Font / MissingGlyph / UnknownFont / FontMetricsMismatch at the right boundaries |
| Tests | 4 | TJ pin and fixture literals still need a first observation |
| Minimal public API | 5 | FontId only |
| Distinct direction | 5 | Marks carry bytes |

**Base recommendation:** candidate-2.

Dropout of 1 and 3 is recorded, not scored.
