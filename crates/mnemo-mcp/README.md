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

## Positioning

Why on-prem + MCP-native + hash-chain-audited memory, and how it compares to
Mem0 / Letta / native provider memory:
**[docs/POSITIONING.md](https://github.com/sattyamjjain/mnemo/blob/main/docs/POSITIONING.md)**.

## License

Apache-2.0.
