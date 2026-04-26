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

## What this does NOT cover

- The capability-leased reads (B2 follow-up): `forget_subject` and
  `export_audit_log` will require a lease token issued by a recent
  `recall`. The store is allocated at startup; the MCP-tools-layer
  wiring lands in a follow-up PR.
- The DPDPA consent-token-per-write path (B4).
- The Letta-protocol-compat surface (B5).

For the threat model and the full design notes, see the rationale at
the top of `crates/mnemo-cli/src/safe_spawn.rs`.
