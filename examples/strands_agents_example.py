"""Example: AWS Strands Agents + Mnemo persistent memory.

Strands Agents connect to Mnemo via MCPClient with stdio transport,
providing persistent memory for AWS-ecosystem agents.

Requirements:
    pip install strands-agents strands-agents-tools
    cargo build --release -p mnemo-cli
    export OPENAI_API_KEY=sk-...
"""

from strands import Agent
from strands.tools.mcp import MCPClient
from mcp import stdio_client, StdioServerParameters

# Configure Mnemo MCP connection
mnemo_client = MCPClient(
    lambda: stdio_client(
        StdioServerParameters(
            command="mnemo",
            args=["--db-path", "strands_demo.db", "--agent-id", "strands-agent"],
        )
    )
)


def main():
    # Connect to Mnemo and create an agent with memory tools
    with mnemo_client:
        agent = Agent(
            tools=mnemo_client.list_tools_sync(),
            system_prompt=(
                "You are a helpful assistant with persistent memory.\n"
                "Use mnemo.remember to store important facts.\n"
                "Use mnemo.recall to retrieve relevant context.\n"
                "Use mnemo.forget to remove outdated information."
            ),
        )

        # Session 1: Store knowledge
        print("=== Store Knowledge ===")
        response = agent(
            "Remember that the user Alice is a senior engineer at AWS "
            "and she is working on a serverless application."
        )
        print(f"Agent: {response}\n")

        # Session 2: Recall context
        print("=== Recall Context ===")
        response = agent("What do you know about Alice's current project?")
        print(f"Agent: {response}\n")

        # Session 3: Multi-step
        print("=== Multi-step ===")
        response = agent(
            "Check memory for Alice's role, then remember that "
            "her project is due by end of Q1 2026."
        )
        print(f"Agent: {response}")


# Multiple MCP servers
def with_multiple_servers():
    aws_docs_client = MCPClient(
        lambda: stdio_client(
            StdioServerParameters(
                command="uvx",
                args=["awslabs.aws-documentation-mcp-server@latest"],
            )
        )
    )

    with mnemo_client, aws_docs_client:
        tools = mnemo_client.list_tools_sync() + aws_docs_client.list_tools_sync()
        agent = Agent(tools=tools)
        response = agent(
            "Look up AWS Lambda best practices and remember the key points."
        )
        print(response)


if __name__ == "__main__":
    main()
