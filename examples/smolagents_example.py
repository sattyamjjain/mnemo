"""Example: Hugging Face smolagents + Mnemo persistent memory.

Smolagents CodeAgent/ToolCallingAgent connects to Mnemo via
ToolCollection.from_mcp, providing persistent memory for code agents.

Requirements:
    pip install 'smolagents[mcp,openai]'
    cargo build --release -p mnemo-cli
    export OPENAI_API_KEY=sk-...
"""

import os

from smolagents import CodeAgent, ToolCallingAgent, ToolCollection, OpenAIServerModel
from mcp import StdioServerParameters

# Configure Mnemo MCP connection
server_params = StdioServerParameters(
    command="mnemo",
    args=["--db-path", "smolagents_demo.db", "--agent-id", "smolagent"],
    env={**os.environ},
)

model = OpenAIServerModel(model_id="gpt-4o")


def code_agent_example():
    """CodeAgent writes Python code to call memory tools."""
    print("=== CodeAgent with Memory ===")

    with ToolCollection.from_mcp(
        server_params,
        trust_remote_code=True,
    ) as tool_collection:
        agent = CodeAgent(
            tools=[*tool_collection.tools],
            model=model,
            add_base_tools=True,
        )

        # The agent writes Python code to call the tools
        agent.run(
            "Store these facts in memory:\n"
            "1. The user prefers Python over JavaScript\n"
            "2. The project deadline is March 15th\n"
            "Then recall all stored memories to confirm."
        )


def tool_calling_agent_example():
    """ToolCallingAgent uses JSON tool calling for memory operations."""
    print("\n=== ToolCallingAgent with Memory ===")

    with ToolCollection.from_mcp(
        server_params,
        trust_remote_code=True,
    ) as tool_collection:
        agent = ToolCallingAgent(
            tools=[*tool_collection.tools],
            model=model,
        )

        # Store knowledge
        result = agent.run("Remember that Alice works at TechCorp as a data scientist.")
        print(f"Store result: {result}\n")

        # Recall knowledge
        result = agent.run("What do you know about Alice's job?")
        print(f"Recall result: {result}")


if __name__ == "__main__":
    code_agent_example()
    tool_calling_agent_example()
