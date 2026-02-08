"""DSPy tool integration for Mnemo.

Provides tool functions that can be used with DSPy's ReAct module,
allowing DSPy agents to interact with Mnemo's memory system.

Example::

    import dspy
    from mnemo.dspy_tools import create_mnemo_tools

    dspy.configure(lm=dspy.LM("openai/gpt-4o"))
    tools = create_mnemo_tools(db_path="agent.db")

    agent = dspy.ReAct("question -> answer", tools=tools)
    result = agent(question="What do you remember about the user?")

Requires:
    pip install dspy mnemo
"""

from __future__ import annotations

from typing import Optional

from mnemo import MnemoClient


def create_mnemo_tools(
    db_path: str = "mnemo.db",
    agent_id: str = "default",
    **kwargs,
) -> list:
    """Create DSPy-compatible tool functions backed by Mnemo.

    Returns a list of plain Python functions that DSPy's ReAct module
    can use as tools. Each function has a docstring that the LM uses
    to decide when and how to call it.

    Args:
        db_path: Path to the Mnemo database file.
        agent_id: Default agent identifier.
        **kwargs: Additional arguments passed to MnemoClient.

    Returns:
        List of tool functions for dspy.ReAct.
    """
    client = MnemoClient(db_path=db_path, agent_id=agent_id, **kwargs)

    def remember_memory(
        content: str,
        tags: Optional[str] = None,
        importance: Optional[float] = None,
    ) -> str:
        """Store a piece of information in persistent memory for later retrieval.

        Args:
            content: The information to remember.
            tags: Comma-separated tags for categorization.
            importance: Importance score from 0.0 to 1.0.

        Returns:
            Confirmation with the memory ID.
        """
        tag_list = [t.strip() for t in tags.split(",")] if tags else None
        result = client.remember(
            content=content,
            tags=tag_list,
            importance=importance,
        )
        return f"Stored memory with ID: {result['id']}"

    def recall_memories(query: str, limit: int = 5) -> str:
        """Search persistent memory for relevant information.

        Args:
            query: Natural language search query.
            limit: Maximum number of results to return.

        Returns:
            Matching memories as a formatted string.
        """
        result = client.recall(query=query, limit=limit)
        memories = result.get("memories", [])
        if not memories:
            return "No memories found matching the query."
        lines = []
        for m in memories:
            score = m.get("score", 0.0)
            content = m.get("content", "")
            lines.append(f"[{score:.2f}] {content}")
        return "\n".join(lines)

    def forget_memory(memory_id: str) -> str:
        """Remove a specific memory by its ID.

        Args:
            memory_id: UUID of the memory to forget.

        Returns:
            Confirmation of deletion.
        """
        result = client.forget([memory_id])
        forgotten = result.get("forgotten", [])
        if forgotten:
            return f"Forgot memory: {forgotten[0]}"
        return "Memory not found or already forgotten."

    return [remember_memory, recall_memories, forget_memory]
