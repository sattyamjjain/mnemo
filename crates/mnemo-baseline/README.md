# `mnemo-baseline`

Per-agent rolling behavioural baseline with OpenTelemetry and OCSF emitters, plus z-score and EWMA drift detection.

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

Agents emit logs but rarely a normalised behavioural baseline an SOC can alert
on. This crate builds one: a per-agent rolling profile (recall rate, write rate,
namespace fanout, tool mix, HMAC continuity) emitted in two canonical schemas.

- **OpenTelemetry semconv 1.31** — `agent.*` attributes on a span an existing
  OTel collector already ingests.
- **OCSF 1.4 Application Activity** — JSON a SIEM pipeline already understands.
- A z-score plus EWMA detector that flags drift.

**Anti-leak invariant:** emitted payloads never carry memory contents. Only
counts, rates and continuity flags leave the process.

## Using it

```toml
[dependencies]
mnemo-core     = "0.5"
mnemo-baseline = "0.5"
```

## License

Apache-2.0.
