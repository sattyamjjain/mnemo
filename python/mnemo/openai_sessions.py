"""OpenAI Agents SDK ``Session`` integration for Mnemo.

The 2026-04-15 release of ``openai-agents`` reshaped the persistence layer
around a ``Session`` protocol (``SessionABC``) exposing four async methods:

* ``get_items(limit: int | None = None) -> list[TResponseInputItem]``
* ``add_items(items: list[TResponseInputItem]) -> None``
* ``pop_item() -> TResponseInputItem | None``
* ``clear_session() -> None``

:class:`MnemoSessionStore` implements that protocol on top of Mnemo. Each
conversation item is stored as an episodic memory tagged with the session
id, which makes session history searchable, auditable, and restorable
across process restarts — independently of the OpenAI Conversations API.

Example::

    import asyncio
    from agents import Agent, Runner
    from mnemo.openai_sessions import MnemoSessionStore

    async def main():
        session = MnemoSessionStore(
            db_path="agent.mnemo.db",
            agent_id="user-42",
            session_id="support-conv-2026-04-20",
        )
        agent = Agent(name="Support")
        result = await Runner.run(agent, "I can't log in", session=session)
        print(result.final_output)

    asyncio.run(main())
"""

from __future__ import annotations

import asyncio
import json
import threading
from typing import Any, Optional

_SESSION_TAG_PREFIX = "_session:"


def _session_tag(session_id: str) -> str:
    return f"{_SESSION_TAG_PREFIX}{session_id}"


class MnemoSessionStore:
    """A Mnemo-backed implementation of the ``openai-agents`` Session protocol.

    The instance is tied to one ``session_id``; pass a new store for each
    conversation (this matches :class:`SQLiteSession` in the upstream SDK).

    Args:
        db_path: Path to the DuckDB database file.
        agent_id: Mnemo agent identifier (namespacing).
        session_id: Stable identifier for this conversation.
        openai_api_key: Optional OpenAI API key for embeddings.
        embedding_model: Embedding model name.
        dimensions: Embedding dimensions.
    """

    def __init__(
        self,
        session_id: str,
        db_path: str = "mnemo.db",
        agent_id: str = "default",
        openai_api_key: Optional[str] = None,
        embedding_model: str = "text-embedding-3-small",
        dimensions: int = 1536,
    ) -> None:
        if not session_id:
            raise ValueError("session_id is required")
        self.session_id = session_id
        self.db_path = db_path
        self.agent_id = agent_id
        self.openai_api_key = openai_api_key
        self.embedding_model = embedding_model
        self.dimensions = dimensions
        self._tag = _session_tag(session_id)
        self._client = None
        # Serialize writes so concurrent ``add_items`` calls don't corrupt the
        # monotonic item index. MnemoClient is not async-aware today.
        self._lock = threading.Lock()
        # Used for the placeholder recall query; Mnemo still embeds it even in
        # "exact" mode, so keep it small and stable.
        self._listing_query = f"_session_listing_{session_id}"

    # ------------------------------------------------------------- client
    def _ensure_client(self):
        if self._client is not None:
            return self._client
        try:
            from mnemo._mnemo import MnemoClient  # type: ignore[attr-defined]
        except ImportError as exc:  # pragma: no cover
            from mnemo.availability import MnemoClientUnavailable

            raise MnemoClientUnavailable(
                "MnemoSessionStore needs the native mnemo._mnemo extension"
            ) from exc
        self._client = MnemoClient(
            db_path=self.db_path,
            agent_id=self.agent_id,
            openai_api_key=self.openai_api_key,
            embedding_model=self.embedding_model,
            dimensions=self.dimensions,
        )
        return self._client

    # ------------------------------------------------ storage encoding
    @staticmethod
    def _encode(item: Any) -> str:
        """Serialize an Agents SDK item to a Mnemo memory content string."""
        try:
            return json.dumps(item, ensure_ascii=False)
        except (TypeError, ValueError):
            # Items sometimes contain pydantic models. Fall back to string repr
            # so a malformed item never breaks the session.
            return json.dumps({"_repr": repr(item), "_note": "non-serializable"})

    @staticmethod
    def _decode(content: str) -> Any:
        try:
            return json.loads(content)
        except (ValueError, TypeError):
            return {"role": "system", "content": content}

    def _next_index(self) -> int:
        """Return the next monotonically-increasing item index for this session."""
        client = self._ensure_client()
        existing = client.recall(
            query=self._listing_query,
            limit=1000,
            tags=[self._tag],
            strategy="exact",
        )
        memories = existing.get("memories", []) if isinstance(existing, dict) else []
        indices = []
        for memory in memories:
            data = self._decode(memory.get("content", ""))
            if isinstance(data, dict) and "_session_index" in data:
                try:
                    indices.append(int(data["_session_index"]))
                except (TypeError, ValueError):
                    continue
        return (max(indices) + 1) if indices else 0

    def _sorted_items(self, limit: Optional[int]) -> list[tuple[str, int, Any]]:
        """Return (memory_id, index, decoded_item) triples in chronological order."""
        client = self._ensure_client()
        # Ask for a generous ceiling; Mnemo caps recall at 100/request but we
        # sort in Python anyway so over-asking is fine.
        fetch_limit = min(1000, max(limit or 1000, 100))
        found = client.recall(
            query=self._listing_query,
            limit=fetch_limit,
            tags=[self._tag],
            strategy="exact",
        )
        memories = found.get("memories", []) if isinstance(found, dict) else []
        triples: list[tuple[str, int, Any]] = []
        for memory in memories:
            data = self._decode(memory.get("content", ""))
            idx = 0
            payload: Any = data
            if isinstance(data, dict) and "_session_index" in data:
                try:
                    idx = int(data["_session_index"])
                except (TypeError, ValueError):
                    idx = 0
                payload = data.get("item", data)
            triples.append((memory["id"], idx, payload))
        triples.sort(key=lambda row: row[1])
        if limit is not None and limit > 0:
            triples = triples[-limit:]
        return triples

    # -------------------------------------------- Session protocol impl
    async def get_items(self, limit: Optional[int] = None) -> list[Any]:
        def _run() -> list[Any]:
            return [item for _id, _idx, item in self._sorted_items(limit)]

        return await asyncio.to_thread(_run)

    async def add_items(self, items: list[Any]) -> None:
        def _run() -> None:
            client = self._ensure_client()
            with self._lock:
                base = self._next_index()
                for offset, item in enumerate(items):
                    payload = {
                        "_session_index": base + offset,
                        "_session_id": self.session_id,
                        "item": item,
                    }
                    client.remember(
                        content=self._encode(payload),
                        memory_type="episodic",
                        importance=0.5,
                        tags=[self._tag],
                        metadata={
                            "session_id": self.session_id,
                            "session_index": base + offset,
                        },
                    )

        await asyncio.to_thread(_run)

    async def pop_item(self) -> Optional[Any]:
        def _run() -> Optional[Any]:
            triples = self._sorted_items(limit=None)
            if not triples:
                return None
            memory_id, _idx, item = triples[-1]
            client = self._ensure_client()
            try:
                client.forget([memory_id])
            except Exception:
                return None
            return item

        return await asyncio.to_thread(_run)

    async def clear_session(self) -> None:
        def _run() -> None:
            triples = self._sorted_items(limit=None)
            if not triples:
                return
            client = self._ensure_client()
            ids = [memory_id for memory_id, _idx, _item in triples]
            try:
                client.forget(ids)
            except Exception:
                # If batch-forget isn't supported, fall back to one-by-one.
                for memory_id in ids:
                    try:
                        client.forget([memory_id])
                    except Exception:
                        continue

        await asyncio.to_thread(_run)


__all__ = ["MnemoSessionStore"]
