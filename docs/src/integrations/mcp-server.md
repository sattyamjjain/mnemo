# mnemo mcp-server — hardened MCP STDIO mode

> **v0.4.0-rc3, Task B2.** Defends against the OX-MCP "exfiltrate-then-act"
> disclosure (2026-04-24) by refusing inherited secrets, JSON-injection
> argv, and untrusted parent processes BEFORE any engine state is
> constructed.

## Why this exists

The default `mnemo` startup path is convenient: it reads `OPENAI_API_KEY`,
`MNEMO_ENCRYPTION_KEY`, `MNEMO_POSTGRES_URL`, and a stack of CLI flags
straight from the environment. That's fine for local development. It is
**not** fine when an attacker can spawn the binary inside someone else's
shell — the OX-MCP disclosure showed how a poisoned Claude Code session
can cause the host to exec an MCP server with the attacker's manifest
attached and the user's secrets visible.

`mnemo mcp-server --manifest <path>` is a hardened entry point with a
narrower trust boundary:

- All privileged knobs (keystore, audit log destination, allowed tools,
  allowed agents, allowed parents) live in a TOML manifest the
  operator controls.
- Sensitive env vars are an automatic refusal.
- `--config`-style argv injection is an automatic refusal.
- Non-TTY parents that aren't on the manifest's allow-list are an
  automatic refusal.

## The manifest

```toml
keystore_path     = "/etc/mnemo/keystore.toml"
audit_log_path    = "/var/log/mnemo/audit.jsonl"
allowed_tools     = ["mnemo.recall", "mnemo.verify"]
allowed_agents    = ["claude-prod"]
allowed_parents   = ["claude", "systemd"]
lease_ttl_seconds = 60
```

A full annotated example lives at
[`examples/mcp-server/manifest.toml`](https://github.com/sattyamjjain/mnemo/blob/main/examples/mcp-server/manifest.toml).

### Keystore

The manifest's `keystore_path` points at a chmod-restricted TOML file:

```toml
key_id  = "mnemo-prov-2026-04"
key_hex = "<64 hex chars / 32 bytes / openssl rand -hex 32>"
```

The hardened mode loads this file at startup and attaches a
`ProvenanceSigner` (B1) to the engine. Every
`recall(..., with_provenance=true)` returns a verifiable HMAC receipt.
Rotate by writing a new file with a fresh `key_id` and updating the
manifest.

## The safe-spawn gauntlet

Before constructing any engine state, the binary runs three checks:

1. **Inherited secrets.** Refuses if the env carries any of:
   `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `HF_TOKEN`,
   `AWS_SECRET_ACCESS_KEY`, `GITHUB_TOKEN`, `MNEMO_ENCRYPTION_KEY`. Run
   the binary through a secret-clearing wrapper (`env -i`, systemd
   `Environment=`). Override at your own risk:
   `MNEMO_REJECT_INHERITED_SECRETS=0`.
2. **Args-based config.** Refuses if argv contains `--config`,
   `--config-json`, `--inline-config`, `-c`, or `--secret` (in any
   `=value` form). All config must live in the manifest.
3. **Untrusted parent.** When stdin is **not** a TTY, the parent
   process basename (set via `MNEMO_PARENT_BASENAME`) must appear in
   `manifest.allowed_parents`. If the variable is unset the check is
   skipped — running interactively (TTY present) also lifts the check.

Each refusal exits non-zero with a stderr message that names the
violating key/arg/parent.

## Running it

```bash
# 1. Clear the env, set the parent assertion, pass the manifest.
env -i \
  PATH="$PATH" HOME="$HOME" \
  MNEMO_PARENT_BASENAME=systemd \
  mnemo mcp-server --manifest /etc/mnemo/manifest.toml
```

Under systemd:

```ini
[Service]
Type=simple
Environment=MNEMO_PARENT_BASENAME=systemd
ExecStart=/usr/local/bin/mnemo mcp-server --manifest /etc/mnemo/manifest.toml
ProtectSystem=strict
PrivateTmp=true
NoNewPrivileges=true
```

## Verifying it

A quick "does the gauntlet actually fire" smoke test:

```bash
ANTHROPIC_API_KEY=leak mnemo mcp-server --manifest /etc/mnemo/manifest.toml
# refused to start: inherited sensitive env var "ANTHROPIC_API_KEY" ...

mnemo mcp-server --manifest /etc/mnemo/manifest.toml --config-json '{}'
# refused to start: command-line carries config-style argument ...
```

The full integration suite that exercises every refusal path lives in
`crates/mnemo-cli/tests/safe_spawn_integration.rs`.

## Role-aware tool filter (v0.4.2 — A1)

Mnemo's MCP server aligns with the 2025-11-25
[MCP authorization spec](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)
role-based annotations. The manifest can declare an optional
`[role_filter]` block that gates `tools/list` (filters the advertised
catalog) and `tools/call` (denies disallowed calls with a spec-compliant
`-32601`).

```toml
[role_filter]
caller_roles = ["auditor"]
default      = "deny_all"

[role_filter.allow]
"mnemo.recall"   = ["auditor", "agent"]
"mnemo.verify"   = ["auditor"]
"mnemo.remember" = ["agent"]
"mnemo.forget"   = ["agent"]

[role_filter.deny]
"mnemo.delegate" = ["auditor"]
```

Rules:

- **Deny always wins.** A tool that appears in both `allow` and `deny`
  for the same role is denied.
- **`default = "allow_all"`** (the implicit default) lets any tool not
  named in `allow`/`deny` through. Use `deny_all` for a strict
  allow-list.
- **`caller_roles`** declares the role assignment the operator has made
  for the binary itself. In stdio transport this is the entire caller
  identity; in future HTTP transports it composes with roles inferred
  from the `Authorization` header.
- Every denied call emits an `McpRoleDenied { caller_id, tool_name,
  attempted_at, reason }` row to `audit_log_path`.
- **Omitting the block keeps pre-v0.4.2 behaviour byte-for-byte.**
  Every advertised tool stays reachable and no audit events are
  emitted.

The filter contract (`RoleFilter` trait + `ManifestRoleFilter` impl) is
public, so a custom filter can replace the manifest-driven default at
test time. See
[`crates/mnemo-mcp/src/role_filter.rs`](https://github.com/sattyamjjain/mnemo/blob/main/crates/mnemo-mcp/src/role_filter.rs)
and the three integration tests under
[`crates/mnemo-mcp/tests/`](https://github.com/sattyamjjain/mnemo/tree/main/crates/mnemo-mcp/tests)
(`role_filter_allow_deny.rs`, `role_filter_audit_event.rs`,
`role_filter_no_block_when_unset.rs`).

## What this does NOT cover

- The capability-leased reads (B2 follow-up): `forget_subject` and
  `export_audit_log` will require a lease token issued by a recent
  `recall`. The store is allocated at startup; the MCP-tools-layer
  wiring lands in a follow-up PR.
- The DPDPA consent-token-per-write path (B4).
- The Letta-protocol-compat surface (B5).
- Per-tool-method enforcement of the role filter at `tools/call`
  dispatch — the manifest schema, the filter trait/impl, and the
  audit emission are shipped in v0.4.2; threading the filter through
  every `MnemoServer` tool method body lands in v0.4.3 with the
  `mnemo-envelope` exporter.

For the threat model and the full design notes, see the rationale at
the top of `crates/mnemo-cli/src/safe_spawn.rs`.

## Compatibility note (v0.4.3 — U1)

The MCP wire-protocol version mnemo's server speaks (`2024-11-05`,
with the [2025-11-25 authorization spec](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)
layered on top) is **independent** of the client SDK version your
agent uses. SDK refreshes are common and don't require a mnemo-side
rev unless the spec itself moves.

The current [version-skew matrix](../../../docs/compat/version-skew-matrix.md)
tracks tested combinations of the four official client SDKs:

- `mcp-python` (refreshed 2026-05-01)
- `mcp-go` (refreshed 2026-05-01)
- `mcp-ruby` (refreshed 2026-05-02)
- `mcp-csharp` (refreshed 2026-05-02)

If your agent hits an SDK-side incompatibility, consult the matrix
first — most issues land on a row that documents which mnemo cut
shipped against that SDK pair. The matrix is enforced in CI by
`crates/mnemo-mcp/tests/sdk_matrix_doc_present.rs`, so the doc itself
cannot silently disappear ahead of an SDK-bump release.

## MCP 2026 Roadmap alignment (v0.4.4 — U1)

The [MCP 2026 Roadmap](https://blog.modelcontextprotocol.io/posts/2026-mcp-roadmap/)
(published 2026-03-09 by lead maintainer David Soria Parra)
reorganises the protocol's direction around four priority areas. The
honest mnemo stance against each is below — *spec-context anchor, not
compliance claim*.

| MCP 2026 priority | What it covers | mnemo stance |
|---|---|---|
| **Transport Evolution and Scalability** | Stateless `Streamable HTTP`, `.well-known` server-discovery metadata, multi-tenant gateway behavior | **Follower.** mnemo speaks MCP via the [`rmcp = "1.3"`](https://crates.io/crates/rmcp) workspace dep. SEPs land in `rmcp` first; mnemo upgrades when they're stable, not before. |
| **Agent Communication** | Tasks-primitive lifecycle gaps; agent ↔ agent semantics outside the tool/resource layer | **Observer.** mnemo's `mnemo.delegate` + ACL/permission model is the existing surface; further coupling to a Tasks primitive waits on the SEP outcome. |
| **Governance Maturation** | Contributor ladder + WG delegation for the spec itself | **Observer.** Not a downstream surface mnemo participates in; we follow the spec the WGs ship. |
| **Enterprise Readiness** | Audit trails, SSO-integrated auth, gateway behavior, configuration portability | **Aligned-by-design.** Operator-held HMAC keystore (`keystore_path` in the manifest), AES-256-GCM at-rest content encryption (`MNEMO_ENCRYPTION_KEY`), `mnemo-compliance` crate's [DPDPA](dpdpa-mannsetu.md) consent-token-per-write surface, dual DuckDB / PostgreSQL backend portability, and the role-aware tool filter (v0.4.2 §"Role-aware tool filter") together form the *attestable memory* layer regulated-workflow buyers can defend today — independent of any one cloud's audit boundary. |

The honest framing: mnemo claims **alignment-by-design with one of
four priorities**, not roadmap compliance. The other three priorities
are spec-evolution work where mnemo follows `rmcp`'s implementation
of the SEPs as they're written. Buyers reading the roadmap should
hear "mnemo's existing audit story already serves the Enterprise
Readiness ask," not "mnemo is MCP-2026-ready."
