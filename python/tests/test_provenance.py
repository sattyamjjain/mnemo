"""v0.4.0-rc3 (Task Q1) — pure-Python provenance SDK tests.

Mirrors the Rust ``provenance::tests`` cases:

1. A round-trip from `_compute_hmac` through `verify_read_provenance`
   passes.
2. Tampered record content fails the hash drift check.
3. Wrong key id fails the key-id pre-check.
4. Wrong key bytes fail the HMAC check.

These run without ``maturin develop`` — the SDK is pure Python so
auditors can verify offline without compiling Rust.
"""

from __future__ import annotations

import hashlib
import hmac
import os

import pytest

from mnemo.provenance import (
    ProvenanceSigner,
    ProvenanceVerificationError,
    ReadProvenance,
    RecordRef,
    _compute_content_hash,
    verify_read_provenance,
)


def _make_record(rid: str, content: str, agent_id: str = "agent-1") -> dict:
    created_at = "2026-04-26T12:00:00Z"
    return {
        "id": rid,
        "content": content,
        "agent_id": agent_id,
        "created_at": created_at,
    }


def _build_receipt(
    signer: ProvenanceSigner, records: list[dict], query: str = "what is X"
) -> ReadProvenance:
    refs = []
    for r in records:
        ch = _compute_content_hash(
            content=r["content"], agent_id=r["agent_id"], created_at=r["created_at"]
        )
        refs.append(RecordRef(id=r["id"], content_hash=ch))
    query_hash = hashlib.sha256(query.encode("utf-8")).digest()

    # Same wire-shape MAC the Rust signer produces.
    mac = hmac.new(signer.key, digestmod=hashlib.sha256)
    read_id = "01900000-0000-7000-8000-000000000001"
    agent_id = records[0]["agent_id"]
    mac.update(read_id.encode("utf-8"))
    mac.update(b"|")
    mac.update(agent_id.encode("utf-8"))
    mac.update(b"|")
    mac.update(query_hash)
    mac.update(b"|")
    for ref in refs:
        mac.update(ref.id.encode("utf-8"))
        mac.update(b":")
        mac.update(ref.content_hash)
        mac.update(b"|")
    tag = mac.digest()

    return ReadProvenance(
        read_id=read_id,
        agent_id=agent_id,
        query_hash=query_hash,
        derived_from=tuple(refs),
        hmac=tag,
        hmac_key_id=signer.key_id,
        ts="2026-04-26T12:00:01Z",
    )


def test_signer_rejects_short_key():
    with pytest.raises(ValueError):
        ProvenanceSigner(key_id="x", key=b"too-short")


def test_round_trip_verifies():
    signer = ProvenanceSigner(key_id="k1", key=os.urandom(32))
    records = [_make_record("r1", "hello"), _make_record("r2", "world")]
    receipt = _build_receipt(signer, records)
    verify_read_provenance(receipt, records, signer)


def test_tampered_content_is_rejected():
    signer = ProvenanceSigner(key_id="k1", key=os.urandom(32))
    records = [_make_record("r1", "hello")]
    receipt = _build_receipt(signer, records)
    records[0]["content"] = "MALICIOUS"
    with pytest.raises(ProvenanceVerificationError):
        verify_read_provenance(receipt, records, signer)


def test_wrong_key_id_is_rejected():
    signer = ProvenanceSigner(key_id="k1", key=os.urandom(32))
    other = ProvenanceSigner(key_id="k2", key=signer.key)
    records = [_make_record("r1", "hello")]
    receipt = _build_receipt(signer, records)
    with pytest.raises(ProvenanceVerificationError):
        verify_read_provenance(receipt, records, other)


def test_wrong_key_bytes_are_rejected():
    signer = ProvenanceSigner(key_id="k1", key=os.urandom(32))
    other = ProvenanceSigner(key_id="k1", key=os.urandom(32))
    records = [_make_record("r1", "hello")]
    receipt = _build_receipt(signer, records)
    with pytest.raises(ProvenanceVerificationError):
        verify_read_provenance(receipt, records, other)


def test_record_count_mismatch_is_rejected():
    signer = ProvenanceSigner(key_id="k1", key=os.urandom(32))
    records = [_make_record("r1", "hello"), _make_record("r2", "world")]
    receipt = _build_receipt(signer, records)
    with pytest.raises(ProvenanceVerificationError):
        verify_read_provenance(receipt, records[:1], signer)
