# `mnemo-deal`

Chained-HMAC ledger of agent-on-agent deal envelopes, for Project-Deal-style agent commerce.

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

When one agent contracts another to perform a task, the buyer's host needs a
tamper-evident record of the agreed terms and the completion. This crate is that
substrate: a chained-HMAC log of `DealEnvelope`s, each signed in line with
Mnemo's memory-provenance chain, so an audit-log export emits one continuous
ledger rather than two disjoint ones.

- `envelope::DealEnvelope` — the minimal contract shape (who, what, when,
  `prev_hash`, `hmac`).
- A ledger that verifies end to end.
- Counterparty discovery and an **advisory** reputation score.

The reputation score is advisory only; see
[`docs/deal-reputation-threats.md`](https://github.com/sattyamjjain/mnemo/blob/main/docs/deal-reputation-threats.md)
before treating it as an authorisation input.

## Using it

```toml
[dependencies]
mnemo-core = "0.5"
mnemo-deal = "0.5"
```

## License

Apache-2.0.
