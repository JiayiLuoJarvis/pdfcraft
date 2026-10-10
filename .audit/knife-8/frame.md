# Knife 8 arena frame — document DSL

## Artifact

Design package under `.audit/knife-8/arena/candidate-N/design.md`.

## Baseline

`feat/document-dsl` @ `6dbf40c` (knife 7 tip).

## Confirmed public surface (提案书 §1 examples + §2 directions)

Must wire:

- `document(page_size, |doc| …) -> Result<Vec<u8>>` (or equivalent that can PDF under backend-krilla and RecordingCanvas in tests)
- Assemble-time: `paragraph`, `table`, `columns`, `keep_together`, `page_break`, `spacer`, list (ul/ol)
- Draw-time: `header` / `footer` with page number/count + margins
- Font registration path for callers
- Lower into internal `FlowNode` + existing `paginate_root` + `Decorations`

Optional if fits without invention:

- `behind` / `front` with `text_at` (image_at needs image layout; skip if not ready)
- Single implicit section (multi-section can wait)

Do not invent:

- Flow canvas, qr, svg, toc
- Named images dict
- Per-page header/footer off
- Serde JSON primary API
- Exact table builder method names beyond §1 sketch + §2.6 direction (`header`/`row`/`col_span`/`row_span`/`repeat_header`)

## Rubric

1. Matches §1 closure split (assemble vs page-time)
2. No unconfirmed builders
3. Lowers to knife-7 layout without reimplementing split
4. RecordingCanvas / `%PDF` smoke tests
5. Laziness: thin builders, one lower path

## Defaults if signatures underspecified

- Page size: reuse `geom::Size` / `A4` (enum PageSize can thin-wrap A4/LETTER + Custom)
- Default margins 0
- Paragraph builder: `.text(&str)` accumulates; optional `.font`/size later if needed for tests
- Table: closure `table(|t| { t.header([...]); t.row([...]); })` string cells → Text nodes
- columns: `columns(|row| { row.col(|c| …); })` equal Auto/Ratio tracks
- List: `ul` / `ol` with item closures
- header/footer: closures receiving page info (document totals; section counters equal totals for single section)
