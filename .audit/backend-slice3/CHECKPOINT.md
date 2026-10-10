# Throughput checkpoint — backend-krilla slice 3

## Blocking first steps
1. Pin krilla =0.8.2 (default-features=false, raster-images) behind feature backend-krilla.
2. Extend Error only as needed: InvalidGeometry, InvalidPageGeometry, Backend(String), BackendRequired.
3. Implement convert identity + KrillaCanvas against existing Canvas trait (no glyphs this slice).

## Independent workstreams
n/a: single backend module cluster; one writer.

## Shared mutable state
n/a: one implementer owns Cargo.toml + backend/* + error.rs + lib wiring.

## Smallest safe decomposition
One worker end-to-end for convert + KrillaCanvas + %PDF smoke + optional content-stream coordinate pin.
