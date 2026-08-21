# `mnemo-cma`

Compatibility shim for the Anthropic CMA-Memory Markdown filesystem-of-memory: mount, mirror and export it.

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

Anthropic's Context-Managed Agent (CMA) Memory models memory as a Markdown
filesystem tree at `<root>/.memory/` with a sibling `audit.jsonl`. This crate
mounts an existing tree so an operator can keep using the Anthropic SDK while
Mnemo provides the durability and the audit chain.

- The Markdown tree on disk, in read-through, write-through or mirror mode
  (`SyncMode`).
- A bridged audit log: every CMA write produces exactly one Mnemo `AuditEvent`
  whose `prev_hash` chains into the engine's existing hash chain.
- Byte-identical export back out, so adopting this is reversible and you are not
  locked in.

## Using it

```toml
[dependencies]
mnemo-core = "0.5"
mnemo-cma  = "0.5"
```

## License

Apache-2.0.
