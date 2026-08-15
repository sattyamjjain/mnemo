# MCP 2026-07-28 readiness audit

**Status: map only. No migration code ships with this document.**

This is a per-item audit of what the 2026-07-28 MCP revision changes for *this*
server, written before any code moved so the next PR can be small and scoped.
Every claim below names the module it came from, and every "not-affected" is a
verified absence rather than an assumption.

**Audited at:** workspace `0.5.23`, `rmcp = { version = "3.0", features = ["server", "transport-io"] }`
(root `Cargo.toml`). The feature set matters more than anything else in this
document, and is the reason most items land on *not-affected* — see below.

| Item | Status |
|---|---|
| [Session IDs](#1-session-ids) | **needs-design** |
| [Roots](#2-roots) | **not-affected** |
| [Sampling](#3-sampling) | **not-affected** |
| [Logging](#4-logging) | **not-affected** |
| [DCR → CIMD](#5-dcr--cimd) | **needs-design** |

---

## The one fact that shapes all five

`rmcp` is compiled with `features = ["server", "transport-io"]`. There is no
`transport-streamable-http` and no SSE feature. The server is **stdio-only**:
`crates/mnemo-cli/src/main.rs` builds one `MnemoServer` and calls
`server.serve(stdio())` (both the normal and hardened boot paths do this).

So this server has no HTTP layer, and therefore no `Mcp-Session-Id`, no OAuth
handshake, and no client registration to migrate. Most of the 2026-07-28 changes
are about making an HTTP-transported server stateless — work that does not apply
to a process that is already one-client-per-process.

That is genuinely good news, and it is also the trap. **The shipped artifact is
mostly not-affected; the moment an HTTP transport is added, three of these items
become load-bearing at once.** The two `needs-design` items below are the ones
that must be answered *before* that transport lands, not after.

---

## 1. Session IDs

**Status: needs-design.**

### Where the server depends on transport-level session identity

Nowhere — there is no transport session to depend on. But the *role* a session
would play is currently played by **the OS process**, and several things are
bound to process lifetime rather than to a request:

- **`MnemoServer.engine.default_agent_id`** — set once at boot from
  `--agent-id` / `MNEMO_AGENT_ID` (`crates/mnemo-cli/src/main.rs`). It is the
  scoping key for resource listing:
  `crates/mnemo-mcp/src/server.rs::list_resources` builds
  `MemoryFilter { agent_id: Some(self.engine.default_agent_id.clone()), .. }`.
  Resource scoping is therefore **process-wide, not per-caller**.
- **The role filter** — `crates/mnemo-mcp/src/role_filter.rs`, attached once at
  boot before `serve(stdio())`; a denied tool is hidden from `tools/list` and
  rejected by `tools/call` with `-32601`. `main.rs` already documents this
  honestly as "a server-wide tool denylist on the stdio transport, not as
  per-caller RBAC."
- **Idle-timeout activity tracking** — `touch_activity()` on the server struct.

Note the name collision, because it is load-bearing: the `session_id` that
appears throughout `crates/mnemo-mcp/src/tools/provenance.rs`,
`mnemo_core::model::write_provenance::WriteProvenance`, and the
`mnemo.forget_by_provenance` tool is an **application-level, caller-supplied
trace id already persisted in the store**. It is not an MCP transport session and
is unaffected by this revision. If anything it is the model to copy.

### What the stateless replacement is

State relocates to the store. The primitives already exist and are already
persisted:

- `mnemo_core::model::write_provenance::WriteProvenance` — `principal`,
  `capability_id`, `session_id`, `op`, timestamp, hash-chained.
- `mnemo_core::model::capability::{Capability, CapabilityIssuer}` — HMAC-signed
  `{ principal, scope, expiry }`, with `remember_with_capability` verifying
  before writing.
- `mnemo_core::model::acl` and `mnemo_core::auth` — the per-record permission
  and bearer-token checks.

So the migration is not "invent a session store". It is: stop deriving identity
from the process, and start deriving it per request from a verified capability,
then feed that principal into the filters that today read `default_agent_id`.

### The honest part

Today the OS process **is** the security boundary. One client per process, a
static tool denylist, and a parent-process gauntlet
(`crates/mnemo-cli/src/safe_spawn.rs`) that refuses untrusted launchers. That
boundary does real work, and a stateless HTTP transport **deletes it entirely**.

Once identity is per-request, nothing is left but what the store itself
enforces — **the auth boundary on the store becomes the security model, whole
and alone.** Concretely, that promotes three things from optional to mandatory:

1. `default_agent_id` must become a request-derived principal, or
   `list_resources` will serve one agent's memories to every caller.
2. The `role_filter` denylist must become per-principal RBAC, or every caller
   gets the same tool surface.
3. Capability verification must move from opt-in (`remember_with_capability`) to
   the default path for every mutating tool.

None of that is a mechanical edit, which is why this is **needs-design** and not
**needs-change**. The design question to answer first: *does a capability travel
per-request, or does the server resolve a principal once per connection and
cache it?* Every other decision here follows from that one.

---

## 2. Roots

**Status: not-affected.**

**Consumes:** no. The server never issues a `roots/list` request to the client.
`roots` does not appear anywhere in `crates/mnemo-mcp/src/`.

**Advertises:** no. `get_info` in `crates/mnemo-mcp/src/server.rs` declares
exactly `ServerCapabilities::builder().enable_tools().enable_resources().build()`.

### What filesystem scoping replaces it

Nothing needs to — **this server has no client-scoped filesystem surface.**
Its scoping axes are all store-side: `agent_id`, `org_id`, ACL entries, and the
single database named by `MNEMO_DB_PATH` or `MNEMO_POSTGRES_URL`. The MCP
`resources/*` surface is not files: `list_resources` returns
`mnemo-memory://<uuid>` URIs backed by rows, not paths.

Two near-misses, both checked and both genuinely out of scope:

- `MNEMO_ONNX_MODEL_PATH` is a real filesystem path, but it is operator
  configuration read at boot — never client-supplied, never client-scoped.
- `mnemo-md-sync` does sync a git-tracked Markdown tree, which *would* be a roots
  candidate. It is **not reachable from the MCP server**: `crates/mnemo-mcp`
  depends only on `mnemo-core`, `mnemo-compliance`, `mnemo-attention-state`, and
  `rmcp`. If `mnemo-md-sync` is ever exposed as a tool, re-open this item — that
  is the one change that would move it to **needs-design**.

---

## 3. Sampling

**Status: not-affected.**

The server never asks the client to run an LLM turn. No `sampling`,
`create_message`, or `createMessage` reference exists in
`crates/mnemo-mcp/src/`, and sampling is not among the declared capabilities.

The one place that used to look like an exception no longer exists.
`mnemo_graph` once carried a `TemporalEdge::extract` stub documenting a
`MNEMO_GRAPH_EXTRACT_MODEL` env var it never read; it was removed in #156 and
the crate is now explicitly a bitemporal **storage + query** layer with no LLM in
it. `mnemo-graph` is not a dependency of `mnemo-mcp` regardless.

Worth stating for the record: mnemo is a store, not an agent. It has no reason to
request model turns from its client, so this item is expected to stay
*not-affected* permanently rather than by accident.

---

## 4. Logging

**Status: not-affected.**

The server implements no MCP logging: no `logging/setLevel` handler, no
`LoggingLevel` reference in `crates/mnemo-mcp/src/`, and `logging` is absent from
the declared capabilities. Clients cannot set a log level, by omission.

Diagnostics go to **stderr** via `tracing`. On a stdio transport that is the only
correct channel — stdout carries the JSON-RPC framing, and anything written there
corrupts the protocol. That constraint is a property of stdio, not of the MCP
revision, so it survives the migration unchanged.

If an HTTP transport is added, stderr stops being a per-client channel and MCP
`logging` becomes the only way to get diagnostics back to a specific caller. That
is a *new feature* at that point, not a migration of something existing, which is
why this stays **not-affected** rather than **needs-design**.

---

## 5. DCR → CIMD

**Status: needs-design.**

### What the server does today

No client registration of any kind. There is no OAuth flow, no `client_id`, no
`registration_endpoint`, and no `.well-known` document served by the MCP server.
(Two `.well-known` paths do exist in the workspace —
`mnemo-deal`'s `/.well-known/mnemo-deal-agent.json` and `mnemo-amp`'s
`.well-known/amp-schema.json` — but neither is MCP client registration, and
neither crate is reachable from `mnemo-mcp`.)

On stdio there is no client to register. The trust decision is "the operator
launched this binary", enforced before any engine state is touched by
`crates/mnemo-cli/src/safe_spawn.rs`: a parent-process gauntlet plus
`MNEMO_REJECT_INHERITED_SECRETS` and `MNEMO_PARENT_BASENAME`.

Because DCR was never implemented, **there is nothing to migrate away from.**
The work is greenfield, and it only becomes necessary alongside an HTTP
transport.

### What a CIMD-hosted metadata document would need to contain

At minimum, for the server to make an authorization decision about a caller:

- `client_id` — under CIMD this *is* the document's URL
- `client_name`, and `logo_uri` / `policy_uri` / `tos_uri` for anything that
  surfaces a consent screen
- `redirect_uris` and `grant_types`
- `token_endpoint_auth_method`
- `scope` — the requested scopes, which for mnemo must map onto the existing
  `Capability { principal, scope, expiry }` scope vocabulary rather than a
  parallel one
- `jwks` or `jwks_uri` — the keys used to authenticate the client
- `software_id` / `software_version`, `contacts`

### The part that is a runtime problem, not a config problem

**CIMD relocates the trust anchor to a URL the client hosts.** Under DCR the
server minted the registration and held it; under CIMD the server *fetches* the
client's self-published identity and must decide how much to believe it, forever.

That makes two things runtime concerns that did not exist before:

- **Freshness.** The document can change after first fetch — new keys, new
  redirect URIs, a different name. The server needs an explicit fetch/cache/TTL
  policy and a re-validation point. A cached-forever document is a permanent
  grant of whatever it said the day it was read.
- **Revocation.** A client that should lose access is revoked by editing or
  removing a document the *client* controls. The server needs a revocation
  signal it actually trusts, and a decision for the unreachable case: **fail
  closed** (reject when the document cannot be fetched, and accept that a client
  outage becomes an auth outage) or **serve stale** (accept the last known good
  document, and accept that revocation is best-effort). These are not equivalent,
  and the choice cannot be deferred to implementation time.

For a memory server the stakes are concrete: a stale CIMD document is a live
credential against stored memories. Given mnemo already has an HMAC capability
primitive with a real `expiry`, the likely design is to treat CIMD as an identity
*claim* that must be exchanged for a short-lived `Capability` — keeping the
authoritative, revocable, expiring token store-side where the rest of the
security model already lives. That is the proposal to evaluate, not a decision.

---

## What the next PR should do

Nothing in this document is implemented. In dependency order:

1. Decide the per-request-vs-per-connection identity question in §1. Everything
   else is downstream of it.
2. Make `default_agent_id` and the `role_filter` denylist principal-derived
   rather than boot-derived — the two concrete code sites, both named above.
3. Only then consider an HTTP transport, and only then does §5 become real.

Items §2, §3 and §4 need no work and should be re-checked, not re-designed. The
re-check is cheap and mechanical: `roots`, `sampling`, and `LoggingLevel` must
stay absent from `crates/mnemo-mcp/src/`, and `mnemo-mcp`'s dependency list must
stay free of `mnemo-md-sync`.
