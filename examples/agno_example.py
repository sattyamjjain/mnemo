"""Example: Agno (formerly PhiData) + Mnemo persistent memory.

Agno agents connect to Mnemo via MCPTools with stdio transport,
providing persistent memory across sessions.

Requirements:
    pip install agno
    cargo build --release -p mnemo-cli
    export OPENAI_API_KEY=sk-...
"""

import asyncio

from agno.agent import Agent
from agno.models.openai import OpenAIChat
from agno.tools.mcp import MCPTools
from mcp import StdioServerParameters

# Configure Mnemo MCP connection
server_params = StdioServerParameters(
    command="mnemo",
    args=["--db-path", "agno_demo.db", "--agent-id", "agno-agent"],
)


async def main():
    # Connect to Mnemo via MCP stdio transport
    async with MCPTools(server_params=server_params) as mnemo_tools:
        agent = Agent(
            model=OpenAIChat(id="gpt-4o"),
            tools=[mnemo_tools],
            instructions=[
                "You are a helpful assistant with persistent memory.",
                "Use mnemo.remember to store important facts.",
                "Use mnemo.recall to retrieve relevant context.",
                "Use mnemo.forget to remove outdated information.",
            ],
            markdown=True,
        )

        # Session 1: Store knowledge
        print("=== Store Knowledge ===")
        await agent.aprint_response(
            "Remember these facts about me: I'm a data scientist, "
            "I work at TechCorp, and I prefer Python over R.",
            stream=True,
        )

        # Session 2: Recall context
        print("\n=== Recall Context ===")
        await agent.aprint_response(
            "What programming language do I prefer?",
            stream=True,
        )

        # Session 3: Multi-hop recall
        print("\n=== Multi-hop Recall ===")
        await agent.aprint_response(
            "Based on what you know about me, suggest a good ML framework.",
            stream=True,
        )


# Multiple MCP servers with MultiMCPTools
async def with_multi_servers():
    from agno.tools.mcp import MultiMCPTools

    async with MultiMCPTools(
        ["mnemo --db-path agno_demo.db", "npx -y @modelcontextprotocol/server-filesystem ."]
    ) as tools:
        agent = Agent(
            model=OpenAIChat(id="gpt-4o"),
            tools=[tools],
            markdown=True,
        )
        await agent.aprint_response(
            "Read the README file and remember its key points.",
            stream=True,
        )


if __name__ == "__main__":
    asyncio.run(main())
