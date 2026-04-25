# Python SDK

The Python SDK provides native access to Mnemo via PyO3 bindings, plus integrations for LangGraph, CrewAI, and OpenAI Agents SDK.

## Installation

```bash
pip install mnemo-db
```

> The PyPI distribution name is `mnemo-db` (not `mnemo`) because the
> unqualified name is held by an unrelated 2021 notebook project.
> The import path is unchanged — your code keeps saying `from mnemo
> import MnemoClient`.

With framework integrations:

```bash
pip install mnemo-db[langgraph]     # LangGraph checkpoint support
pip install mnemo-db[crewai]        # CrewAI memory integration
pip install mnemo-db[openai-agents] # OpenAI Agents SDK integration
```

## Basic Usage

```python
from mnemo import MnemoClient

client = MnemoClient(db_path="agent.db", agent_id="my-agent")

# Store a memory
result = client.remember("The user likes dark mode", importance=0.8)

# Recall memories
memories = client.recall("user preferences", limit=5)
for m in memories:
    print(f"{m['content']} (score: {m['score']:.2f})")

# Forget a memory
client.forget([result["id"]])
```

## OpenAI Agents SDK

```python
import asyncio
from agents import Agent, Runner
from mnemo.openai_agents import MnemoAgentMemory

async def main():
    async with MnemoAgentMemory(db_path="agent.db") as memory:
        agent = Agent(
            name="MemoryAgent",
            instructions="Use memory tools to remember and recall information.",
            mcp_servers=memory.mcp_servers,
        )
        result = await Runner.run(agent, "Remember that I prefer Python over JavaScript")
        print(result.final_output)

asyncio.run(main())
```

## LangGraph Checkpointer

```python
from mnemo import MnemoClient
from mnemo.checkpointer import ASMDCheckpointer

client = MnemoClient(db_path="agent.db")
checkpointer = ASMDCheckpointer(client)

# Use with LangGraph
from langgraph.graph import StateGraph
graph = StateGraph(...).compile(checkpointer=checkpointer)
```

## CrewAI Memory

```python
from mnemo.crewai_memory import ASMDMemory

memory = ASMDMemory(db_path="crew.db")
# Use with CrewAI agents
```

## Claude Agent SDK (0.2.0+)

Connects Mnemo to the `claude-agent-sdk` Python package used by Claude Opus
4.7's Auto Memory workflow. Exposes the MCP tool surface and optionally
materializes memories into Markdown files that Auto Memory reads and edits
directly; a `watchdog` observer persists those edits back into Mnemo.

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
        memory.materialize(query="recent work", limit=25)
        memory.watch()
        options = ClaudeAgentOptions(
            mcp_servers={"mnemo": memory.mcp_server_config},
            allowed_tools=["mcp__mnemo__recall", "mcp__mnemo__remember"],
        )
        async with ClaudeSDKClient(options=options) as client:
            await client.query("Summarize yesterday's work.")

asyncio.run(main())
```

Install with `pip install mnemo[claude]`.

## OpenAI Agents SDK — Session store (0.2.0+)

Implements the `SessionABC` protocol introduced in the 2026-04-15 release,
so conversation history is stored in Mnemo. Each turn becomes a
session-tagged episodic memory, so a new process can resume the conversation
by opening a store with the same `session_id`.

```python
import asyncio
from agents import Agent, Runner
from mnemo.openai_sessions import MnemoSessionStore

async def main():
    session = MnemoSessionStore(
        db_path="agent.mnemo.db",
        agent_id="user-42",
        session_id="support-2026-04-20",
    )
    agent = Agent(name="Support")
    result = await Runner.run(agent, "I can't log in", session=session)
    print(result.final_output)

asyncio.run(main())
```

## GDPR / DPDPA-safe erasure (0.2.0+)

Subject-scoped erasure through the engine, MCP, REST, or gRPC. Memories are
matched by the tag convention `subject:<subject_id>`. The default `redact`
strategy preserves the memory's hash chain (so audit verification still
succeeds) and replaces content with `[REDACTED]`.

```
# REST — redact
curl -X POST -H 'content-type: application/json' \
  -d '{"subject_id":"user-42","strategy":"redact"}' \
  http://localhost:8080/v1/forget_subject
```

To hard-delete instead, use `{"strategy": "hard_delete"}`.

## Ranking provenance (0.2.0+)

Pass `explain=True` to `recall` to receive a `score_breakdown` for each
result showing the per-signal contributions (vector, BM25, graph, recency)
and the final RRF rank.

```python
result = client.recall("alpha", explain=True, strategy="hybrid")
for memory in result["memories"]:
    bd = memory.get("score_breakdown")
    if bd:
        print(memory["content"], bd)
```

## TTL sweeper + point-in-time replay (0.2.0+)

The engine can run a background TTL sweeper that hard-deletes expired
memories and emits `MemoryExpired` audit events. Enable it via
`--ttl-sweep-interval` / `MNEMO_TTL_SWEEP_INTERVAL`.

`replay` accepts an `as_of` timestamp that synthesizes a virtual checkpoint
of the agent state at that instant:

```python
state = client.replay(
    thread_id="support-2026-04-20",
    as_of="2026-04-18T00:00:00Z",
)
```
