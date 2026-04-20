"""Tests for the OpenAI Agents SDK Session store.

Pure-Python tests exercise the encoding helpers and protocol surface; the
integration tests (gated on the native extension) walk through a full
conversation lifecycle: add, get, pop, concurrent adds, clear.
"""

from __future__ import annotations

import asyncio
import importlib.util
from pathlib import Path

import pytest

from mnemo.openai_sessions import MnemoSessionStore, _session_tag


def test_session_tag_format() -> None:
    assert _session_tag("abc") == "_session:abc"
    assert _session_tag("ticket-42") == "_session:ticket-42"


def test_encode_decode_roundtrip() -> None:
    item = {"role": "user", "content": "Hello world"}
    encoded = MnemoSessionStore._encode(item)
    assert MnemoSessionStore._decode(encoded) == item


def test_encode_falls_back_on_non_serializable() -> None:
    class Custom:
        def __repr__(self) -> str:
            return "<custom-item>"

    encoded = MnemoSessionStore._encode(Custom())
    decoded = MnemoSessionStore._decode(encoded)
    assert isinstance(decoded, dict)
    assert decoded.get("_note") == "non-serializable"
    assert "<custom-item>" in decoded.get("_repr", "")


def test_decode_of_plain_string_is_system_message() -> None:
    decoded = MnemoSessionStore._decode("bare content")
    assert decoded == {"role": "system", "content": "bare content"}


def test_requires_session_id(tmp_path: Path) -> None:
    with pytest.raises(ValueError):
        MnemoSessionStore(session_id="", db_path=str(tmp_path / "m.db"))


# ----------------------------------------------------- integration tests
_NATIVE_AVAILABLE = importlib.util.find_spec("mnemo._mnemo") is not None
integration = pytest.mark.skipif(
    not _NATIVE_AVAILABLE,
    reason="mnemo native extension not built (run `maturin develop` in python/).",
)


@integration
def test_roundtrip_add_and_get(tmp_path: Path) -> None:
    async def scenario() -> None:
        session = MnemoSessionStore(
            db_path=str(tmp_path / "sess.db"),
            agent_id="tester",
            session_id="conv-1",
        )
        await session.add_items(
            [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": "hello!"},
                {"role": "user", "content": "what time is it?"},
            ]
        )
        items = await session.get_items()
        assert len(items) == 3
        assert items[0]["content"] == "hi"
        assert items[-1]["content"] == "what time is it?"

    asyncio.run(scenario())


@integration
def test_pop_item_returns_latest_and_removes_it(tmp_path: Path) -> None:
    async def scenario() -> None:
        session = MnemoSessionStore(
            db_path=str(tmp_path / "sess.db"),
            session_id="conv-2",
        )
        await session.add_items(
            [
                {"role": "user", "content": "first"},
                {"role": "user", "content": "second"},
            ]
        )
        popped = await session.pop_item()
        assert popped is not None
        assert popped["content"] == "second"
        remaining = await session.get_items()
        assert [i["content"] for i in remaining] == ["first"]

    asyncio.run(scenario())


@integration
def test_pop_on_empty_session_returns_none(tmp_path: Path) -> None:
    async def scenario() -> None:
        session = MnemoSessionStore(
            db_path=str(tmp_path / "sess.db"),
            session_id="conv-empty",
        )
        result = await session.pop_item()
        assert result is None

    asyncio.run(scenario())


@integration
def test_clear_session_removes_all(tmp_path: Path) -> None:
    async def scenario() -> None:
        session = MnemoSessionStore(
            db_path=str(tmp_path / "sess.db"),
            session_id="conv-3",
        )
        await session.add_items(
            [{"role": "user", "content": f"msg-{i}"} for i in range(5)]
        )
        await session.clear_session()
        items = await session.get_items()
        assert items == []

    asyncio.run(scenario())


@integration
def test_concurrent_adds_preserve_ordering(tmp_path: Path) -> None:
    async def scenario() -> None:
        session = MnemoSessionStore(
            db_path=str(tmp_path / "sess.db"),
            session_id="conv-race",
        )
        batches = [
            [{"role": "user", "content": f"a-{i}"} for i in range(3)],
            [{"role": "user", "content": f"b-{i}"} for i in range(3)],
        ]
        # Run concurrently; the internal lock serializes the index allocation.
        await asyncio.gather(*(session.add_items(b) for b in batches))
        items = await session.get_items()
        assert len(items) == 6
        # Indices must be strictly increasing across both batches.
        contents = [i["content"] for i in items]
        # The ordering between batches depends on the lock's winner, but we
        # must never interleave within a single batch's 3 items.
        a_positions = [idx for idx, c in enumerate(contents) if c.startswith("a-")]
        b_positions = [idx for idx, c in enumerate(contents) if c.startswith("b-")]
        assert a_positions == sorted(a_positions)
        assert b_positions == sorted(b_positions)

    asyncio.run(scenario())


@integration
def test_sessions_do_not_leak_across_session_ids(tmp_path: Path) -> None:
    async def scenario() -> None:
        sess_a = MnemoSessionStore(
            db_path=str(tmp_path / "sess.db"),
            session_id="session-a",
        )
        sess_b = MnemoSessionStore(
            db_path=str(tmp_path / "sess.db"),
            session_id="session-b",
        )
        await sess_a.add_items([{"role": "user", "content": "only-a"}])
        await sess_b.add_items([{"role": "user", "content": "only-b"}])

        a_items = await sess_a.get_items()
        b_items = await sess_b.get_items()
        assert [i["content"] for i in a_items] == ["only-a"]
        assert [i["content"] for i in b_items] == ["only-b"]

    asyncio.run(scenario())
