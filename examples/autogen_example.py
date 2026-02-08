"""Example: Microsoft AutoGen + Mnemo persistent memory.

AutoGen agents connect to Mnemo via McpWorkbench, providing persistent
memory tools for single agents and multi-agent teams.

Requirements:
    pip install autogen-agentchat "autogen-ext[openai,mcp]"
    cargo build --release -p mnemo-cli
    export OPENAI_API_KEY=sk-...
"""

import asyncio

from autogen_agentchat.agents import AssistantAgent
from autogen_agentchat.teams import RoundRobinGroupChat
from autogen_agentchat.conditions import MaxMessageTermination
from autogen_agentchat.ui import Console
from autogen_ext.models.openai import OpenAIChatCompletionClient
from autogen_ext.tools.mcp import McpWorkbench, StdioServerParams

# Configure Mnemo MCP server
server_params = StdioServerParams(
    command="mnemo",
    args=["--db-path", "autogen_demo.db", "--agent-id", "autogen-agent"],
    read_timeout_seconds=60,
)


async def single_agent():
    """Single agent with persistent memory."""
    model = OpenAIChatCompletionClient(model="gpt-4o")

    async with McpWorkbench(server_params) as mnemo:
        agent = AssistantAgent(
            "memory_agent",
            model_client=model,
            workbench=mnemo,
            system_message=(
                "You are a helpful assistant with persistent memory.\n"
                "Use mnemo.remember to store important facts.\n"
                "Use mnemo.recall to retrieve relevant context.\n"
                "Always check memory before answering questions."
            ),
            reflect_on_tool_use=True,
        )

        # Store knowledge
        print("=== Store Knowledge ===")
        await Console(
            agent.run_stream(
                task="Remember that Alice is a Python developer at Acme Corp "
                "who prefers functional programming."
            )
        )

        # Recall knowledge
        print("\n=== Recall Knowledge ===")
        await Console(
            agent.run_stream(task="What do you know about Alice?")
        )


async def multi_agent_team():
    """Multi-agent team sharing persistent memory."""
    model = OpenAIChatCompletionClient(model="gpt-4o")

    async with McpWorkbench(server_params) as mnemo:
        researcher = AssistantAgent(
            "researcher",
            model_client=model,
            workbench=mnemo,
            system_message=(
                "You are a researcher. Store all findings in memory "
                "using mnemo.remember with relevant tags."
            ),
            description="Researches topics and stores findings.",
        )

        analyst = AssistantAgent(
            "analyst",
            model_client=model,
            workbench=mnemo,
            system_message=(
                "You are an analyst. Use mnemo.recall to retrieve "
                "research findings and provide analysis. Say DONE when finished."
            ),
            description="Analyzes research from memory.",
        )

        termination = MaxMessageTermination(max_messages=6)
        team = RoundRobinGroupChat(
            [researcher, analyst],
            termination_condition=termination,
        )

        print("=== Multi-Agent Team ===")
        await Console(
            team.run_stream(
                task="Research the state of AI agent memory systems. "
                "Store findings, then analyze them."
            )
        )


if __name__ == "__main__":
    asyncio.run(single_agent())
