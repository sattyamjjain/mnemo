# `mnemo-mesh`

SPIFFE-style identity and per-namespace ACL for Mnemo agents, speaking the lifecycle-attestation envelope Cloudflare Mesh expects.

Part of [Mnemo](https://github.com/sattyamjjain/mnemo), an MCP-native memory database for AI agents.

> ### Standalone adapter: the running `mnemo` server does not call this crate
>
> This is a **library you drive yourself**. Nothing in the `mnemo` MCP server
> invokes it: starting `mnemo-mcp-server` does not load, initialise or execute
> any code here. Installing it alongside the server changes the server's
> behaviour in no way.
>
> That is a deliberate boundary, not an oversight, but it is easy to miss if you
> found this crate on crates.io rather than through the workspace README, where
> it is listed as *"standalone adapter crate, not invoked by the running
> server"*. If you want this behaviour, you have to write the code that calls
> it.

## What it is

Cloudflare Mesh defines a lifecycle-attestation envelope: every workload presents
a SPIFFE-style identity plus an attestation token, and every privileged operation
carries an audit envelope back to a chained ledger. This crate lets a
Mesh-deployed agent use Mnemo as its memory plane without breaking that chain.

- `identity::MeshIdentity` — the `(workload_spiffe_id, attestation_token)` pair a
  caller presents on every operation.
- Per-namespace ACL enforcement over a `MnemoEngine`.
- Attestation envelopes that chain into Mnemo's existing audit log.

## Using it

Add it alongside `mnemo-core` and call it from your own code:

```toml
[dependencies]
mnemo-core = "0.5"
mnemo-mesh = "0.5"
```

## License

Apache-2.0.
