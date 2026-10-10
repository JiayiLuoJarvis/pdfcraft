# Explorer: proposal vs design (knife 7)

Source: how explorer task 053002db-0b16-4c82-a38c-094229eb2b0f

## Must (提案书)

- Columns: Fixed/Ratio/≤1 Auto; gap default 12pt; top align; **splittable** per remaining height; nest with stack; W = column width.
- Table: row-atomic split; multi-row header; repeat_header default on (all header rows); Fixed/Ratio/≤1 Auto; padding/border; **colSpan/rowSpan**; cell Paragraph+Spacer (+list); forbid columns/pageBreak/keep_together/nested table → InvalidCellContent.
- List: enum markers (decimal/A/a/roman upper+lower) + per-item custom string; hanging indent; marker first fragment only; nest; no table/columns in item; pageBreak → InvalidCellContent.
- Decorate: band = margin top/bottom; full page width x=0..page_w; overflow error; no body reflow; PageNumber/PageCount after plan.

## Design keep (non-conflicting tech)

- resolve_column_widths algorithm; gap before resolve; border centered one border_width deduction; row natural_height on full body frame; decorate after plan; list continue indent without re-marker.

## Conflicts (提案书 wins)

| Topic | Win |
| --- | --- |
| Columns split | Splittable Partial |
| Cell merge | Implement colspan/rowspan |
| Header repeat | All header rows, not only row 0 |
| Decorate x | Full page width |
| Default margin | 0 (already in code) |
| List markers | Full enum + custom |
| Paint order | behind→header→body→footer→front |

## Open for synthesis defaults

- Decorate multi-line in band vs single line: proposal allows multi-line until taller than band.
- Section dual page numbers: defer public closures to knife 8; internal PageNumber/PageCount = document totals for knife 7.
- Custom marker: per-item string (no callback API this knife).
- nest_indent default = marker_width.
- Adopt design error names ColumnWidths/Decoration* as tech detail.
