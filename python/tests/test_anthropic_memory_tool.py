"""Unit tests for the Anthropic memory_20250818 tool server.

The tests do NOT require the native Mnemo extension. A small in-process
fake stands in for `MnemoClient` so we can exercise the 6-op contract
(view / create / str_replace / insert / delete / rename), the path
validation, and the canonical return-string formats from the spec
without needing maturin or an Anthropic API key.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any
from uuid import uuid4

import pytest

from mnemo.anthropic_memory_tool import (
    MANAGED_AGENTS_BETA,
    MnemoMemoryToolServer,
    PATH_TAG_PREFIX,
    TAG_MARKER,
    TOOL_TYPE,
)


# ----------------------------------------------------------------------
# In-memory fake of the subset of MnemoClient the server uses.
# ----------------------------------------------------------------------


@dataclass
class _FakeRecord:
    id: str
    content: str
    tags: list[str]


@dataclass
class FakeMnemoStore:
    """Tiny in-memory MnemoClient stand-in for the memory tool tests."""

    records: dict[str, _FakeRecord] = field(default_factory=dict)
    forget_calls: list[list[str]] = field(default_factory=list)

    def remember(
        self,
        content: str,
        memory_type: str | None = None,
        tags: list[str] | None = None,
        metadata: dict[str, Any] | None = None,
        thread_id: str | None = None,
    ) -> dict[str, Any]:
        rid = str(uuid4())
        self.records[rid] = _FakeRecord(id=rid, content=content, tags=tags or [])
        return {"id": rid, "content_hash": "deadbeef"}

    def recall(
        self,
        query: str,
        limit: int | None = None,
        tags: list[str] | None = None,
    ) -> dict[str, Any]:
        wanted = set(tags or [])
        out = []
        for r in self.records.values():
            if wanted.issubset(set(r.tags)):
                out.append({"id": r.id, "content": r.content, "tags": r.tags})
        return {"memories": out, "total": len(out)}

    def forget(
        self,
        memory_ids: list[str],
        strategy: str | None = None,
    ) -> dict[str, Any]:
        self.forget_calls.append(list(memory_ids))
        forgotten = []
        for mid in memory_ids:
            if mid in self.records:
                del self.records[mid]
                forgotten.append(mid)
        return {"forgotten": forgotten, "errors": []}


# ----------------------------------------------------------------------
# Helpers
# ----------------------------------------------------------------------


def _server(managed: bool = False) -> tuple[MnemoMemoryToolServer, FakeMnemoStore]:
    store = FakeMnemoStore()
    return (
        MnemoMemoryToolServer(client=store, managed_agents_beta=managed),
        store,
    )


def _content(server: MnemoMemoryToolServer, store: FakeMnemoStore, path: str) -> str:
    """Return the stored body for one memory-tool path."""
    files = server._list_files()  # type: ignore[attr-defined]
    return files[path]["content"]


# ----------------------------------------------------------------------
# Surface
# ----------------------------------------------------------------------


def test_tool_schema_returns_memory_20250818() -> None:
    srv, _ = _server()
    assert srv.tool_schema() == {"type": TOOL_TYPE, "name": "memory"}


def test_beta_header_off_by_default() -> None:
    srv, _ = _server()
    assert srv.beta_header() is None


def test_beta_header_on_when_managed_agents_beta() -> None:
    srv, _ = _server(managed=True)
    assert srv.beta_header() == MANAGED_AGENTS_BETA


# ----------------------------------------------------------------------
# create
# ----------------------------------------------------------------------


def test_create_writes_record_with_tags_and_returns_success() -> None:
    srv, store = _server()
    out = srv._dispatch({"command": "create", "path": "/memories/notes.txt", "file_text": "hi\nthere"})
    assert out == "File created successfully at: /memories/notes.txt"
    # Stored as memory-tool record with `path:/memories/notes.txt` tag.
    rec = next(iter(store.records.values()))
    assert rec.content == "hi\nthere"
    assert TAG_MARKER in rec.tags
    assert f"{PATH_TAG_PREFIX}/memories/notes.txt" in rec.tags


def test_create_rejects_duplicate_path() -> None:
    srv, _ = _server()
    srv._dispatch({"command": "create", "path": "/memories/dup.txt", "file_text": "first"})
    res = srv.handle({"command": "create", "path": "/memories/dup.txt", "file_text": "second"})
    assert res["is_error"] is True
    assert res["content"] == "Error: File /memories/dup.txt already exists"


def test_create_rejects_root_as_file() -> None:
    srv, _ = _server()
    res = srv.handle({"command": "create", "path": "/memories", "file_text": "x"})
    assert res["is_error"] is True
    assert "memory root" in res["content"]


# ----------------------------------------------------------------------
# view
# ----------------------------------------------------------------------


def test_view_file_returns_line_numbered_content() -> None:
    srv, _ = _server()
    srv._dispatch({"command": "create", "path": "/memories/a.txt", "file_text": "Hello\nworld"})
    out = srv._dispatch({"command": "view", "path": "/memories/a.txt"})
    assert out == "Here's the content of /memories/a.txt with line numbers:\n     1\tHello\n     2\tworld"


def test_view_file_with_view_range() -> None:
    srv, _ = _server()
    body = "\n".join(f"line{i}" for i in range(1, 11))
    srv._dispatch({"command": "create", "path": "/memories/long.txt", "file_text": body})
    out = srv._dispatch({"command": "view", "path": "/memories/long.txt", "view_range": [3, 5]})
    # 3 lines starting at line 3.
    assert "     3\tline3" in out
    assert "     4\tline4" in out
    assert "     5\tline5" in out
    assert "line2" not in out
    assert "line6" not in out


def test_view_directory_lists_children() -> None:
    srv, _ = _server()
    srv._dispatch({"command": "create", "path": "/memories/a.txt", "file_text": "a"})
    srv._dispatch({"command": "create", "path": "/memories/b.txt", "file_text": "bb"})
    srv._dispatch({"command": "create", "path": "/memories/sub/c.txt", "file_text": "ccc"})
    out = srv._dispatch({"command": "view", "path": "/memories"})
    assert "Here're the files and directories up to 2 levels deep in /memories" in out
    assert "/memories/a.txt" in out
    assert "/memories/b.txt" in out
    # The 'sub' directory shows up, not /memories/sub/c.txt directly.
    assert "/memories/sub" in out


def test_view_nonexistent_path_errors() -> None:
    srv, _ = _server()
    res = srv.handle({"command": "view", "path": "/memories/nope.txt"})
    assert res["is_error"] is True
    assert res["content"] == "The path /memories/nope.txt does not exist. Please provide a valid path."


def test_view_invalid_range_errors() -> None:
    srv, _ = _server()
    srv._dispatch({"command": "create", "path": "/memories/x.txt", "file_text": "a\nb"})
    res = srv.handle({"command": "view", "path": "/memories/x.txt", "view_range": [5, 2]})
    assert res["is_error"] is True
    assert "Invalid `view_range`" in res["content"]


# ----------------------------------------------------------------------
# str_replace
# ----------------------------------------------------------------------


def test_str_replace_unique_succeeds_and_rewrites() -> None:
    srv, store = _server()
    srv._dispatch({"command": "create", "path": "/memories/p.txt", "file_text": "Favorite color: blue"})
    out = srv._dispatch(
        {
            "command": "str_replace",
            "path": "/memories/p.txt",
            "old_str": "Favorite color: blue",
            "new_str": "Favorite color: green",
        }
    )
    assert out.startswith("The memory file has been edited.")
    # Stored content reflects the rewrite.
    assert _content(srv, store, "/memories/p.txt") == "Favorite color: green"


def test_str_replace_no_match_returns_doc_string() -> None:
    srv, _ = _server()
    srv._dispatch({"command": "create", "path": "/memories/p.txt", "file_text": "alpha"})
    out = srv._dispatch(
        {
            "command": "str_replace",
            "path": "/memories/p.txt",
            "old_str": "beta",
            "new_str": "gamma",
        }
    )
    assert (
        out
        == "No replacement was performed, old_str `beta` did not appear verbatim in /memories/p.txt."
    )


def test_str_replace_multiple_matches_returns_line_list() -> None:
    srv, _ = _server()
    body = "alpha\nbeta\nalpha\nbeta\n"
    srv._dispatch({"command": "create", "path": "/memories/p.txt", "file_text": body})
    out = srv._dispatch(
        {
            "command": "str_replace",
            "path": "/memories/p.txt",
            "old_str": "alpha",
            "new_str": "ALPHA",
        }
    )
    assert "Multiple occurrences of old_str `alpha`" in out
    assert "lines: 1, 3" in out


def test_str_replace_missing_file_errors() -> None:
    srv, _ = _server()
    res = srv.handle(
        {"command": "str_replace", "path": "/memories/nope.txt", "old_str": "x", "new_str": "y"}
    )
    assert res["is_error"] is True
    assert "/memories/nope.txt does not exist" in res["content"]


# ----------------------------------------------------------------------
# insert
# ----------------------------------------------------------------------


def test_insert_at_zero_prepends() -> None:
    srv, store = _server()
    srv._dispatch({"command": "create", "path": "/memories/t.txt", "file_text": "alpha\nbeta"})
    srv._dispatch(
        {"command": "insert", "path": "/memories/t.txt", "insert_line": 0, "insert_text": "intro"}
    )
    assert _content(srv, store, "/memories/t.txt") == "intro\nalpha\nbeta"


def test_insert_at_middle() -> None:
    srv, store = _server()
    srv._dispatch({"command": "create", "path": "/memories/t.txt", "file_text": "alpha\nbeta\ngamma"})
    srv._dispatch(
        {"command": "insert", "path": "/memories/t.txt", "insert_line": 2, "insert_text": "MID"}
    )
    assert _content(srv, store, "/memories/t.txt") == "alpha\nbeta\nMID\ngamma"


def test_insert_invalid_line_errors() -> None:
    srv, _ = _server()
    srv._dispatch({"command": "create", "path": "/memories/t.txt", "file_text": "a\nb"})
    res = srv.handle({"command": "insert", "path": "/memories/t.txt", "insert_line": 99, "insert_text": "x"})
    assert res["is_error"] is True
    assert "Invalid `insert_line` parameter: 99" in res["content"]


# ----------------------------------------------------------------------
# delete
# ----------------------------------------------------------------------


def test_delete_file_succeeds() -> None:
    srv, _ = _server()
    srv._dispatch({"command": "create", "path": "/memories/del.txt", "file_text": "x"})
    out = srv._dispatch({"command": "delete", "path": "/memories/del.txt"})
    assert out == "Successfully deleted /memories/del.txt"


def test_delete_directory_recursive() -> None:
    srv, store = _server()
    srv._dispatch({"command": "create", "path": "/memories/sub/a.txt", "file_text": "1"})
    srv._dispatch({"command": "create", "path": "/memories/sub/b.txt", "file_text": "2"})
    srv._dispatch({"command": "create", "path": "/memories/keep.txt", "file_text": "k"})
    srv._dispatch({"command": "delete", "path": "/memories/sub"})
    files = srv._list_files()  # type: ignore[attr-defined]
    assert "/memories/sub/a.txt" not in files
    assert "/memories/sub/b.txt" not in files
    assert "/memories/keep.txt" in files


def test_delete_missing_path_errors() -> None:
    srv, _ = _server()
    res = srv.handle({"command": "delete", "path": "/memories/nope"})
    assert res["is_error"] is True
    assert res["content"] == "Error: The path /memories/nope does not exist"


# ----------------------------------------------------------------------
# rename
# ----------------------------------------------------------------------


def test_rename_file() -> None:
    srv, _ = _server()
    srv._dispatch({"command": "create", "path": "/memories/old.txt", "file_text": "stays"})
    out = srv._dispatch({"command": "rename", "old_path": "/memories/old.txt", "new_path": "/memories/new.txt"})
    assert out == "Successfully renamed /memories/old.txt to /memories/new.txt"
    files = srv._list_files()  # type: ignore[attr-defined]
    assert "/memories/new.txt" in files
    assert "/memories/old.txt" not in files
    assert files["/memories/new.txt"]["content"] == "stays"


def test_rename_directory_re_tags_descendants() -> None:
    srv, _ = _server()
    srv._dispatch({"command": "create", "path": "/memories/old/a.txt", "file_text": "A"})
    srv._dispatch({"command": "create", "path": "/memories/old/b.txt", "file_text": "B"})
    srv._dispatch({"command": "rename", "old_path": "/memories/old", "new_path": "/memories/new"})
    files = srv._list_files()  # type: ignore[attr-defined]
    assert "/memories/new/a.txt" in files
    assert "/memories/new/b.txt" in files
    assert "/memories/old/a.txt" not in files


def test_rename_missing_source_errors() -> None:
    srv, _ = _server()
    res = srv.handle({"command": "rename", "old_path": "/memories/nope", "new_path": "/memories/other"})
    assert res["is_error"] is True
    assert "Error: The path /memories/nope does not exist" in res["content"]


def test_rename_destination_exists_errors() -> None:
    srv, _ = _server()
    srv._dispatch({"command": "create", "path": "/memories/a.txt", "file_text": "a"})
    srv._dispatch({"command": "create", "path": "/memories/b.txt", "file_text": "b"})
    res = srv.handle({"command": "rename", "old_path": "/memories/a.txt", "new_path": "/memories/b.txt"})
    assert res["is_error"] is True
    assert res["content"] == "Error: The destination /memories/b.txt already exists"


# ----------------------------------------------------------------------
# Path traversal protection
# ----------------------------------------------------------------------


@pytest.mark.parametrize(
    "path",
    [
        "/etc/passwd",
        "/memories/../etc/passwd",
        "/memories/sub/..",
        "/memories/%2e%2e/passwd",
    ],
)
def test_path_outside_root_is_rejected(path: str) -> None:
    srv, _ = _server()
    res = srv.handle({"command": "view", "path": path})
    assert res["is_error"] is True


# ----------------------------------------------------------------------
# Dispatcher
# ----------------------------------------------------------------------


def test_handle_unwraps_full_tool_use_block_and_echoes_id() -> None:
    srv, _ = _server()
    res = srv.handle(
        {
            "type": "tool_use",
            "id": "toolu_TEST",
            "name": "memory",
            "input": {"command": "view", "path": "/memories"},
        }
    )
    assert res["type"] == "tool_result"
    assert res["tool_use_id"] == "toolu_TEST"
    # Empty memories — directory listing of root with no children.
    assert "Here're the files and directories" in res["content"]


def test_handle_unknown_command_errors() -> None:
    srv, _ = _server()
    res = srv.handle({"command": "summon_demon", "path": "/memories"})
    assert res["is_error"] is True
    assert "Unknown command" in res["content"]


# ----------------------------------------------------------------------
# Canonical fixture round-trip — guards against drift from the
# Anthropic memory-tool docs page.
# ----------------------------------------------------------------------


def test_canonical_fixture_shapes_round_trip(tmp_path) -> None:
    import json
    from pathlib import Path

    fixture = (
        Path(__file__).parent / "fixtures" / "anthropic_memory_tool_2025_08_18.json"
    )
    payload = json.loads(fixture.read_text(encoding="utf-8"))
    assert payload["tool_schema"]["type"] == TOOL_TYPE

    srv, _ = _server()
    # Pre-seed enough memory for the non-create exchanges to land on
    # something — this isn't exercising semantics, just shape.
    srv._dispatch(
        {"command": "create", "path": "/memories/preferences.txt", "file_text": "Favorite color: blue"}
    )
    srv._dispatch(
        {"command": "create", "path": "/memories/todo.txt", "file_text": "- one\n- two\n- three"}
    )
    srv._dispatch(
        {"command": "create", "path": "/memories/old_file.txt", "file_text": "stale"}
    )
    srv._dispatch(
        {"command": "create", "path": "/memories/draft.txt", "file_text": "draft body"}
    )
    srv._dispatch(
        {
            "command": "create",
            "path": "/memories/customer_service_guidelines.xml",
            "file_text": "<guidelines>...</guidelines>",
        }
    )

    for exchange in payload["exchanges"]:
        res = srv.handle(exchange["tool_use"])
        assert res["type"] == "tool_result"
        assert res["tool_use_id"] == exchange["tool_use"]["id"]
        assert "content" in res
        # Spec guarantees: only error responses set is_error.
        if res.get("is_error"):
            raise AssertionError(
                f"canonical exchange {exchange['name']!r} unexpectedly errored: {res['content']}"
            )
