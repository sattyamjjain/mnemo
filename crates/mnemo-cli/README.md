# `mnemo-mcp-server`

The runnable Mnemo server: an **MCP-native memory database for AI agents**, in Rust.

```bash
cargo install mnemo-mcp-server   # the package
mnemo                            # the binary
```

> **The package name and the binary name differ on purpose.** This crate lives at
> `crates/mnemo-cli/` in the repository, but `mnemo-cli` on crates.io belongs to
> [github.com/watzon/mnemo](https://github.com/watzon/mnemo), an unrelated
> LLM-memory-proxy project, and the bare name `mnemo` belongs to someone else
> again. So the package publishes as `mnemo-mcp-server` and installs a binary
> called `mnemo`. `cargo install mnemo` or `cargo install mnemo-cli` resolves to
> **another author's program** — a CI guard exists specifically to stop that line
> reappearing in this project's docs.

## What it serves

Speaks **MCP over stdio** by default (23 registered tools), and optionally REST,
gRPC, PostgreSQL wire protocol and an authenticated Streamable HTTP transport
behind feature flags. Storage is embedded DuckDB by default, or PostgreSQL +
pgvector.

```bash
MNEMO_DB_PATH=./mnemo.db mnemo          # stdio MCP, the default
```

Point an MCP client at it:

```json
{ "mcpServers": { "mnemo": { "command": "mnemo", "env": { "MNEMO_DB_PATH": "./mnemo.db" } } } }
```

## Configuration

Every flag has an environment variable. The ones most people need:

| variable | meaning |
|---|---|
| `MNEMO_DB_PATH` | database file (default `mnemo.db`) |
| `MNEMO_AGENT_ID` | default agent id |
| `OPENAI_API_KEY` | enables OpenAI embeddings |
| `MNEMO_ONNX_MODEL_PATH` | enables local ONNX embeddings (takes priority) |
| `MNEMO_POSTGRES_URL` | use PostgreSQL instead of DuckDB |
| `MNEMO_ENCRYPTION_KEY` | AES-256-GCM at rest (64-char hex) |

**Without an embedder configured, semantic recall refuses rather than returning
an empty result set.** A memory database that answers "nothing found" to a
question it cannot answer is worse than one that errors, because the caller
writes the empty result down as a fact.

## Links

- Repository, full documentation and benchmarks: <https://github.com/sattyamjjain/mnemo>
- Docs site: <https://sattyamjjain.github.io/mnemo/>

Licensed under Apache-2.0.
