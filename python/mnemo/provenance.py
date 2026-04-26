"""v0.4.0-rc3 (Task Q1) — Pure-Python provenance SDK.

Wraps the same wire shape as the Rust ``ProvenanceSigner`` /
``ReadProvenance`` types so Python tooling can verify a receipt
offline (an auditor's notebook, a CI pipeline checking that a model
response cited the records it claims) without compiling Rust.

The receipt itself is produced server-side by Mnemo when the caller
sets ``with_provenance=True`` on a recall. Use
:func:`verify_read_provenance` to check that:

1. Each cited record's stored ``content_hash`` matches what the
   receipt claims (post-recall tamper detection).
2. The receipt's HMAC binds the cited records to the right read.

Example::

    from mnemo.provenance import ProvenanceSigner, verify_read_provenance

    # Operator side: ship the same key the Rust ProvenanceSigner uses.
    signer = ProvenanceSigner(key_id="mnemo-prov-2026-04", key=os.urandom(32))

    # Auditor side: a verified recall response carries `provenance`.
    verify_read_provenance(receipt, records, signer)  # raises on mismatch
"""

from __future__ import annotations

import base64
import hashlib
import hmac
from dataclasses import dataclass
from typing import Any, Mapping, Sequence


@dataclass(frozen=True)
class RecordRef:
    """One source record cited by a :class:`ReadProvenance`."""

    id: str
    content_hash: bytes
    prev_hash: bytes | None = None

    @classmethod
    def from_wire(cls, payload: Mapping[str, Any]) -> "RecordRef":
        ch = payload["content_hash"]
        ph = payload.get("prev_hash")
        return cls(
            id=str(payload["id"]),
            content_hash=_decode_bytes(ch),
            prev_hash=_decode_bytes(ph) if ph is not None else None,
        )


@dataclass(frozen=True)
class ReadProvenance:
    """A signed receipt the engine returns alongside a recall response."""

    read_id: str
    agent_id: str
    query_hash: bytes
    derived_from: tuple[RecordRef, ...]
    hmac: bytes
    hmac_key_id: str
    ts: str

    @classmethod
    def from_wire(cls, payload: Mapping[str, Any]) -> "ReadProvenance":
        derived = tuple(
            RecordRef.from_wire(r) for r in payload.get("derived_from", [])
        )
        return cls(
            read_id=str(payload["read_id"]),
            agent_id=str(payload["agent_id"]),
            query_hash=_decode_bytes(payload["query_hash"]),
            derived_from=derived,
            hmac=_decode_bytes(payload["hmac"]),
            hmac_key_id=str(payload["hmac_key_id"]),
            ts=str(payload["ts"]),
        )


@dataclass(frozen=True)
class ProvenanceSigner:
    """A pre-shared HMAC key + identifier.

    Compatible with the Rust ``ProvenanceSigner`` — the ``key_id`` is
    a stable string that lands in the receipt so verifiers can swap
    in the historical key during rotation.
    """

    key_id: str
    key: bytes

    def __post_init__(self) -> None:
        if len(self.key) < 32:
            raise ValueError(
                f"provenance key must be >= 32 bytes (got {len(self.key)})"
            )


class ProvenanceVerificationError(ValueError):
    """Raised when a receipt fails any verification step."""


def verify_read_provenance(
    receipt: ReadProvenance,
    records: Sequence[Mapping[str, Any]],
    signer: ProvenanceSigner,
) -> None:
    """Verify a receipt produced by Mnemo.

    Parameters
    ----------
    receipt : ReadProvenance
        The decoded receipt (use :meth:`ReadProvenance.from_wire`).
    records : sequence of mapping
        The cited records. Each must have ``id``, ``content``,
        ``agent_id``, and ``created_at`` (the same fields the Rust
        ``MemoryRecord`` ships over the wire). Mnemo recomputes the
        ``content_hash`` from these fields exactly the way the Rust
        ``hash::compute_content_hash`` does.
    signer : ProvenanceSigner
        The signer holding the key ``receipt.hmac_key_id`` was issued
        under. For rotated keys, look up the historical signer first.

    Raises
    ------
    ProvenanceVerificationError
        If the key id does not match, any record's content hash has
        drifted, or the HMAC tag does not match the key.
    """
    if receipt.hmac_key_id != signer.key_id:
        raise ProvenanceVerificationError(
            f"signer key id {signer.key_id!r} does not match "
            f"receipt key id {receipt.hmac_key_id!r}"
        )

    if len(receipt.derived_from) != len(records):
        raise ProvenanceVerificationError(
            f"receipt cites {len(receipt.derived_from)} records but "
            f"{len(records)} were supplied for verification"
        )

    by_id = {ref.id: ref for ref in receipt.derived_from}
    for rec in records:
        rid = str(rec["id"])
        if rid not in by_id:
            raise ProvenanceVerificationError(
                f"record id {rid} is not cited by the receipt"
            )
        ref = by_id[rid]
        recomputed = _compute_content_hash(
            content=rec["content"],
            agent_id=rec["agent_id"],
            created_at=rec["created_at"],
        )
        if recomputed != ref.content_hash:
            raise ProvenanceVerificationError(
                f"record {rid} content_hash drift: storage={recomputed.hex()} "
                f"receipt={ref.content_hash.hex()}"
            )

    expected = _compute_hmac(receipt, signer)
    if not hmac.compare_digest(expected, receipt.hmac):
        raise ProvenanceVerificationError("receipt HMAC mismatch")


def _compute_content_hash(*, content: str, agent_id: str, created_at: str) -> bytes:
    """Mirror of ``mnemo_core::hash::compute_content_hash``.

    The Rust impl concatenates ``agent_id || ":" || created_at || ":" || content``
    and SHA-256s it. Keep the Python and Rust hashers in lock-step — a
    drift here silently disables verification.
    """
    h = hashlib.sha256()
    h.update(agent_id.encode("utf-8"))
    h.update(b":")
    h.update(created_at.encode("utf-8"))
    h.update(b":")
    h.update(content.encode("utf-8"))
    return h.digest()


def _compute_hmac(receipt: ReadProvenance, signer: ProvenanceSigner) -> bytes:
    mac = hmac.new(signer.key, digestmod=hashlib.sha256)
    mac.update(receipt.read_id.encode("utf-8"))
    mac.update(b"|")
    mac.update(receipt.agent_id.encode("utf-8"))
    mac.update(b"|")
    mac.update(receipt.query_hash)
    mac.update(b"|")
    for ref in receipt.derived_from:
        mac.update(ref.id.encode("utf-8"))
        mac.update(b":")
        mac.update(ref.content_hash)
        mac.update(b"|")
    return mac.digest()


def _decode_bytes(value: Any) -> bytes:
    """Accept bytes / hex strings / base64 strings on the wire — the
    Rust serializer can render any of the three depending on
    ``serde_with`` annotation.
    """
    if isinstance(value, (bytes, bytearray)):
        return bytes(value)
    if isinstance(value, list) and all(isinstance(b, int) for b in value):
        return bytes(value)
    if isinstance(value, str):
        # Try hex first (most common). Fall back to base64.
        try:
            return bytes.fromhex(value)
        except ValueError:
            return base64.b64decode(value)
    raise TypeError(f"cannot decode bytes from {type(value).__name__}")


__all__ = [
    "ProvenanceSigner",
    "ProvenanceVerificationError",
    "ReadProvenance",
    "RecordRef",
    "verify_read_provenance",
]
