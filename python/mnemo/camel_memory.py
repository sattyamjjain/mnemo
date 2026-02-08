"""CAMEL AI integration for Mnemo.

Provides a wrapper that connects CAMEL AI agents to Mnemo's MCP server
via MCPToolkit, or directly via FunctionTool.

Example::

    from mnemo.camel_memory import create_mnemo_camel_tools

    tools = create_mnemo_camel_tools(db_path="agent.db")

    from camel.agents import ChatAgent
    from camel.models import ModelFactory
    from camel.types import ModelPlatformType, ModelType

    model = ModelFactory.create(
        model_platform=ModelPlatformType.OPENAI,
        model_type=ModelType.GPT_4O,
    )
    agent = ChatAgent(model=model, tools=tools)
    response = agent.step("Remember that the user prefers dark mode")

Requires:
    pip install 'camel-ai[all]'
"""

from __future__ import annotations

from typing import Optional

from mnemo import MnemoClient


def create_mnemo_camel_tools(
    db_path: str = "mnemo.db",
    agent_id: str = "default",
    **kwargs,
) -> list:
    """Create CAMEL AI FunctionTool instances backed by Mnemo.

    Args:
        db_path: Path to the Mnemo database file.
        agent_id: Default agent identifier.
        **kwargs: Additional arguments passed to MnemoClient.

    Returns:
        List of FunctionTool instances for CAMEL AI agents.
    """
    try:
        from camel.toolkits import FunctionTool
    except ImportError:
        raise ImportError(
            "camel-ai is required for create_mnemo_camel_tools. "
            "Install with: pip install 'camel-ai[all]'"
        )

    client = MnemoClient(db_path=db_path, agent_id=agent_id, **kwargs)

    def remember(
        content: str,
        tags: Optional[str] = None,
        importance: Optional[float] = None,
    ) -> str:
        """Store information in persistent memory for later retrieval.

        Args:
            content: The information to remember.
            tags: Comma-separated tags for categorization.
            importance: Importance score from 0.0 to 1.0.

        Returns:
            Confirmation with the memory ID.
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
            Matching memories as formatted text.
        """
        result = client.recall(query=query, limit=limit)
        memories = result.get("memories", [])
        if not memories:
            return "No memories found."
        return "\n".join(
            f"[{m.get('score', 0):.2f}] {m.get('content', '')}" for m in memories
        )

    def forget(memory_id: str) -> str:
        """Remove a specific memory by its ID.

        Args:
            memory_id: UUID of the memory to forget.

        Returns:
            Confirmation of deletion.
        """
        result = client.forget([memory_id])
        return f"Forgot: {result.get('forgotten', [])}"

    return [
        FunctionTool(remember),
        FunctionTool(recall),
        FunctionTool(forget),
    ]
