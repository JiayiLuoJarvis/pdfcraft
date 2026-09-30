# quire

Rust PDF document layout library.

You describe the document with a Rust DSL (flow content + page-anchored escapes).
`quire` measures text, breaks lines and pages, lays out tables, then draws through
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
