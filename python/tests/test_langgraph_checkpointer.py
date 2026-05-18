"""v0.4.5 — tests for `MnemoCheckpointer` (LangGraph 1.x adapter).

Coverage:
- put → get_tuple round-trip for a single thread.
- Thread isolation: two threads, separate state.
- Branch round-trip: `branch="dev"` flows through `config.configurable`.
- delete_thread: invokes `MnemoClient.forget` with the thread id.
- Stub methods: `list` yields nothing; `put_writes` is a no-op.
- Back-compat: `ASMDCheckpointer` is the same class as `MnemoCheckpointer`.

The tests stub `MnemoClient` so the suite does NOT spawn the mnemo
binary; the unit being exercised is the LangGraph-shape ↔ Mnemo-API
translation, not the underlying engine.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

import pytest

# Skip the whole module when LangGraph isn't installed, matching the
# soft-import pattern in `mnemo.__init__`.
pytest.importorskip("langgraph.checkpoint.base")

from mnemo import checkpointer as ckpt_module  # noqa: E402
from mnemo.checkpointer import ASMDCheckpointer, MnemoCheckpointer  # noqa: E402


@dataclass
class _FakeMnemoClient:
    """In-process MnemoClient stand-in.

    Implements only the surface `MnemoCheckpointer` actually calls:
    ``checkpoint`` / ``replay`` / ``forget``. Each call is recorded so
    tests can assert the LangGraph-shape ↔ Mnemo-API translation.
    """

    checkpoints: dict[tuple[str, str, str], dict[str, Any]] = field(default_factory=dict)
    """(thread_id, branch, checkpoint_id) → stored state."""
    last_checkpoint_per_branch: dict[tuple[str, str], str] = field(default_factory=dict)
    """(thread_id, branch) → most recent checkpoint_id, used when caller
    omits `checkpoint_id` from `get_tuple`."""
    forgets: list[list[str]] = field(default_factory=list)

    def checkpoint(
        self,
        thread_id: str,
        state_snapshot: dict[str, Any],
        branch_name: str = "main",
        label: str | None = None,
    ) -> dict[str, Any]:
        cid = f"ckpt-{len(self.checkpoints) + 1}"
        self.checkpoints[(thread_id, branch_name, cid)] = {
            "id": cid,
            "thread_id": thread_id,
            "branch_name": branch_name,
            "label": label,
            "state": state_snapshot,
            "created_at": "2026-05-18T00:00:00Z",
        }
        self.last_checkpoint_per_branch[(thread_id, branch_name)] = cid
        return {"checkpoint_id": cid}

    def replay(
        self,
        thread_id: str,
        checkpoint_id: str | None = None,
        branch_name: str = "main",
    ) -> dict[str, Any]:
        if checkpoint_id is None:
            checkpoint_id = self.last_checkpoint_per_branch.get((thread_id, branch_name))
            if checkpoint_id is None:
                raise KeyError(f"no checkpoint for thread={thread_id} branch={branch_name}")
        key = (thread_id, branch_name, checkpoint_id)
        if key not in self.checkpoints:
            raise KeyError(f"checkpoint missing: {key!r}")
        cp = self.checkpoints[key]
        return {"checkpoint": cp}

    def forget(self, ids: list[str]) -> dict[str, Any]:
        self.forgets.append(list(ids))
        return {"forgotten": list(ids), "errors": []}


def _build_checkpointer(monkeypatch: pytest.MonkeyPatch) -> tuple[MnemoCheckpointer, _FakeMnemoClient]:
    """Construct a `MnemoCheckpointer` whose `client` is a `_FakeMnemoClient`."""
    fake = _FakeMnemoClient()
    # Patch the symbol the checkpointer module resolved at import time
    # so `MnemoCheckpointer.__init__` picks up the fake.
    monkeypatch.setattr(ckpt_module, "MnemoClient", lambda **kwargs: fake)
    return MnemoCheckpointer(db_path=":memory:", agent_id="test-agent"), fake


def _config(thread_id: str, branch: str = "main", checkpoint_id: str | None = None) -> dict:
    cfg: dict[str, Any] = {"configurable": {"thread_id": thread_id, "branch": branch}}
    if checkpoint_id is not None:
        cfg["configurable"]["checkpoint_id"] = checkpoint_id
    return cfg


def _checkpoint(value: int) -> dict[str, Any]:
    return {
        "v": 1,
        "id": "n/a",
        "ts": "",
        "channel_values": {"counter": value},
        "channel_versions": {"counter": 1},
        "versions_seen": {},
    }


def test_put_then_get_tuple_round_trip(monkeypatch: pytest.MonkeyPatch) -> None:
    cp, fake = _build_checkpointer(monkeypatch)

    out = cp.put(_config("t1"), _checkpoint(7), {"step_type": "agent"}, {})

    assert out["configurable"]["thread_id"] == "t1"
    assert out["configurable"]["checkpoint_id"].startswith("ckpt-")
    assert len(fake.checkpoints) == 1
    cid = out["configurable"]["checkpoint_id"]
    stored = fake.checkpoints[("t1", "main", cid)]
    assert stored["state"]["channel_values"] == {"counter": 7}
    assert stored["label"] == "agent"

    # Now resolve via get_tuple using only the thread id (no
    # checkpoint_id in config) — the fake should fall back to the
    # most recent checkpoint for the branch.
    tup = cp.get_tuple(_config("t1"))
    assert tup is not None
    assert tup.checkpoint["id"] == cid
    assert tup.metadata == {"branch": "main"}


def test_thread_isolation(monkeypatch: pytest.MonkeyPatch) -> None:
    cp, fake = _build_checkpointer(monkeypatch)

    cp.put(_config("alpha"), _checkpoint(1), {"step_type": "agent"}, {})
    cp.put(_config("beta"), _checkpoint(99), {"step_type": "agent"}, {})

    # Each thread has its own checkpoint; replaying one must not return
    # the other's state.
    alpha = cp.get_tuple(_config("alpha"))
    beta = cp.get_tuple(_config("beta"))
    assert alpha is not None and beta is not None
    assert alpha.checkpoint["id"] != beta.checkpoint["id"]
    assert fake.last_checkpoint_per_branch[("alpha", "main")] != fake.last_checkpoint_per_branch[("beta", "main")]


def test_branch_round_trip(monkeypatch: pytest.MonkeyPatch) -> None:
    cp, fake = _build_checkpointer(monkeypatch)

    cp.put(_config("t1", branch="main"), _checkpoint(1), {"step_type": "agent"}, {})
    cp.put(_config("t1", branch="dev"), _checkpoint(2), {"step_type": "agent"}, {})

    # Two branches, two distinct stored checkpoints.
    assert len(fake.checkpoints) == 2

    # Each branch's get_tuple resolves to its own state.
    main_tup = cp.get_tuple(_config("t1", branch="main"))
    dev_tup = cp.get_tuple(_config("t1", branch="dev"))
    assert main_tup is not None and dev_tup is not None
    assert main_tup.metadata["branch"] == "main"
    assert dev_tup.metadata["branch"] == "dev"
    assert main_tup.checkpoint["id"] != dev_tup.checkpoint["id"]


def test_get_tuple_returns_none_when_replay_raises(monkeypatch: pytest.MonkeyPatch) -> None:
    cp, _fake = _build_checkpointer(monkeypatch)
    # No checkpoint written → fake.replay raises → get_tuple returns None.
    assert cp.get_tuple(_config("never-written")) is None


def test_delete_thread_calls_forget(monkeypatch: pytest.MonkeyPatch) -> None:
    cp, fake = _build_checkpointer(monkeypatch)

    cp.delete_thread(_config("doomed-thread"))

    assert fake.forgets == [["doomed-thread"]]


def test_list_yields_empty_iterator(monkeypatch: pytest.MonkeyPatch) -> None:
    cp, _fake = _build_checkpointer(monkeypatch)
    # `list` is a stub today — documented as such in the module
    # docstring. The contract under v0.4.5 is "returns an iterator that
    # yields nothing"; this test pins that contract so a future
    # implementation can flip the bit without sneaking in a default
    # that breaks callers.
    out = list(cp.list(_config("anything")))
    assert out == []


def test_put_writes_is_a_noop(monkeypatch: pytest.MonkeyPatch) -> None:
    cp, fake = _build_checkpointer(monkeypatch)
    # Returns None and does not touch the underlying client.
    result = cp.put_writes(_config("t1"), [("ch", {"key": "val"})], task_id="task-1")
    assert result is None
    assert fake.checkpoints == {}
    assert fake.forgets == []


def test_asmd_checkpointer_is_back_compat_alias() -> None:
    """`ASMDCheckpointer` and `MnemoCheckpointer` must be the same class.

    Anything else (subclass, separate class with the same methods)
    would silently break ``isinstance(x, ASMDCheckpointer)`` checks
    in code that pre-dates v0.4.5.
    """
    assert ASMDCheckpointer is MnemoCheckpointer
