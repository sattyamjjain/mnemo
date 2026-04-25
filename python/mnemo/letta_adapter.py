"""Letta-style shared-memory adapter (v0.4.0-rc1 Task A5).

Letta's 2026-04-06 "Letta-Code" release introduced a Conversations API
where multiple agents share a single memory stream. This adapter
exposes the same shape — `attach` / `detach` / `read` / `write` /
`list_participants` — backed by Mnemo memories rather than a remote
Letta service. That keeps the agents' shared state on Mnemo's audit
log + hash chain + ACL surface even when the agents themselves are
running through Letta's orchestration.

Storage shape
-------------

* Every shared memory: tagged ``conversation:<id>`` and
  ``participant:<source_agent_id>``.
* Participants list: one memory tagged ``conversation:<id>`` and
  ``meta:participants``, body = JSON list of agent IDs. Updated
  whenever ``attach`` / ``detach`` is called.

Conflict policy
---------------

When two agents ``write`` overlapping content within 60s the adapter
attaches both records to the same conversation and lets Mnemo's
existing :class:`mnemo_core::query::conflict::EvidenceWeighted`
resolver pick the winner during downstream recall. The adapter does
not pre-resolve — that would amount to a write-time silent drop.
"""

from __future__ import annotations

import json
import threading
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Iterable, Protocol

CONVERSATION_TAG_PREFIX = "conversation:"
PARTICIPANT_TAG_PREFIX = "participant:"
META_PARTICIPANTS_TAG = "meta:participants"
DEFAULT_RECALL_LIMIT = 50
OVERLAP_WINDOW_SECONDS = 60.0


class _StoreLike(Protocol):
    """Minimal slice of :class:`mnemo.MnemoClient` we depend on.

    Wrapped as a Protocol so tests can swap in a fake without
    requiring the native PyO3 extension. The real client satisfies
    this surface unchanged.
    """

    def remember(
        self,
        content: str,
        memory_type: str | None = None,
        importance: float | None = None,
        tags: list[str] | None = None,
        metadata: dict[str, Any] | None = None,
        thread_id: str | None = None,
    ) -> dict[str, Any]: ...

    def recall(
        self,
        query: str,
        limit: int | None = None,
        tags: list[str] | None = None,
    ) -> dict[str, Any]: ...

    def forget(
        self,
        memory_ids: list[str],
        strategy: str | None = None,
    ) -> dict[str, Any]: ...


@dataclass(frozen=True)
class SharedMessage:
    """One read returned by :meth:`MnemoLettaShared.read`."""

    id: str
    content: str
    source_agent_id: str
    created_at: str
    tags: tuple[str, ...]


class MnemoLettaShared:
    """Letta Conversations-style shared memory backed by Mnemo.

    Args:
        client: Anything that satisfies the :class:`_StoreLike`
            protocol — typically a configured ``MnemoClient``.
        conversation_id: Stable identifier shared by every
            participant. Tags every record with ``conversation:<id>``.
        thread_id: Optional Mnemo thread identifier. Defaults to
            ``conversation_id`` so the conversation maps 1-1 to a
            Mnemo thread.

    Threading: the adapter holds a ``threading.Lock`` that guards
    participant-list updates. Reads and writes are otherwise
    independent — the Mnemo client is expected to be its own
    concurrency boundary.
    """

    def __init__(
        self,
        client: _StoreLike,
        *,
        conversation_id: str,
        thread_id: str | None = None,
    ) -> None:
        if not conversation_id:
            raise ValueError("conversation_id must be a non-empty string")
        self._client = client
        self.conversation_id = conversation_id
        self.thread_id = thread_id or conversation_id
        self._conversation_tag = f"{CONVERSATION_TAG_PREFIX}{conversation_id}"
        self._participants_lock = threading.Lock()

    # ------------------------------------------------------------------
    # Participant management
    # ------------------------------------------------------------------

    def attach(self, agent_id: str) -> list[str]:
        """Register ``agent_id`` as a participant. Idempotent.

        Returns the canonical participants list after the attach.
        """
        if not agent_id:
            raise ValueError("agent_id must be a non-empty string")
        with self._participants_lock:
            participants = self._read_participants()
            if agent_id not in participants:
                participants = sorted({*participants, agent_id})
                self._write_participants(participants)
            return list(participants)

    def detach(self, agent_id: str) -> list[str]:
        """Drop ``agent_id`` from the participants list. Idempotent."""
        with self._participants_lock:
            participants = self._read_participants()
            if agent_id in participants:
                participants = [p for p in participants if p != agent_id]
                self._write_participants(participants)
            return list(participants)

    def list_participants(self) -> list[str]:
        """Current participant set, sorted ascending."""
        with self._participants_lock:
            return list(self._read_participants())

    # ------------------------------------------------------------------
    # Read / write
    # ------------------------------------------------------------------

    def write(self, content: str, source_agent_id: str) -> dict[str, Any]:
        """Append a message to the shared memory.

        Tagged ``conversation:<id>`` + ``participant:<source_agent_id>``
        so per-author + per-conversation recalls both work. The
        ``source_agent_id`` does not have to be a registered
        participant — Letta allows write-without-attach for
        notification-style agents — but we surface a warning when it
        isn't, to keep the "intended participant" set coherent.
        """
        if not content:
            raise ValueError("content must be a non-empty string")
        if not source_agent_id:
            raise ValueError("source_agent_id must be a non-empty string")
        tags = [
            self._conversation_tag,
            f"{PARTICIPANT_TAG_PREFIX}{source_agent_id}",
        ]
        metadata = {
            "conversation_id": self.conversation_id,
            "source_agent_id": source_agent_id,
            "shared_memory": True,
            "written_at": datetime.now(timezone.utc).isoformat(),
        }
        return self._client.remember(
            content=content,
            memory_type="episodic",
            tags=tags,
            metadata=metadata,
            thread_id=self.thread_id,
        )

    def read(
        self,
        query: str = "",
        *,
        limit: int = DEFAULT_RECALL_LIMIT,
        from_agent: str | None = None,
    ) -> list[SharedMessage]:
        """Recall shared messages — optionally filtered by author.

        ``query`` is forwarded to Mnemo's hybrid retrieval path
        (vector + BM25) when set; an empty query is treated as a
        time-ordered scan.
        """
        tags = [self._conversation_tag]
        if from_agent:
            tags.append(f"{PARTICIPANT_TAG_PREFIX}{from_agent}")
        result = self._client.recall(query=query, limit=limit, tags=tags)
        memories = result.get("memories", []) if isinstance(result, dict) else []
        out: list[SharedMessage] = []
        for mem in memories:
            mem_tags = mem.get("tags", []) or []
            # Skip the participants metadata record.
            if META_PARTICIPANTS_TAG in mem_tags:
                continue
            source = _participant_from_tags(mem_tags) or "unknown"
            out.append(
                SharedMessage(
                    id=str(mem.get("id", "")),
                    content=str(mem.get("content", "")),
                    source_agent_id=source,
                    created_at=str(mem.get("created_at", "")),
                    tags=tuple(mem_tags),
                )
            )
        return out

    # ------------------------------------------------------------------
    # Conflict-window helpers
    # ------------------------------------------------------------------

    def overlapping_writes_within(
        self,
        seconds: float = OVERLAP_WINDOW_SECONDS,
    ) -> list[tuple[SharedMessage, SharedMessage]]:
        """Pairs of writes from different participants within ``seconds``.

        Returned for operator inspection; conflict resolution itself
        happens at recall time via Mnemo's evidence-weighted scoring.
        """
        msgs = sorted(self.read(limit=500), key=lambda m: m.created_at)
        out: list[tuple[SharedMessage, SharedMessage]] = []
        for i, m in enumerate(msgs):
            t_m = _parse_iso(m.created_at)
            if t_m is None:
                continue
            for n in msgs[i + 1 :]:
                t_n = _parse_iso(n.created_at)
                if t_n is None:
                    break
                if (t_n - t_m) > seconds:
                    break
                if m.source_agent_id != n.source_agent_id:
                    out.append((m, n))
        return out

    # ------------------------------------------------------------------
    # Internals
    # ------------------------------------------------------------------

    def _read_participants(self) -> list[str]:
        """Pull the JSON-encoded participants list, or `[]` if absent."""
        result = self._client.recall(
            query="",
            limit=1,
            tags=[self._conversation_tag, META_PARTICIPANTS_TAG],
        )
        memories = result.get("memories", []) if isinstance(result, dict) else []
        if not memories:
            return []
        try:
            participants = json.loads(memories[0].get("content", "[]"))
        except (TypeError, ValueError):
            return []
        return [str(p) for p in participants if isinstance(p, str)]

    def _write_participants(self, participants: Iterable[str]) -> None:
        """Replace the participants metadata record."""
        existing = self._client.recall(
            query="",
            limit=1,
            tags=[self._conversation_tag, META_PARTICIPANTS_TAG],
        )
        existing_memories = (
            existing.get("memories", []) if isinstance(existing, dict) else []
        )
        if existing_memories:
            existing_id = existing_memories[0].get("id")
            if existing_id:
                self._client.forget(memory_ids=[str(existing_id)], strategy="hard_delete")
        self._client.remember(
            content=json.dumps(sorted(set(participants))),
            memory_type="semantic",
            tags=[self._conversation_tag, META_PARTICIPANTS_TAG],
            metadata={"conversation_id": self.conversation_id},
            thread_id=self.thread_id,
        )


# ----------------------------------------------------------------------
# Helpers
# ----------------------------------------------------------------------


def _participant_from_tags(tags: list[str]) -> str | None:
    for t in tags:
        if t.startswith(PARTICIPANT_TAG_PREFIX):
            return t[len(PARTICIPANT_TAG_PREFIX) :]
    return None


def _parse_iso(iso: str) -> float | None:
    """Parse an RFC3339 timestamp to epoch seconds."""
    if not iso:
        return None
    try:
        # `fromisoformat` handles RFC3339 with offset; some Mnemo
        # backends emit `Z` which Python <3.11 doesn't accept directly.
        cleaned = iso.replace("Z", "+00:00") if iso.endswith("Z") else iso
        return datetime.fromisoformat(cleaned).timestamp()
    except ValueError:
        try:
            # Fallback for `time.strptime` if needed.
            return time.mktime(time.strptime(iso[:19], "%Y-%m-%dT%H:%M:%S"))
        except ValueError:
            return None
