# `@mndfreek/mnemo-sdk`

TypeScript SDK for [Mnemo](https://github.com/sattyamjjain/mnemo) — an MCP-native memory database for AI agents.

```bash
npm install @mndfreek/mnemo-sdk
```

## Version & compatibility

This SDK is versioned **independently** of the Rust workspace. `npm install`
currently gives you **0.4.4** (latest on npm); the Rust workspace is on the
0.5.x line. The SDK is a thin **MCP-over-STDIO client** — it does not embed the
engine — so it targets the `mnemo` / `mnemo-mcp-server` binary's MCP **tool
surface** (the 23 registered tools) rather than a specific `mnemo-core`
version. For the documented tool set you need a **0.5.x** `mnemo-mcp-server`,
which `cargo install` cannot give you yet — see the warning below.

`package.json` is on **0.4.8**, ahead of npm's **0.4.4**: the automated `npm
publish` (0.4.5–0.4.8) has been failing on an invalid `NPM_TOKEN` — an operator
action, not a code bug. `npm-publish.yml` now fails fast with the exact fix
instead of a cryptic 404, and `scripts/check_version_drift.sh` guards the
`package.json` ↔ npm gap so a further bump-without-publish is caught. Once the
token is rotated, the accumulated versions publish.

## Quick start

The SDK speaks MCP over STDIO to a `mnemo` binary running on the same machine. If you don't have the binary yet:

```bash
cargo install mnemo-mcp-server   # Rust toolchain required
```

<!-- STALE-PUBLISH-NOTE(#140): remove in the PR that publishes mnemo-mcp-server >= 0.5.23 -->
> ⚠️ **That command currently resolves `0.4.4`, published 2026-05-18 — a binary
> old enough to predate most of the tool surface this SDK documents.** The
> publish walk is blocked on
> [#140](https://github.com/sattyamjjain/mnemo/issues/140). Build the current
> server from source instead:
>
> ```bash
> cargo build --release -p mnemo-mcp-server   # runs as `mnemo`
> ```

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
