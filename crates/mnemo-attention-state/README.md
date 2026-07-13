# mnemo-attention-state

[![crates.io](https://img.shields.io/crates/v/mnemo-attention-state.svg)](https://crates.io/crates/mnemo-attention-state)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

Attention-state-memory storage substrate for
**[Mnemo](https://github.com/sattyamjjain/mnemo)** — the on-prem, MCP-native,
cryptographically-auditable memory database for AI agents.

This crate provides the `AttentionStateStore` trait and its store for persisting
and retrieving attention-state blobs (anchored on
[arXiv:2605.18226 — Context Memorization](https://arxiv.org/abs/2605.18226)). It
is a required dependency of [`mnemo-mcp`](https://crates.io/crates/mnemo-mcp),
which exposes it as the `mnemo.attention_state.put` / `.get` MCP tools, and is
built on [`mnemo-core`](https://crates.io/crates/mnemo-core).

> Scope: this crate stores and serves the attention-state blobs. Producing and
> consuming them (the model-side KV/attention capture) is out of scope.

## Install

```bash
cargo add mnemo-attention-state
```

## Positioning

How Mnemo compares to Mem0 / Letta / native provider memory on the
compliance-audit axis:
**[docs/POSITIONING.md](https://github.com/sattyamjjain/mnemo/blob/main/docs/POSITIONING.md)**.

## License

Apache-2.0.
