# Arena synthesis note

**Base.** Candidate 3 (parent + [cross-judge](../cross-judge.md) agree, 30/30).
**Grafts.** C1 ship order + Hard/eof tests. C2 width debug_assert + offset page coords. C4 em length via stroke at default 12 pt (font-size API absent).
**Rejected.** Larger APIs (C1/C4), required empty closures and negative offsets (C2), InvalidGeometry for fill-after (C4).
**Verification.** Synthesis matches how.md plug-in seams (InlineItem, break_spans, LinePiece Stroke, no HLine height). Frame defaults unchanged. Ready to implement.
