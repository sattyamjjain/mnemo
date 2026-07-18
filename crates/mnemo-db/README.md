# `mnemo-db` (Rust) — name-reservation pointer

**This crates.io crate is intentionally empty.** [mnemo](https://github.com/sattyamjjain/mnemo)
— an MCP-native, cryptographically-auditable memory database for AI agents —
ships as focused Rust crates, not a single `mnemo-db` crate.

## Install the real crates

```bash
cargo add mnemo-core   # embeddable memory engine: storage, vector + full-text
                       # search, hash-chained tamper-evident audit log
cargo add mnemo-mcp    # MCP server: REMEMBER / RECALL / FORGET / SHARE as tools
```

Also published: [`mnemo-compliance`](https://crates.io/crates/mnemo-compliance)
(EU AI Act Art.12 · India DPDP · OWASP ASI06 mappings) and
[`mnemo-attention-state`](https://crates.io/crates/mnemo-attention-state).

## Why this crate exists

The unqualified `mnemo` name on crates.io belongs to an unrelated crate. This
crate reserves `mnemo-db` and points Rust users at the official crates rather
than leaving the name to a squatter. It carries no code and will stay a thin
pointer.

## Not the Python package

The Python bindings are distributed **on PyPI** as `mnemo-db`
(`pip install mnemo-db`) — a *separate registry* and a real, functional package.
This crates.io Rust crate is only a pointer; do not confuse the two.

Apache-2.0 · <https://github.com/sattyamjjain/mnemo>
