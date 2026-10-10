# Explorer: flow split protocol

Source: how explorer task ab819a46-f21f-4b95-84c3-7aa23c6495ec

Summary captured in full agent return. Key hooks for knife 7:

- Extend `FlowNode` + `split` match; Stack already composes any conforming child.
- `Partial.tail` carries remaining Columns / Table rows / List items.
- Paragraph width baked at `layout` time; column/cell must pass narrower W then.
- `FragmentCommand` needs Stroke (+ with_offset/draw_fragment); decorate is not a FlowNode.
- Columns: 提案书 splittable (not design unsplittable).
- Repeat header lives on tail for next split, not in previous head.
- List continue: keep indent without marker commands on later fragments.
- Avoid zero-height Partial loops on full frames.
