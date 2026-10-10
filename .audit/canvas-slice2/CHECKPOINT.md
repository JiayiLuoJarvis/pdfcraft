# Throughput checkpoint — canvas slice 2

## Blocking first steps

1. Arena synthesis must land a single SKETCH before any source edit.
2. Branch from `feat/geom-length-types` (`feat/canvas-recording`) so geom types stay the base.
3. Add only `EmptyLink` (and nothing else) to `Error` if the sketch requires URI rejection on `annotate_link`.

## Independent workstreams

n/a: single shared module surface (`canvas.rs` + thin `DocumentInfo`/`FontId` homes). Parallelizing writers would race the same files.

## Shared mutable state

n/a: one implementer owns `src/canvas.rs`, `src/error.rs`, `src/lib.rs`, and any thin `document`/`fonts` data types. Arena candidates already write to isolated dirs.

## Smallest safe decomposition

One worker implements the synthesized sketch end to end (trait, RecordingCanvas, tests, re-exports, four gates). Splitting trait vs tests across agents adds merge cost without parallelism gain.
