# Python SDK

The Python SDK provides native access to Mnemo via PyO3 bindings, plus integrations for LangGraph, CrewAI, and OpenAI Agents SDK.

## Installation

```bash
pip install mnemo
```

With framework integrations:

```bash
pip install mnemo[langgraph]     # LangGraph checkpoint support
pip install mnemo[crewai]        # CrewAI memory integration
pip install mnemo[openai-agents] # OpenAI Agents SDK integration
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
