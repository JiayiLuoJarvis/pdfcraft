# Knife 18 throughput checkpoint

## Blocking

Arena locks preset names/values and conversion shape before code.

## Independent workstreams

One implementer after synthesis: `geom` type + re-export + small test.

## Shared mutable state

| Shared | Rule |
| --- | --- |
| `feat/margin-presets` | Single writer |
| Arena dirs | One candidate each |

## Smallest safe decomposition

One worker. File touch is `geom.rs` (+ maybe `lib.rs` + one test).
