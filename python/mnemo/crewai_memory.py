"""CrewAI memory backend integration for Mnemo.

Provides ASMDMemory that can be used as a shared memory backend
for CrewAI agents.

Usage::

    from mnemo.crewai_memory import ASMDMemory

    memory = ASMDMemory(db_path="crew.mnemo.db", scope="shared")
    memory.add("The client prefers Python over JavaScript")
    results = memory.search("client preferences")
"""

from __future__ import annotations

from typing import Any, Optional

from mnemo import MnemoClient


class ASMDMemory:
    """CrewAI-compatible shared memory backed by Mnemo.

    Provides add/search interface that maps to Mnemo's
    REMEMBER/RECALL operations with configurable scope.
    """

    def __init__(
        self,
        db_path: str = "mnemo.db",
        agent_id: str = "crewai",
        scope: str = "shared",
        **kwargs: Any,
    ) -> None:
        self.client = MnemoClient(db_path=db_path, agent_id=agent_id)
        self.scope = scope

    def add(
        self,
        content: str,
        tags: Optional[list[str]] = None,
        importance: Optional[float] = None,
        metadata: Optional[dict] = None,
        **kwargs: Any,
    ) -> dict:
        """Add a memory entry."""
        return self.client.remember(
            content=content,
            scope=self.scope,
            tags=tags,
            importance=importance,
            metadata=metadata,
        )

    def search(
        self,
        query: str,
        limit: int = 5,
        **kwargs: Any,
    ) -> list[dict]:
        """Search memories by semantic similarity."""
        result = self.client.recall(query=query, limit=limit)
        return result.get("memories", [])

    def reset(self) -> None:
        """Reset is a no-op — Mnemo persists across sessions by design."""
        pass
