"""Example: Microsoft Semantic Kernel + Mnemo persistent memory.

Semantic Kernel agents connect to Mnemo via MCPStdioPlugin,
adding persistent memory as a kernel plugin.

Requirements:
    pip install semantic-kernel
    cargo build --release -p mnemo-cli
    export OPENAI_API_KEY=sk-...
"""

import asyncio

from semantic_kernel import Kernel
from semantic_kernel.connectors.ai.open_ai import OpenAIChatCompletion
from semantic_kernel.connectors.ai.function_choice_behavior import FunctionChoiceBehavior
from semantic_kernel.connectors.mcp import MCPStdioPlugin
from semantic_kernel.agents import ChatCompletionAgent


async def main():
    # Connect to Mnemo via MCP stdio plugin
    async with MCPStdioPlugin(
        name="mnemo",
        description="Persistent memory database for AI agents",
        command="mnemo",
        args=["--db-path", "sk_demo.db", "--agent-id", "sk-agent"],
    ) as mnemo_plugin:

        # Set up kernel with Mnemo plugin
        kernel = Kernel()
        service = OpenAIChatCompletion(service_id="chat")
        kernel.add_service(service)
        kernel.add_plugin(mnemo_plugin)

        # Enable auto function calling
        settings = kernel.get_prompt_execution_settings_from_service_id("chat")
        settings.function_choice_behavior = FunctionChoiceBehavior.Auto()

        # Create an agent with memory
        agent = ChatCompletionAgent(
            service_id="chat",
            name="MemoryAssistant",
            instructions=(
                "You are a helpful assistant with persistent memory.\n"
                "Use mnemo-remember to store important facts.\n"
                "Use mnemo-recall to retrieve relevant context.\n"
                "Use mnemo-forget to remove outdated information.\n"
                "Always check memory before answering questions."
            ),
            kernel=kernel,
        )

        # Session 1: Store knowledge
        print("=== Store Knowledge ===")
        response = await agent.get_response(
            messages="Remember that Alice is a .NET developer at Microsoft "
            "who prefers C# and uses Azure for deployments."
        )
        print(f"Agent: {response}\n")

        # Session 2: Recall context
        print("=== Recall Context ===")
        response = await agent.get_response(
            messages="What cloud platform does Alice use?"
        )
        print(f"Agent: {response}\n")

        # Session 3: Update knowledge
        print("=== Update Knowledge ===")
        response = await agent.get_response(
            messages="Alice has switched to AWS. Update memory accordingly."
        )
        print(f"Agent: {response}")


# Multi-agent group chat with shared memory
async def multi_agent():
    async with MCPStdioPlugin(
        name="mnemo",
        description="Shared memory",
        command="mnemo",
        args=["--db-path", "sk_demo.db"],
    ) as mnemo_plugin:
        from semantic_kernel.agents import AgentGroupChat

        kernel = Kernel()
        kernel.add_service(OpenAIChatCompletion(service_id="chat"))
        kernel.add_plugin(mnemo_plugin)

        researcher = ChatCompletionAgent(
            service_id="chat",
            name="Researcher",
            instructions="Research topics and store findings in memory.",
            kernel=kernel,
        )
        analyst = ChatCompletionAgent(
            service_id="chat",
            name="Analyst",
            instructions="Recall research from memory and provide analysis.",
            kernel=kernel,
        )

        chat = AgentGroupChat(agents=[researcher, analyst])
        # ... run the group chat


if __name__ == "__main__":
    asyncio.run(main())
