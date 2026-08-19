# mnemo-mcp

[![crates.io](https://img.shields.io/crates/v/mnemo-mcp.svg)](https://crates.io/crates/mnemo-mcp)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

The [Model Context Protocol](https://modelcontextprotocol.io/) server interface
for **[Mnemo](https://github.com/sattyamjjain/mnemo)** — the on-prem, MCP-native,
cryptographically-auditable memory database for AI agents.

This crate exposes Mnemo's primitives (**REMEMBER**, **RECALL**, **FORGET**,
**SHARE**, checkpoint/branch/merge/replay, and attention-state put/get) as MCP
tools any agent can connect to over stdio, backed by
[`mnemo-core`](https://crates.io/crates/mnemo-core) and
[`mnemo-compliance`](https://crates.io/crates/mnemo-compliance).

## Install

```bash
cargo add mnemo-mcp
```

For a ready-to-run binary you can register with Claude Desktop / Cursor, install
the CLI instead:

```bash
cargo install mnemo-mcp-server
```

## Use as a library

```rust
use std::sync::Arc;
use mnemo_mcp::MnemoServer; // see crate docs for the exact builder surface

// Wire a MnemoEngine (from mnemo-core) into an MCP server and serve it over
// stdio; optionally attach an attention-state store with `.with_attention_state`.
```

Every write flows through the same hash-chained, tamper-evident path as the core
engine, so the MCP surface is auditable end-to-end.

## Cross-call state: explicit handles, never sessions

The [2026-07-28 MCP revision](https://modelcontextprotocol.io/specification/2026-07-28/changelog)
removes protocol-level sessions and the `Mcp-Session-Id` header, and states the
replacement directly:

> Servers that need cross-call state use explicit, server-minted handles passed
> as ordinary tool arguments.
>
> - [SEP-2567](https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2567)

mnemo has always worked this way. All cross-call state travels through two
server-minted handles, each passed back as an ordinary tool argument:

| Handle | Minted by | Consumed by |
|---|---|---|
| `checkpoint_id` | `mnemo.checkpoint` | `mnemo.branch`, `mnemo.replay` |
| `lease_token` | `mnemo.recall` | `mnemo.forget_subject` |

```jsonc
// 1. the server mints a handle
{"name": "mnemo.checkpoint", "arguments": {"thread_id": "t1", "state_snapshot": {...}}}
// -> {"checkpoint_id": "01a01af0-...", "status": "checkpointed"}

// 2. later calls thread it back as an argument; no session is involved
{"name": "mnemo.branch",  "arguments": {"thread_id": "t1", "new_branch_name": "explore",
                                        "source_checkpoint_id": "01a01af0-..."}}
{"name": "mnemo.replay",  "arguments": {"thread_id": "t1", "checkpoint_id": "01a01af0-..."}}
```

19 of mnemo's 23 tools take their arguments and nothing else, so they *cannot*
depend on connection state. The remaining 4 receive a caller identity resolved
per request from that request's `_meta` (ADR 0002), which is also not
connection state.

This is pinned by `tests/explicit_handle_roundtrip.rs`, which drives
`checkpoint` → `branch` → `replay` over a transport that carries no session
identifier of any kind, and asserts that a handle the server never minted does
not resolve.

Row-by-row conformance against the rest of the revision, including what is still
open and what waits on rmcp:
**[docs/src/integrations/mcp-2026-07-28.md](https://github.com/sattyamjjain/mnemo/blob/main/docs/src/integrations/mcp-2026-07-28.md)**.

## Positioning

Why on-prem + MCP-native + hash-chain-audited memory, and how it compares to
Mem0 / Letta / native provider memory:
**[docs/POSITIONING.md](https://github.com/sattyamjjain/mnemo/blob/main/docs/POSITIONING.md)**.

## License

Apache-2.0.
