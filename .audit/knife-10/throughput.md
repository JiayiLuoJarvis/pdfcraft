# Throughput checkpoint — knife 10 section

## Blocking first steps
1. Gates already green on a12b610 (done).
2. how.md grounded (done).
3. Arena design must land before any src/ edits (paper types + Document section model + paginate seam are coupled).

## Independent workstreams
After synthesis locks the shape:
- **geom paper types** (`PageSize` / `Orientation` → `Size`) can land first as a verifiable unit (tests only).
- **layout paginate/decorate multi-config** can follow with unit tests using layout-only fixtures.
- **document DSL sugar + section builder** serializes after layout seam exists (shared `PageInfo` / `PagePlan` fields).

## Shared mutable state
- One branch `feat/section`, one working tree. Arena candidates write only under `.audit/knife-10/arena/candidate-N/` (no src/).
- Implementation: serialize writes to `document.rs` / `paginate.rs` / `decorate.rs` / `geom.rs` in small ordered commits.

## Smallest safe decomposition
One implementer owns the full knife after synthesis (coupled paint path). Not three parallel code writers on the same files.
