# quire-pdf

Rust PDF document layout library (crate name **`quire-pdf`**).

You describe the document as a `DocumentSpec` (flow content + page-anchored marks) and lay it out with `layout_spec`.
The library measures text, breaks lines and pages, lays out tables, then draws through
an internal canvas. The only PDF backend is a private [krilla](https://crates.io/crates/krilla)
adapter.

**Status:** scaffolding. Public API is not frozen. See `docs/提案书.md`.

## Docs

| Document | Role |
| --- | --- |
| [`docs/提案书.md`](docs/提案书.md) | Confirmed product decisions (source of truth) |
| [`docs/quire-design.md`](docs/quire-design.md) | Design draft; unconfirmed items do not bind |
| [`AGENTS.md`](AGENTS.md) | Conventions for humans and coding agents |

## License

MIT OR Apache-2.0
