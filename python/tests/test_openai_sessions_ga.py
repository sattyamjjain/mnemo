"""Tests for the GA OpenAI Agents SDK snapshot contract."""

from __future__ import annotations

import asyncio
import importlib.util
from pathlib import Path

import pytest

from mnemo.openai_sessions_ga import (
    MnemoSnapshotStore,
    SnapshotRef,
    WorkspaceStorage,
)


def test_snapshot_ref_uri_shape() -> None:
    ref = SnapshotRef(session_id="s1", snapshot_id="abc", created_at="2026-04-20T00:00:00Z")
    assert ref.as_uri() == "snapshot://s1/2026-04-20T00:00:00Z"


def test_workspace_storage_requires_matching_backend_args(tmp_path: Path) -> None:
    with pytest.raises(ValueError):
        WorkspaceStorage(backend="local")  # no workspace_root
    with pytest.raises(ValueError):
        WorkspaceStorage(backend="s3")  # no bucket
    with pytest.raises(ValueError):
        WorkspaceStorage(backend="azure")


def test_workspace_local_roundtrip(tmp_path: Path) -> None:
    ws = WorkspaceStorage(backend="local", workspace_root=tmp_path / "w")
    loc = ws.put("a/b/c.json", b"payload")
    assert Path(loc).read_bytes() == b"payload"
    assert ws.get(loc) == b"payload"
    ws.delete(loc)
    assert not Path(loc).exists()


def test_workspace_remote_backends_stub_until_deps_installed() -> None:
    ws = WorkspaceStorage(backend="s3", bucket="test-bucket")
    with pytest.raises(NotImplementedError):
        ws.put("k", b"x")


def test_requires_session_id(tmp_path: Path) -> None:
    with pytest.raises(ValueError):
        MnemoSnapshotStore(
            session_id="",
            db_path=str(tmp_path / "m.db"),
            workspace_root=tmp_path / "w",
        )


# --------------------------------------------------------- integration tests
_NATIVE_AVAILABLE = importlib.util.find_spec("mnemo._mnemo") is not None
integration = pytest.mark.skipif(
    not _NATIVE_AVAILABLE,
    reason="mnemo native extension not built (run `maturin develop` in python/).",
)


@integration
def test_save_and_load_inline_roundtrip(tmp_path: Path) -> None:
    async def scenario() -> None:
        store = MnemoSnapshotStore(
            session_id="inline-session",
            db_path=str(tmp_path / "snap.db"),
            agent_id="tester",
            workspace_root=tmp_path / "w",
        )
        run = {"cursor": 5, "tools": ["search"]}
        sandbox = {"files": {"notes.md": "hello"}}
        ref = await store.save_snapshot(run, sandbox)
        loaded_run, loaded_sandbox = await store.load_snapshot(ref)
        assert loaded_run == run
        assert loaded_sandbox == sandbox

    asyncio.run(scenario())


@integration
def test_list_snapshots_newest_first(tmp_path: Path) -> None:
    async def scenario() -> None:
        store = MnemoSnapshotStore(
            session_id="list-session",
            db_path=str(tmp_path / "snap.db"),
            workspace_root=tmp_path / "w",
        )
        refs = []
        for step in range(3):
            refs.append(await store.save_snapshot({"step": step}, {}))
            await asyncio.sleep(0.005)  # guarantee created_at ordering
        listed = await store.list_snapshots(limit=10)
        assert [r.snapshot_id for r in listed] == [
            refs[2].snapshot_id,
            refs[1].snapshot_id,
            refs[0].snapshot_id,
        ]

    asyncio.run(scenario())


@integration
def test_resume_latest(tmp_path: Path) -> None:
    async def scenario() -> None:
        store = MnemoSnapshotStore(
            session_id="resume-session",
            db_path=str(tmp_path / "snap.db"),
            workspace_root=tmp_path / "w",
        )
        for step in range(5):
            await store.save_snapshot({"step": step}, {"sbx": step})
            await asyncio.sleep(0.005)
        ref, run, sandbox = await store.resume(from_ref="latest")
        assert run["step"] == 4
        assert sandbox["sbx"] == 4
        assert isinstance(ref, SnapshotRef)

    asyncio.run(scenario())


@integration
def test_offloaded_payload_goes_through_workspace(tmp_path: Path) -> None:
    async def scenario() -> None:
        ws_root = tmp_path / "w"
        store = MnemoSnapshotStore(
            session_id="offload-session",
            db_path=str(tmp_path / "snap.db"),
            workspace_root=ws_root,
            # Force offload on every call.
            inline_threshold_bytes=1,
        )
        run = {"giant": "x" * 256}
        sandbox = {"files": {"data.bin": "y" * 256}}
        ref = await store.save_snapshot(run, sandbox)
        assert any(ws_root.rglob("*.json")), "payload should be written to workspace"
        loaded_run, loaded_sandbox = await store.load_snapshot(ref)
        assert loaded_run == run
        assert loaded_sandbox == sandbox

    asyncio.run(scenario())


@integration
def test_resume_empty_session_raises(tmp_path: Path) -> None:
    async def scenario() -> None:
        store = MnemoSnapshotStore(
            session_id="empty-session",
            db_path=str(tmp_path / "snap.db"),
            workspace_root=tmp_path / "w",
        )
        with pytest.raises(LookupError):
            await store.resume()

    asyncio.run(scenario())
