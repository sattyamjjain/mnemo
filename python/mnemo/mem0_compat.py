"""Mem0-compatible wrapper around MnemoClient.

Provides a drop-in replacement interface matching Mem0's API surface,
allowing users migrating from Mem0 to use Mnemo without changing their
application code.

Example::

    from mnemo import Mem0Compat

    m = Mem0Compat(db_path="agent.mnemo.db")
    m.add("The user prefers dark mode", user_id="alice")
    results = m.search("user preferences", user_id="alice")
    m.delete(results[0]["id"])
"""

from mnemo._mnemo import MnemoClient


class Mem0Compat:
    """Mem0-compatible interface wrapping MnemoClient.

    Translates Mem0's ``add/search/delete/get_all/history/reset``
    methods into the corresponding Mnemo operations.
    """

    def __init__(self, db_path: str = "mnemo.db", **kwargs):
        """Initialize with a MnemoClient backend.

        Args:
            db_path: Path to the Mnemo database file.
            **kwargs: Additional keyword arguments forwarded to MnemoClient.
        """
        self._client = MnemoClient(db_path=db_path, **kwargs)

    # ------------------------------------------------------------------
    # Mem0 API surface
    # ------------------------------------------------------------------

    def add(self, messages, user_id=None, metadata=None):
        """Store a memory. Accepts a string or a list of message dicts.

        Args:
            messages: A plain string, or a list of dicts with ``role``
                and ``content`` keys (Mem0 message format).
            user_id: Optional user/agent ID for scoping.
            metadata: Optional dict of metadata to attach.

        Returns:
            dict with ``id`` and ``content_hash`` of the stored memory.
        """
        if isinstance(messages, list):
            # Mem0 message-list format: [{"role": "user", "content": "..."}]
            parts = []
            for msg in messages:
                if isinstance(msg, dict) and "content" in msg:
                    role = msg.get("role", "user")
                    parts.append(f"{role}: {msg['content']}")
                else:
                    parts.append(str(msg))
            content = "\n".join(parts)
        else:
            content = str(messages)

        kwargs = {}
        if user_id is not None:
            kwargs["agent_id"] = user_id
        if metadata is not None:
            kwargs["metadata"] = metadata

        return self._client.remember(content, **kwargs)

    def search(self, query, user_id=None, limit=10):
        """Search memories by semantic similarity.

        Args:
            query: Natural language search query.
            user_id: Optional user/agent ID filter.
            limit: Maximum number of results.

        Returns:
            List of dicts with ``id``, ``memory``, and ``score`` keys,
            matching Mem0's expected return format.
        """
        kwargs = {"limit": limit}
        if user_id is not None:
            kwargs["agent_id"] = user_id

        result = self._client.recall(query, **kwargs)
        memories = result.get("memories", [])
        return [
            {
                "id": str(m.get("id", "")),
                "memory": m.get("content", ""),
                "score": m.get("score", 0.0),
            }
            for m in memories
        ]

    def delete(self, memory_id):
        """Delete a single memory by ID.

        Args:
            memory_id: UUID string of the memory to delete.

        Returns:
            Result dict from the underlying forget operation.
        """
        return self._client.forget([str(memory_id)])

    def get_all(self, user_id=None):
        """Retrieve all memories, optionally filtered by user.

        Args:
            user_id: Optional user/agent ID filter.

        Returns:
            List of dicts with ``id``, ``memory``, and ``score`` keys.
        """
        kwargs = {"limit": 1000, "strategy": "exact"}
        if user_id is not None:
            kwargs["agent_id"] = user_id

        result = self._client.recall("", **kwargs)
        memories = result.get("memories", [])
        return [
            {
                "id": str(m.get("id", "")),
                "memory": m.get("content", ""),
                "score": m.get("score", 0.0),
            }
            for m in memories
        ]

    def history(self, memory_id):
        """Return edit history for a memory.

        Not directly supported by Mnemo; returns an empty list.

        Args:
            memory_id: UUID string of the memory.

        Returns:
            Empty list (placeholder for API compatibility).
        """
        return []

    def reset(self):
        """Delete all memories.

        Retrieves all memory IDs via ``get_all`` and batch-deletes them.
        """
        all_memories = self.get_all()
        if all_memories:
            ids = [m["id"] for m in all_memories]
            self._client.forget(ids)
