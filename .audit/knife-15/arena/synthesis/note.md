# Arena synthesis note

**Base.** Candidate 2. Parent agrees with [cross-judge](../cross-judge.md) (30/30). Smaller public API than C3 `Linked` / C4 `IntoLinkTarget`. Avoids C1's `link -> Result` clash with `paragraph(|_| ())`.

**Grafts.**

| From | Graft |
| --- | --- |
| C1 | Ship order: internal run/group → FragmentCommand → DSL. Keep group-link homogeneity `debug_assert` after flush. |
| C3 | Tests `header_uri_can_differ_per_page`, `link_across_pages_restarts_y`, scheme/space URI as-given, `cell_and_layer_text_do_not_annotate`. Internal `TextRun::with_link` helper only (not public `Linked`). Latin fixture glyphs in document tests (avoid MissingGlyph on Han). |
| C4 | Document why `Paragraph::link` returns `()` and records `EmptyLink` on the builder (not `Result`). |

**Rejected.**

- C1 dual API / `Result` inside paragraph closure.
- C3 public `Linked` re-export (runtime blank-in-scope is enough; frame allows blank in link closure).
- C4 `IntoLinkTarget` and PDF plaintext URI structure asserts.

**Verification.** Synthesis matches `how.md` seams (`TextRun.link`, flush-on-link, `LinePiece`/`FragmentCommand::Link`, Band slot helpers, reuse canvas/backend). Frame autonomy defaults unchanged. Ready to implement.
