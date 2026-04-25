"""Tests for the Letta-style shared-memory adapter (v0.4.0-rc1 Task A5)."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any
from uuid import uuid4

import pytest

from mnemo.letta_adapter import (
    META_PARTICIPANTS_TAG,
    MnemoLettaShared,
    PARTICIPANT_TAG_PREFIX,
)


@dataclass
class _FakeRecord:
    id: str
    content: str
    tags: list[str]
    created_at: str = ""


@dataclass
class FakeMnemoStore:
    """In-process MnemoClient stand-in. Tag-filtered recall + remember + forget."""

    records: list[_FakeRecord] = field(default_factory=list)

    def remember(
        self,
        content: str,
        memory_type: str | None = None,
        importance: float | None = None,
        tags: list[str] | None = None,
        metadata: dict[str, Any] | None = None,
        thread_id: str | None = None,
    ) -> dict[str, Any]:
        rid = str(uuid4())
        self.records.append(
            _FakeRecord(
                id=rid,
                content=content,
                tags=tags or [],
                created_at=metadata.get("written_at", "") if metadata else "",
            )
        )
        return {"id": rid, "content_hash": "deadbeef"}

    def recall(
        self,
        query: str,
        limit: int | None = None,
        tags: list[str] | None = None,
    ) -> dict[str, Any]:
        wanted = set(tags or [])
        out = []
        for r in self.records:
            if wanted.issubset(set(r.tags)):
                out.append(
                    {
                        "id": r.id,
                        "content": r.content,
                        "tags": r.tags,
                        "created_at": r.created_at,
                    }
                )
        return {"memories": out, "total": len(out)}

    def forget(
        self,
        memory_ids: list[str],
        strategy: str | None = None,
    ) -> dict[str, Any]:
        keep = [r for r in self.records if r.id not in memory_ids]
        forgotten = [r.id for r in self.records if r.id in memory_ids]
        self.records = keep
        return {"forgotten": forgotten, "errors": []}


# ----------------------------------------------------------------------
# Construction
# ----------------------------------------------------------------------


def test_requires_non_empty_conversation_id() -> None:
    with pytest.raises(ValueError, match="conversation_id"):
        MnemoLettaShared(client=FakeMnemoStore(), conversation_id="")


def test_thread_id_defaults_to_conversation_id() -> None:
    shared = MnemoLettaShared(client=FakeMnemoStore(), conversation_id="conv-1")
    assert shared.thread_id == "conv-1"


# ----------------------------------------------------------------------
# Participant lifecycle
# ----------------------------------------------------------------------


def test_attach_and_detach_round_trip() -> None:
    store = FakeMnemoStore()
    shared = MnemoLettaShared(client=store, conversation_id="conv-1")
    assert shared.list_participants() == []
    assert shared.attach("agent-A") == ["agent-A"]
    assert shared.attach("agent-B") == ["agent-A", "agent-B"]
    # Idempotent.
    assert shared.attach("agent-A") == ["agent-A", "agent-B"]
    assert shared.detach("agent-A") == ["agent-B"]
    assert shared.list_participants() == ["agent-B"]


def test_participants_metadata_record_is_overwritten_not_duplicated() -> None:
    store = FakeMnemoStore()
    shared = MnemoLettaShared(client=store, conversation_id="conv-1")
    shared.attach("agent-A")
    shared.attach("agent-B")
    shared.detach("agent-A")
    meta_records = [r for r in store.records if META_PARTICIPANTS_TAG in r.tags]
    assert len(meta_records) == 1, "must collapse to a single canonical record"


# ----------------------------------------------------------------------
# Read / write
# ----------------------------------------------------------------------


def test_write_tags_message_with_conversation_and_participant() -> None:
    store = FakeMnemoStore()
    shared = MnemoLettaShared(client=store, conversation_id="conv-1")
    shared.write("hello", source_agent_id="agent-A")
    msg = next(
        r for r in store.records if META_PARTICIPANTS_TAG not in r.tags
    )
    assert "conversation:conv-1" in msg.tags
    assert f"{PARTICIPANT_TAG_PREFIX}agent-A" in msg.tags


def test_read_returns_messages_excluding_participants_metadata() -> None:
    store = FakeMnemoStore()
    shared = MnemoLettaShared(client=store, conversation_id="conv-1")
    shared.attach("agent-A")
    shared.write("hello world", source_agent_id="agent-A")
    msgs = shared.read()
    assert len(msgs) == 1
    assert msgs[0].content == "hello world"
    assert msgs[0].source_agent_id == "agent-A"


def test_read_filtered_by_from_agent() -> None:
    store = FakeMnemoStore()
    shared = MnemoLettaShared(client=store, conversation_id="conv-1")
    shared.write("from-A", source_agent_id="agent-A")
    shared.write("from-B", source_agent_id="agent-B")
    only_a = shared.read(from_agent="agent-A")
    assert {m.content for m in only_a} == {"from-A"}


def test_write_rejects_empty_content() -> None:
    shared = MnemoLettaShared(client=FakeMnemoStore(), conversation_id="conv-1")
    with pytest.raises(ValueError):
        shared.write("", source_agent_id="agent-A")


def test_write_rejects_empty_source_agent() -> None:
    shared = MnemoLettaShared(client=FakeMnemoStore(), conversation_id="conv-1")
    with pytest.raises(ValueError):
        shared.write("hi", source_agent_id="")


# ----------------------------------------------------------------------
# Conflict window
# ----------------------------------------------------------------------


def test_overlapping_writes_within_returns_cross_participant_pairs() -> None:
    store = FakeMnemoStore()
    shared = MnemoLettaShared(client=store, conversation_id="conv-1")
    shared.write("first by A", source_agent_id="agent-A")
    # Force created_at on the next message to be one second later.
    shared.write("then by B", source_agent_id="agent-B")
    pairs = shared.overlapping_writes_within(seconds=60.0)
    assert pairs, "two writes within 60s by different agents must surface"
    a, b = pairs[0]
    assert a.source_agent_id != b.source_agent_id


def test_overlapping_writes_does_not_pair_same_agent() -> None:
    store = FakeMnemoStore()
    shared = MnemoLettaShared(client=store, conversation_id="conv-1")
    shared.write("a1", source_agent_id="agent-A")
    shared.write("a2", source_agent_id="agent-A")
    assert shared.overlapping_writes_within(seconds=60.0) == []
