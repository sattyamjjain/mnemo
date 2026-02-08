"""Example: Pydantic AI + Mnemo persistent memory.

Pydantic AI agents connect to Mnemo via MCPServerStdio toolset,
adding type-safe persistent memory to any Pydantic AI agent.

Requirements:
    pip install pydantic-ai
    cargo build --release -p mnemo-cli
    export OPENAI_API_KEY=sk-...
"""

import asyncio

from pydantic_ai import Agent
from pydantic_ai.mcp import MCPServerStdio

# Configure Mnemo as an MCP toolset
mnemo_server = MCPServerStdio(
    "mnemo",
    args=["--db-path", "pydantic_demo.db", "--agent-id", "pydantic-agent"],
    timeout=30,
)

# Create the agent with Mnemo memory tools
agent = Agent(
    "openai:gpt-4o",
    toolsets=[mnemo_server],
    system_prompt=(
        "You are a helpful assistant with persistent memory.\n"
        "Use mnemo.remember to store facts the user shares.\n"
        "Use mnemo.recall to retrieve relevant context before answering.\n"
        "Use mnemo.forget to remove outdated information."
    ),
)


async def main():
    # The agent context manager starts the MCP server subprocess
    async with agent:
        # Session 1: Store knowledge
        print("=== Store Knowledge ===")
        result = await agent.run(
            "Remember that I'm working on a machine learning project "
            "using PyTorch and the deadline is March 15th."
        )
        print(f"Agent: {result.output}\n")

        # Session 2: Recall and reason
        print("=== Recall and Reason ===")
        result = await agent.run(
            "What project am I working on and when is it due?"
        )
        print(f"Agent: {result.output}\n")

        # Session 3: Update
        print("=== Update Knowledge ===")
        result = await agent.run(
            "The deadline has been extended to April 1st. Update your memory."
        )
        print(f"Agent: {result.output}")


# Alternative: Using the Mnemo wrapper
async def with_wrapper():
    from mnemo.pydantic_ai_memory import MnemoPydanticToolset

    mnemo = MnemoPydanticToolset(db_path="pydantic_demo.db")
    server = mnemo.create_server()

    agent = Agent("openai:gpt-4o", toolsets=[server])
    async with agent:
        result = await agent.run("Remember that I prefer dark mode")
        print(result.output)


if __name__ == "__main__":
    asyncio.run(main())
