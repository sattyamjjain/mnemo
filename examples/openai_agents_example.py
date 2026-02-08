"""Example: OpenAI Agents SDK + Mnemo persistent memory.

The OpenAI Agents SDK is stateless by design — Mnemo fills the persistent
memory gap via MCP, giving agents remember/recall/forget tools.

Requirements:
    pip install openai-agents
    cargo build --release -p mnemo-cli
    export OPENAI_API_KEY=sk-...
"""

import asyncio

from agents import Agent, Runner
from agents.mcp import MCPServerStdio


async def main():
    # Connect to Mnemo's MCP server via stdio
    async with MCPServerStdio(
        name="mnemo",
        params={
            "command": "mnemo",
            "args": ["--db-path", "openai_demo.db", "--agent-id", "assistant"],
        },
    ) as mnemo_server:

        # Create an agent with memory tools
        agent = Agent(
            name="ResearchAssistant",
            instructions=(
                "You are a research assistant with persistent memory.\n"
                "Use mnemo.remember to store important facts.\n"
                "Use mnemo.recall to retrieve relevant context.\n"
                "Use mnemo.forget to remove outdated information.\n"
                "Always check memory before answering questions."
            ),
            mcp_servers=[mnemo_server],
        )

        # Session 1: Store some knowledge
        print("=== Session 1: Learning ===")
        result = await Runner.run(
            agent,
            "Remember these facts: The user's name is Alice, she is a senior "
            "Python developer at Acme Corp, and she prefers functional programming.",
        )
        print(f"Agent: {result.final_output}\n")

        # Session 2: Recall context
        print("=== Session 2: Recall ===")
        result = await Runner.run(
            agent,
            "What do you know about the user's programming preferences?",
        )
        print(f"Agent: {result.final_output}\n")

        # Session 3: Update knowledge
        print("=== Session 3: Update ===")
        result = await Runner.run(
            agent,
            "The user has switched to Rust as her primary language. "
            "Update the memory accordingly.",
        )
        print(f"Agent: {result.final_output}")


# Alternative: Using the Mnemo wrapper class
async def with_wrapper():
    from mnemo.openai_agents import MnemoAgentMemory

    async with MnemoAgentMemory(db_path="openai_demo.db") as memory:
        agent = Agent(
            name="MemoryAgent",
            instructions="You have persistent memory. Use it.",
            mcp_servers=memory.mcp_servers,
        )
        result = await Runner.run(agent, "Remember that I like dark mode")
        print(result.final_output)


if __name__ == "__main__":
    asyncio.run(main())
