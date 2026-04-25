# `@mndfreek/mnemo-sdk`

TypeScript SDK for [Mnemo](https://github.com/sattyamjjain/mnemo) — an MCP-native memory database for AI agents.

```bash
npm install @mndfreek/mnemo-sdk
```

## Quick start

The SDK speaks MCP over STDIO to a `mnemo` binary running on the same machine. If you don't have the binary yet:

```bash
cargo install mnemo-mcp-server   # Rust toolchain required
```

Then from your TypeScript app:

```ts
import { MnemoClient } from "@mndfreek/mnemo-sdk";

const client = new MnemoClient({ dbPath: "agent.mnemo.db" });
await client.connect();

const { id } = await client.remember({
  content: "User prefers dark mode",
  tags: ["preference"],
});

const { memories } = await client.recall({ query: "user preferences", limit: 5 });
console.log(memories);

await client.close();
```

## Surface

The SDK exposes typed bindings for all 10 MCP tools:

| Method | Tool |
|---|---|
| `client.remember(...)` | `remember` |
| `client.recall(...)` | `recall` |
| `client.forget(...)` | `forget` |
| `client.share(...)` | `share` |
| `client.checkpoint(...)` | `checkpoint` |
| `client.branch(...)` | `branch` |
| `client.merge(...)` | `merge` |
| `client.replay(...)` | `replay` |
| `client.verify(...)` | `verify` |
| `client.delegate(...)` | `delegate` |

Every input and response is typed (`RememberInput`, `RecallResponse`, etc.). Errors land as `MnemoToolError`, `MnemoRpcError`, or `MnemoConnectionError`.

## Configuration

```ts
const client = new MnemoClient({
  dbPath: "agent.mnemo.db",         // required
  agentId: "agent-1",                // optional; default "default"
  binaryPath: "/usr/local/bin/mnemo",// optional; defaults to PATH lookup
  cwd: process.cwd(),                // optional; binary working directory
  env: { ...process.env },           // optional; child process env
});
```

## Why a separate binary?

The TypeScript SDK is a thin client. Mnemo's actual storage engine, hybrid retrieval, and bitemporal graph live in Rust crates that ship as the `mnemo` binary. The binary speaks the [Model Context Protocol](https://modelcontextprotocol.io) over STDIO, which is the same protocol Claude Desktop, OpenAI Agents SDK, and most modern agent frameworks already understand. This SDK exists so TypeScript apps can consume that surface with full type safety.

For other languages: there's also a [Python SDK](https://pypi.org/project/mnemo-db/) and a [Go SDK](https://github.com/sattyamjjain/mnemo/tree/main/sdks/go).

## Source + license

Source: <https://github.com/sattyamjjain/mnemo/tree/main/sdks/typescript>.

Apache-2.0. See [LICENSE](./LICENSE).
