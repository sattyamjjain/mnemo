"""Example: CAMEL AI + Mnemo persistent memory.

CAMEL AI agents connect to Mnemo via FunctionTool wrappers,
providing persistent memory for single agents and role-playing societies.

Requirements:
    pip install 'camel-ai[all]' mnemo
    export OPENAI_API_KEY=sk-...
"""

from camel.agents import ChatAgent
from camel.models import ModelFactory
from camel.toolkits import FunctionTool
from camel.types import ModelPlatformType, ModelType
from typing import Optional

from mnemo import MnemoClient

# Initialize Mnemo client
client = MnemoClient(db_path="camel_demo.db", agent_id="camel-agent")


# Define memory tool functions
def remember(content: str, tags: Optional[str] = None, importance: float = 0.5) -> str:
    """Store information in persistent memory.

    Args:
        content: The information to remember.
        tags: Comma-separated tags for categorization.
        importance: Importance score from 0.0 to 1.0.

    Returns:
        Confirmation with memory ID.
    """
    tag_list = [t.strip() for t in tags.split(",")] if tags else None
    result = client.remember(content=content, tags=tag_list, importance=importance)
    return f"Stored memory: {result['id']}"


def recall(query: str, limit: int = 5) -> str:
    """Search persistent memory for relevant information.

    Args:
        query: Natural language search query.
        limit: Maximum number of results.

    Returns:
        Matching memories.
    """
    result = client.recall(query=query, limit=limit)
    memories = result.get("memories", [])
    if not memories:
        return "No memories found."
    return "\n".join(
        f"[{m.get('score', 0):.2f}] {m.get('content', '')}" for m in memories
    )


def forget(memory_id: str) -> str:
    """Remove a specific memory by ID.

    Args:
        memory_id: UUID of the memory to forget.

    Returns:
        Confirmation.
    """
    result = client.forget([memory_id])
    return f"Forgot: {result.get('forgotten', [])}"


def single_agent():
    """Single CAMEL agent with persistent memory."""
    model = ModelFactory.create(
        model_platform=ModelPlatformType.OPENAI,
        model_type=ModelType.GPT_4O,
    )

    # Wrap functions as CAMEL FunctionTools
    tools = [
        FunctionTool(remember),
        FunctionTool(recall),
        FunctionTool(forget),
    ]

    agent = ChatAgent(
        model=model,
        tools=tools,
        system_message=(
            "You are a helpful assistant with persistent memory. "
            "Use remember() to store facts, recall() to search, "
            "and forget() to remove outdated information."
        ),
    )

    # Store knowledge
    print("=== Store Knowledge ===")
    response = agent.step(
        "Remember that Alice is a researcher working on NLP "
        "and her paper deadline is March 15th."
    )
    print(f"Agent: {response.msg.content}\n")

    # Recall knowledge
    print("=== Recall Knowledge ===")
    response = agent.step("What do you know about Alice's deadline?")
    print(f"Agent: {response.msg.content}")


def role_playing_with_memory():
    """Two CAMEL agents sharing memory via role-playing."""
    from camel.societies import RolePlaying

    # Both agents share the same Mnemo database
    role_play = RolePlaying(
        assistant_role_name="Research Assistant with Memory",
        user_role_name="Project Manager",
        task_prompt=(
            "Research AI agent memory systems and store key findings. "
            "The assistant should use remember() to save findings and "
            "recall() to retrieve them when asked."
        ),
        model_platform=ModelPlatformType.OPENAI,
        model_type=ModelType.GPT_4O,
    )

    print("\n=== Role-Playing with Shared Memory ===")
    for i in range(5):
        assistant_response, user_response = role_play.step()
        print(f"\n[Turn {i+1}]")
        print(f"  Manager: {user_response.msg.content[:100]}...")
        print(f"  Assistant: {assistant_response.msg.content[:100]}...")
        if assistant_response.terminated or user_response.terminated:
            break


if __name__ == "__main__":
    single_agent()
