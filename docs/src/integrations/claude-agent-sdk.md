# Claude Agent SDK integration

Mnemo integrates with Anthropic's `claude-agent-sdk` two ways at once:

1. **MCP tool surface** — every Mnemo MCP tool
   (`remember` / `recall` / `forget` / `share` / `checkpoint` / `branch` /
   `merge` / `replay` / `delegate` / `verify` / `forget_subject` / `reflect`)
   is exposed to the agent through the standard
   `ClaudeAgentOptions.mcp_servers` parameter.

2. **Memory-file bridge** — recalled memories are materialised as
   Markdown files on disk with YAML frontmatter. Claude Opus 4.7's Auto
   Memory reads and edits those files directly; a `watchdog` observer
   picks up the edits and persists them back into Mnemo so the two
   views stay in sync.

Install with `pip install mnemo[claude]`.

## Minimal example

```python
import asyncio
from pathlib import Path
from claude_agent_sdk import ClaudeSDKClient, ClaudeAgentOptions
from mnemo.claude_agent_sdk import MnemoClaudeMemory

async def main():
    async with MnemoClaudeMemory(
        db_path="agent.mnemo.db",
        agent_id="my-project",
        memory_dir=Path(".claude/memory"),
    ) as memory:
        # Seed the memory directory from Mnemo so Auto Memory has context.
        memory.materialize(query="recent work", limit=25)
        # Start watching for Auto Dream / Auto Memory edits.
        memory.watch()

        options = ClaudeAgentOptions(
            mcp_servers={"mnemo": memory.mcp_server_config},
            allowed_tools=[
                "mcp__mnemo__recall",
                "mcp__mnemo__remember",
            ],
        )
        async with ClaudeSDKClient(options=options) as client:
            await client.query("Summarize what I worked on yesterday.")

asyncio.run(main())
```

## What the bridge does to each file

* **Write from Mnemo** — `materialize(...)` writes
  `{memory_dir}/{uuid}.md` with frontmatter

  ```
  ---
  id: 0195a7e8-...
  importance: 0.7
  tags: ["decision", "roadmap"]
  expires_at: 2026-05-20T00:00:00Z
  ---
  The team agreed to ship v0.3.1 on 2026-04-22.
  ```

* **Edit by Opus 4.7 Auto Memory / Auto Dream** — the watchdog observer
  detects an edit, parses the (possibly rewritten) frontmatter, calls
  `engine.remember(...)` with the new content and importance, and the
  reflection pass (`mnemo.reflect`) then picks up `metadata.dreamed_at`
  markers so they don't get double-consolidated.

## Auto Dream coordination

v0.3.1 adds `ReflectionMode::Coordinated` which honours the same cadence
Auto Dream does: skip when fewer than 5 new records have accumulated or
fewer than 24 h have elapsed since the last successful pass. Run via:

```python
# The engine is exposed indirectly through the MCP tool surface; the
# Python bridge also owns a MnemoClient instance you can drive directly:
client = memory._ensure_client()
# Coordinated is the default; pass force=True to override.
client.reflect(mode="coordinated")
```

Parse the Auto Dream "organization report" trailer (the markdown block
with `Consolidated: N / Removed: M / Reindexed: K`) automatically —
`mnemo.reflect` emits a `dream_report_ingested` audit event per memory
containing one and marks the record so subsequent passes are no-ops.

## Caveats (v0.3.1)

* Python `MnemoClient` currently does not attach a full-text index, so
  `lexical` and `hybrid_rrf` recall strategies return no results when
  driven from Python. Tracked; see `docs/benchmarks/2026-04-21-mnemo-v0.3.0.md`.
* If `OPENAI_API_KEY` is unset, `MnemoClient` falls back to
  `NoopEmbedding`. Semantic recall is then random. Set the key, or wait
  for the v0.3.x ONNX-embedding repair.
