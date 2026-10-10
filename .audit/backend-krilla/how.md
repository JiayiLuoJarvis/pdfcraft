# How: Canvas + planned krilla adapter (knife 3 grounding)

Summarized from how explainer (agent db9c80ee).

## Shipped
- Canvas trait + RecordingCanvas + CanvasCommand (src/canvas.rs)
- Branded geom Length<Finite|NonNegative|Positive>; Point/Size/Rect accessors; Color::channels()
- LinkTarget::uri rejects empty; annotate_link takes LinkTarget
- Error: NonFinite/Negative/NonPositive Length + EmptyLink (Copy today)
- backend/ placeholders only; feature backend-krilla empty; default=[]

## Must build (design PR4)
- krilla=0.8.2 optional, default-features=false, features=[raster-images]
- convert identity; KrillaCanvas + finish(self)->Vec<u8>
- metadata before first page; link queue until after surface drop
- Tests: %PDF, identity convert, observed path ops + Annot rect under SerializeSettings pretty/uncompressed/no tagging

## Hard fork
Document/Page/Surface cannot co-reside in one struct (forbid unsafe_code). Need buffer-replay or equivalent.

## Defer
FontStore/skrifa glyph TJ tests; image decode; document DSL; full Error enum.
