# `@mndfreek/mnemo-sdk`

TypeScript SDK for [Mnemo](https://github.com/sattyamjjain/mnemo) — an MCP-native memory database for AI agents.

```bash
npm install @mndfreek/mnemo-sdk
```

## Status: maintenance only

**This SDK is not being developed on the 0.5 train.** It works, it is not
abandoned, and it is not gaining features alongside the Rust line.

| | |
|---|---|
| Latest on npm | **0.4.4**, published 2026-05-18 |
| Last server version it is **verified** against | **0.5.26** |
| In this repo | `package.json` is at 0.4.8 (0.4.5 through 0.4.8 are unpublished) |
| Why the gap | `npm publish` fails with `npm whoami -> 401 Unauthorized`: the `NPM_TOKEN` secret is expired or revoked. That is an operator action, not a code change. |

### What "verified against 0.5.26" means, exactly

The published **0.4.4** package was installed from npm and run against a
`mnemo-mcp-server` built from workspace `0.5.26`. `remember`, `recall`
(`strategy: "lexical"`) and `verify` all succeeded, and `verify` returned a valid
hash chain. That is the compatibility claim, and it is the whole of it.

It works because the SDK is a thin **MCP-over-STDIO client**. It does not embed
the engine, so it targets the server's **tool surface** (the 23 registered tools)
rather than a `mnemo-core` version. Tools have been added to that surface since
0.4.4 and none removed, so an older client keeps working against a newer server.

### What is not covered

Worth stating plainly, because the test count looks reassuring and is not:

- The SDK's own suite is **21 tests of type shapes and error paths**. Not one of
  them spawns a server. Passing tests here say nothing about wire compatibility;
  the 0.5.26 verification above was done by hand and is a point-in-time check,
  not a gate in CI.
- Tools added to the server after 0.4.4 are absent from this client. You get the
  subset 0.4.4 knows about.

### One thing that will bite you immediately

`recall()` defaults to the `auto` strategy, which requires the server to have a
configured embedder. A stock `mnemo` server has none, and will return a hard
error rather than an empty list, on purpose:

```
mnemo.recall: embedder not configured for 'auto' recall on backend 'duckdb'
```

Pass `strategy: "lexical"` for BM25 without an embedder, or configure one with
`OPENAI_API_KEY` or `MNEMO_ONNX_MODEL_PATH`.

### If you need the current line

Use the Python SDK (`pip install mnemo-db`, versioned with the Rust workspace) or
drive the MCP server directly over STDIO. Once the npm token is rotated the
accumulated versions publish and this section gets rewritten.

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

The SDK exposes typed bindings for the MCP tools:

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
| `client.getMemoryProvenance(id)` | `provenance` |
| `client.writesByPrincipal(p)` / `client.writesBySession(s)` | `provenance` |
| `client.forgetByProvenance(...)` | `forget_by_provenance` |

Every input and response is typed (`RememberInput`, `RecallResponse`, etc.). Errors land as `MnemoToolError`, `MnemoRpcError`, or `MnemoConnectionError`.

### Write provenance & FORGET BY PROVENANCE

```typescript
// Who wrote this memory, under what authority?
const prov = await client.getMemoryProvenance(id);
// Everything a principal / session wrote (newest first)
await client.writesByPrincipal("alice");
await client.writesBySession("sess-42");
// FORGET BY PROVENANCE — revoke everything alice wrote (audit trail survives)
await client.forgetByProvenance({ principal: "alice", strategy: "hard_delete" });
```

Each provenance record carries a `flags` array. On `REMEMBER`, mnemo flags content
that has the **shape** of a provider-returned opaque reasoning payload
([arXiv:2608.09867](https://arxiv.org/abs/2608.09867)) with
`"opaque_reasoning_payload"` — such blocks have leaked credentials/PII in the wild,
and remembering a raw assistant turn can persist one. The write is **recorded, not
rejected** (warn-and-record), so you can find and revoke it later by principal or
session. **This detects a shape; it does not prove a secret is present, and mnemo
never decodes the content.**

```typescript
const prov = await client.getMemoryProvenance(id);
if (prov?.flags.includes("opaque_reasoning_payload")) {
  // review / revoke: client.forgetByProvenance({ session_id: prov.session_id, strategy: "redact" })
}
```

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
