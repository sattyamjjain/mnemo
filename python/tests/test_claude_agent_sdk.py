"""Tests for the Claude Agent SDK adapter.

The tests are split into two layers:

1. **Pure-Python tests** — run without the native PyO3 extension. These
   validate frontmatter parsing, MCP config construction, and suppression
   semantics.

2. **Integration tests** — require the ``mnemo._mnemo`` native extension
   (built via ``maturin develop``). They exercise the full roundtrip:
   materialize → file edit → sync → recall; plus file-delete → soft-delete
   and frontmatter importance updates.

Integration tests are skipped when the extension is not importable, so the
suite stays green in environments where ``maturin develop`` has not been run.
"""

from __future__ import annotations

import importlib.util
import uuid
from pathlib import Path
from typing import Any

import pytest

from mnemo.claude_agent_sdk import (
    MemoryFile,
    MnemoClaudeMemory,
    _dump_frontmatter,
    parse_memory_file,
)


# ---------------------------------------------------------- pure-Python tests
def test_dump_and_parse_roundtrip() -> None:
    mf = MemoryFile(
        id="abc",
        importance=0.73,
        tags=["python", "notes"],
        expires_at="2027-01-01T00:00:00Z",
        body="The user prefers dark mode.",
    )
    text = mf.to_text()
    parsed = parse_memory_file(text)
    assert parsed.id == "abc"
    assert parsed.importance == pytest.approx(0.73)
    assert parsed.tags == ["python", "notes"]
    assert parsed.expires_at == "2027-01-01T00:00:00Z"
    assert parsed.body == "The user prefers dark mode."


def test_parse_missing_frontmatter_returns_body_only() -> None:
    parsed = parse_memory_file("no frontmatter here")
    assert parsed.id is None
    assert parsed.importance == 0.5
    assert parsed.tags == []
    assert parsed.body == "no frontmatter here"


def test_dump_emits_json_encoded_scalars() -> None:
    raw = _dump_frontmatter({"id": "abc", "importance": 0.5, "tags": ["a", "b"]})
    lines = raw.splitlines()
    assert lines[0] == "---"
    assert lines[-1] == "---"
    # Values round-trip through json so strings get quoted.
    assert any("\"abc\"" in line for line in lines)
    # Lists serialize inline.
    assert any(line.startswith("tags:") for line in lines)


def test_mcp_server_config_shape(tmp_path: Path) -> None:
    memory = MnemoClaudeMemory(
        db_path=str(tmp_path / "memory.db"),
        agent_id="proj-alpha",
        memory_dir=tmp_path / "memory",
        openai_api_key="sk-test",
    )
    config = memory.mcp_server_config
    assert config["type"] == "stdio"
    assert config["command"]  # binary path resolves to something
    args = config["args"]
    assert "--db-path" in args
    assert "--agent-id" in args
    assert args[args.index("--agent-id") + 1] == "proj-alpha"
    assert "--openai-api-key" in args


def test_suppression_fires_once(tmp_path: Path) -> None:
    memory = MnemoClaudeMemory(
        db_path=str(tmp_path / "m.db"),
        memory_dir=tmp_path / "memory",
    )
    path = tmp_path / "memory" / f"{uuid.uuid4()}.md"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("hello")
    memory._suppress_once(path)
    assert memory._should_suppress(path) is True
    # Second call returns False — suppression is consumed.
    assert memory._should_suppress(path) is False


def test_memory_dir_created_on_init(tmp_path: Path) -> None:
    target = tmp_path / "nested" / "memory"
    MnemoClaudeMemory(db_path=str(tmp_path / "m.db"), memory_dir=target)
    assert target.is_dir()


# --------------------------------------------------------- integration tests
_NATIVE_AVAILABLE = importlib.util.find_spec("mnemo._mnemo") is not None
# The materialize/sync tests exercise the full recall pipeline, which
# returns nothing under the default `NoopEmbedding`. Gate them on a
# functional embedding provider; the v0.3.1 benchmark report documents
# why (`docs/benchmarks/2026-04-21-mnemo-v0.3.0.md`).
import os as _os  # noqa: E402

_HAS_EMBEDDING = bool(_os.environ.get("OPENAI_API_KEY"))

integration = pytest.mark.skipif(
    not (_NATIVE_AVAILABLE and _HAS_EMBEDDING),
    reason=(
        "needs the native `mnemo._mnemo` extension (run `maturin develop`) "
        "AND a functional embedding provider. Set OPENAI_API_KEY or wait "
        "for the ONNX-embedding repair; see 2026-04-21-mnemo-v0.3.0 report."
    ),
)


@integration
def test_materialize_writes_md_files(tmp_path: Path) -> None:
    from mnemo._mnemo import MnemoClient  # type: ignore[attr-defined]

    db = tmp_path / "agent.db"
    client = MnemoClient(db_path=str(db), agent_id="t")
    client.remember(content="alpha fact about the project", importance=0.8, tags=["alpha"])
    client.remember(content="beta fact worth remembering", importance=0.6, tags=["beta"])

    memory = MnemoClaudeMemory(db_path=str(db), agent_id="t", memory_dir=tmp_path / "out")
    written = memory.materialize(query="facts", limit=10)
    assert len(written) >= 2
    for path in written:
        assert path.exists()
        parsed = parse_memory_file(path.read_text(encoding="utf-8"))
        assert parsed.id is not None
        assert parsed.body


@integration
def test_sync_file_persists_edit(tmp_path: Path) -> None:
    from mnemo._mnemo import MnemoClient  # type: ignore[attr-defined]

    db = tmp_path / "agent.db"
    client = MnemoClient(db_path=str(db), agent_id="t")
    client.remember(content="original content", importance=0.5, tags=["a"])

    memory = MnemoClaudeMemory(db_path=str(db), agent_id="t", memory_dir=tmp_path / "out")
    paths = memory.materialize(query="content", limit=5)
    assert paths

    edited_path = paths[0]
    parsed = parse_memory_file(edited_path.read_text(encoding="utf-8"))
    parsed.body = "the user now prefers nano for quick edits"
    parsed.importance = 0.9
    edited_path.write_text(parsed.to_text(), encoding="utf-8")

    new_id = memory.sync_file(edited_path)
    assert new_id is not None

    found = client.recall(query="nano quick edits", limit=5)
    assert any("nano" in m["content"] for m in found.get("memories", []))


@integration
def test_delete_file_triggers_forget(tmp_path: Path) -> None:
    from mnemo._mnemo import MnemoClient  # type: ignore[attr-defined]

    db = tmp_path / "agent.db"
    client = MnemoClient(db_path=str(db), agent_id="t")
    client.remember(content="deletable", importance=0.5)

    memory = MnemoClaudeMemory(db_path=str(db), agent_id="t", memory_dir=tmp_path / "out")
    paths = memory.materialize(query="deletable", limit=5)
    assert paths

    target = paths[0]
    forgotten_id = memory.delete_file(target)
    assert forgotten_id is not None
    assert target.stem == forgotten_id

    # After soft-delete, the deleted memory should not resurface in default recall.
    found = client.recall(query="deletable", limit=5)
    ids = [m["id"] for m in found.get("memories", [])]
    assert forgotten_id not in ids


@integration
def test_frontmatter_importance_update_round_trips(tmp_path: Path) -> None:
    from mnemo._mnemo import MnemoClient  # type: ignore[attr-defined]

    db = tmp_path / "agent.db"
    client = MnemoClient(db_path=str(db), agent_id="t")
    client.remember(content="promote me please", importance=0.2)

    memory = MnemoClaudeMemory(db_path=str(db), agent_id="t", memory_dir=tmp_path / "out")
    paths = memory.materialize(query="promote", limit=5)
    assert paths

    target = paths[0]
    parsed = parse_memory_file(target.read_text(encoding="utf-8"))
    parsed.importance = 0.95
    parsed.tags = list(set(parsed.tags + ["promoted"]))
    target.write_text(parsed.to_text(), encoding="utf-8")

    new_id = memory.sync_file(target)
    assert new_id is not None

    # The new memory carries the higher importance we wrote.
    found = client.recall(query="promote me", limit=5)
    hits: list[dict[str, Any]] = found.get("memories", [])
    assert any(float(m.get("importance", 0)) >= 0.9 for m in hits)
