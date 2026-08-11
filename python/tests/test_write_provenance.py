"""Python SDK surface for write provenance + FORGET BY PROVENANCE.

Exercises the native ``MnemoClient`` methods added alongside the Rust engine:

- ``write_provenance(memory_id)``  -> who wrote it, under what authority
- ``writes_by_principal(principal)`` / ``writes_by_session(session_id)``
- ``verify_provenance_chain()``     -> tamper-evidence over the append history
- ``forget_by_principal`` / ``forget_by_session`` -> FORGET BY PROVENANCE

Skips cleanly when the native extension has not been built (``maturin develop``).
Deletion semantics themselves are proven in the Rust integration tests; here we
assert the SDK can *read* provenance and *clean up* by principal/session, and
that the audit trail survives the wipe (wiping is not remediation).
"""

from __future__ import annotations

import pytest

try:
    from mnemo._mnemo import MnemoClient  # type: ignore[attr-defined]

    _NATIVE = True
except Exception:  # pragma: no cover - native ext optional at import time
    MnemoClient = None  # type: ignore[assignment,misc]
    _NATIVE = False

pytestmark = pytest.mark.skipif(
    not _NATIVE, reason="native extension not built (run `maturin develop`)"
)


def _client(tmp_path):
    db = str(tmp_path / "prov.mnemo.db")
    return MnemoClient(
        db_path=db,
        agent_id="alice",
        with_noop_embedding=True,
    )


def test_write_provenance_is_recorded_and_queryable(tmp_path):
    client = _client(tmp_path)
    r1 = client.remember("first fact", thread_id="sess-1")
    client.remember("second fact", thread_id="sess-1")

    mid = r1["id"]

    # By memory id.
    prov = client.write_provenance(mid)
    assert prov is not None
    assert prov["principal"] == "alice"
    assert prov["op"] == "remember"
    assert prov["memory_id"] == mid
    assert prov["session_id"] == "sess-1"
    # Content hash crosses the boundary as hex text, not a byte array.
    assert isinstance(prov["content_hash"], str) and len(prov["content_hash"]) == 64

    # By principal and by session.
    assert len(client.writes_by_principal("alice")) == 2
    assert len(client.writes_by_session("sess-1")) == 2
    assert client.writes_by_principal("nobody") == []


def test_provenance_chain_verifies(tmp_path):
    client = _client(tmp_path)
    for i in range(3):
        client.remember(f"fact {i}", thread_id="sess-chain")
    result = client.verify_provenance_chain()
    assert result["valid"] is True
    assert result["verified_records"] == result["total_records"] >= 3
    assert result["first_broken_at"] is None


def test_forget_by_session_revokes_but_keeps_audit_trail(tmp_path):
    client = _client(tmp_path)
    client.remember("m1", thread_id="doomed")
    client.remember("m2", thread_id="doomed")
    client.remember("survivor", thread_id="kept")

    resp = client.forget_by_session("doomed", "hard_delete")
    assert len(resp["forgotten"]) == 2
    assert resp["errors"] == []

    # The audit trail is durable: FORGET BY PROVENANCE removes the memories, not
    # the record of who wrote them.
    assert len(client.writes_by_session("doomed")) == 2
    # An unrelated session is untouched.
    assert len(client.writes_by_session("kept")) == 1


def test_forget_by_principal_targets_one_writer(tmp_path):
    client = _client(tmp_path)
    client.remember("a1", thread_id="s")
    resp = client.forget_by_principal("alice", "hard_delete")
    assert len(resp["forgotten"]) == 1
    # Nothing written by an unknown principal -> nothing forgotten.
    resp2 = client.forget_by_principal("mallory", "hard_delete")
    assert resp2["forgotten"] == []
