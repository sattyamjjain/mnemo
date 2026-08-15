# ADR 0002 — Request identity model for an authenticated MCP transport

- Status: **Accepted — implemented** for the MCP surface (`crates/mnemo-mcp/src/identity.rs`,
  wired in `server.rs` at `list_tools` / `call_tool` / `list_resources`)
- Date: 2026-08-15
- Tracking: [#126](https://github.com/sattyamjjain/mnemo/issues/126), [`docs/mcp-2026-07-28-migration.md`](../mcp-2026-07-28-migration.md) §1
- Related: [ADR 0001 — capability-leased reads](0001-capability-leased-reads.md)

## Why this ADR exists

Two separate pieces of work are blocked on the *same unanswered question*,
and neither can proceed until it is decided:

- **ADR 0001 / #126** defers capability-leased reads until "a multi-caller
  authenticated transport" exists — but never says what identity means on
  that transport.
- **The MCP 2026-07-28 migration** names §1 (session IDs) as the item
  everything else is downstream of, and its own "what the next PR should
  do" list opens with *"decide the per-request-vs-per-connection identity
  question."*

This ADR answers it, so both can move. It decides **one thing** and
deliberately does not design the transport itself.

## The question

When mnemo gains an authenticated, multi-caller transport, does a caller's
identity attach to:

- **(A) the connection** — authenticate once at handshake, resolve a
  principal, cache it for the connection's lifetime; or
- **(B) the request** — every JSON-RPC call carries its own verifiable
  credential, resolved independently?

## Decision

**(B) per-request identity, with a per-connection resolution *cache* that
is an optimisation only and never the source of truth.**

Concretely: every mutating call must carry a credential that verifies on
its own. A connection may memoise the *result* of verifying a credential,
but no call may be authorised solely because an earlier call on the same
connection was.

## Why

### 1. Per-connection identity re-creates the exact bug this repo keeps fixing

mnemo has now fixed the same defect three times: `role_filter` allocated
but not dispatched (#124), tool-catalog attestation claimed but not
served (v0.5.20), and `LeaseStore` allocated but never consumed (the
lease removal that produced ADR 0001). The shape is always *a boundary
that is established once and then assumed to still hold*.

Per-connection identity is that shape by construction. It establishes a
principal at handshake and every later call inherits it without
re-checking. That is precisely the pattern whose failure mode is silent.

### 2. It is the only model ADR 0001's threat survives

ADR 0001's whole argument against session tokens applies verbatim here.
A connection-scoped principal *is* a session token wearing different
clothes: acquire it once, and every downstream privileged call is
unlocked. The OX-MCP exfiltrate-then-act chain wants exactly that.

If identity is per-connection, capability-leased reads cannot be built on
top — the lease would bind an act to a read while the *identity* under
both remained a single long-lived grant. Choosing (A) would make #126
undeliverable in any form that addresses its own threat model.

### 3. The store is already the right place, and already has the parts

The migration doc's §1 conclusion is that stateless relocates state to the
store, and *"the auth boundary on the store becomes the security model,
whole and alone."* Per-request identity is what makes that sentence
implementable rather than aspirational. The primitives exist today:

| Need | Existing primitive |
|---|---|
| Verifiable per-request credential | `mnemo_core::model::capability::{Capability, CapabilityIssuer}` — HMAC-signed `{principal, scope, expiry}` |
| Who wrote what, under what authority | `mnemo_core::model::write_provenance::WriteProvenance` |
| Per-record permission | `mnemo_core::model::acl` |
| Principal checks | `mnemo_core::auth` |

Note `Capability` already carries an `expiry`. Per-request verification
turns that expiry into an enforced bound; per-connection caching would
turn it into a lower bound on how long a compromised grant stays live.

### 4. The cost is real but lands in the right place

Verifying an HMAC per request is cheap — microseconds, against a call
path that already does vector search and disk I/O. Per-connection caching
of the *resolution* (principal → ACL set) recovers whatever is left. The
expensive part of a request was never the auth check.

## How it was implemented

The ADR assumed this work was gated behind "an authenticated HTTP transport".
**It was not.** MCP requests carry a `_meta` object and rmcp surfaces it as
`RequestMetaObject` on *every* transport, stdio included — so a capability can
ride each individual request today, with no new transport.

- `crates/mnemo-mcp/src/identity.rs` — reads the capability from
  `_meta["dev.mnemo/capability"]`, verifies it with `CapabilityIssuer`
  (signature, expiry, key id), and returns a `CallerContext` whose `caller_id`
  is the capability's principal and whose roles are its `role:`-prefixed scope
  tokens.
- `server.rs` resolves the caller **once per request**, in `call_tool` (before
  the role gate, so gating uses the capability's roles), `list_tools` (so the
  catalog is per-caller), and `list_resources` (so reads are scoped to the
  caller, closing the read leak named below). The resolved `CallerContext` is
  injected into the request's `extensions`, and the tools that record authority
  (`mnemo.delegate`) or scope reads (`mnemo.trajectory_audit`) take it as an
  `Extension<CallerContext>` rather than reading a boot-time constant.
- **Fail-closed**: a capability that cannot be verified — no issuer configured,
  malformed, forged, expired, unknown key — is an error. It is *never*
  downgraded to the boot identity, which would hand a forgery the operator's
  authority. `no_rejection_path_ever_yields_the_boot_identity` pins this over
  every failure mode.
- **Backward compatible**: a request with no capability still resolves to the
  boot-derived identity, so existing stdio deployments are unchanged.

This satisfies #126's revisit gate ("distinct callers hold distinct
identities") without the transport — see
`crates/mnemo-mcp/tests/per_request_identity.rs`, which asserts two
capabilities resolve to two distinct callers on one server instance.

## What this changes in the code

Two concrete sites, both already named in the migration doc as boot-time
where they must become request-derived:

1. **`engine.default_agent_id`** — set once at boot from
   `--agent-id` / `MNEMO_AGENT_ID`, and used as the scoping key in
   `crates/mnemo-mcp/src/server.rs::list_resources`. Under (B) it becomes
   a per-request principal. Until then, on stdio, it stays boot-derived
   and correct, because one process *is* one caller.
2. **`crates/mnemo-mcp/src/role_filter.rs`** — a boot-time, server-wide
   tool denylist. Under (B) it becomes per-principal RBAC resolved from
   the request's capability.

**Both are now done, and — contrary to this ADR's original expectation —
both landed on the stdio transport**, because `_meta` carries a per-request
capability there too. `list_resources` scopes by the resolved caller and the
role filter gates on the capability's roles. What is unchanged is the
*capability-less* path: with no token presented, both still resolve to the
boot identity, exactly as before.

With both prerequisites met, **#126 is no longer gated on them.**

## Consequences

- **#126 becomes buildable** the moment an authenticated transport lands —
  its revisit gate ("distinct callers hold distinct identities") is
  satisfied by (B) and *not* by (A).
- **The migration's §1 is unblocked**; §2 (`default_agent_id` and
  `role_filter` becoming principal-derived) can be specified as a
  standalone PR that does not require the transport itself.
- **CIMD (§5) inherits a clean answer.** A client-hosted metadata document
  becomes an identity *claim* exchanged for a short-lived `Capability`,
  keeping the authoritative, revocable, expiring token store-side. That
  resolves the freshness/revocation problem the migration doc raises: the
  server never has to trust a cached copy of someone else's document to
  authorise a call, because the call carries its own capability.
- **Nothing changes today.** On stdio there is one caller and one process;
  this ADR does not alter shipped behaviour. It exists so the first PR
  that touches identity does not have to relitigate the question.

## What was rejected

**(A) per-connection identity.** Rejected on the grounds above: it is the
claimed-but-not-rechecked pattern this codebase has had to repair three
times, it is a session token by another name, and adopting it would make
ADR 0001 undeliverable against its own threat.

**A hybrid where reads are per-connection and writes are per-request.**
Rejected because ADR 0001's threat begins with a *read* — the exfiltrate
half of exfiltrate-then-act. Weakening read identity is weakening the step
the lease design exists to bind.
