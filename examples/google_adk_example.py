"""Example: Google ADK (Agent Development Kit) + Mnemo persistent memory.

Google ADK agents connect to Mnemo via McpToolset, gaining access
to all 10 memory tools through MCP stdio transport.

Requirements:
    pip install google-adk
    cargo build --release -p mnemo-cli
    export OPENAI_API_KEY=sk-...
"""

from google.adk.agents import LlmAgent
from google.adk.runners import InMemoryRunner
from google.adk.tools.mcp_tool.mcp_toolset import McpToolset
from google.adk.tools.mcp_tool.mcp_session_manager import StdioConnectionParams
from google.genai import types as genai_types
from mcp.client.stdio import StdioServerParameters

# Configure Mnemo MCP toolset
mnemo_toolset = McpToolset(
    connection_params=StdioConnectionParams(
        server_params=StdioServerParameters(
            command="mnemo",
            args=["--db-path", "adk_demo.db", "--agent-id", "google-agent"],
        )
    ),
)

# Create the agent
agent = LlmAgent(
    model="gemini-2.0-flash",
    name="memory_assistant",
    instruction=(
        "You are a helpful assistant with persistent memory.\n"
        "Use mnemo.remember to store facts the user shares.\n"
        "Use mnemo.recall to look up relevant context.\n"
        "Use mnemo.forget to remove outdated information.\n"
        "Always check memory before answering questions about the user."
    ),
    tools=[mnemo_toolset],
)

# Run the agent
runner = InMemoryRunner(agent=agent)

user_id = "user_alice"
session_id = "session_001"


def run_turn(message: str):
    """Send a message and print the agent's response."""
    user_message = genai_types.Content(
        role="user",
        parts=[genai_types.Part(text=message)],
    )
    for event in runner.run(
        user_id=user_id,
        session_id=session_id,
        new_message=user_message,
    ):
        if event.content and event.content.parts:
            text = event.content.parts[0].text
            if text:
                print(f"[{event.author}]: {text}")


if __name__ == "__main__":
    print("=== Store Knowledge ===")
    run_turn("Remember that I'm Alice, a Python developer who prefers dark mode.")

    print("\n=== Recall Knowledge ===")
    run_turn("What do you remember about my preferences?")

    print("\n=== Update Knowledge ===")
    run_turn("Actually, I've switched to light mode. Update that.")
