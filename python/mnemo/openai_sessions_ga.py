"""OpenAI Agents SDK GA resume contract — Mnemo-backed snapshot store.

The 2026-04-16 GA release of ``openai-agents`` generalised the preview
``Session`` protocol into three cooperating interfaces:

* ``SessionStore`` — per-turn conversation items (supported today by
  :class:`mnemo.openai_sessions.MnemoSessionStore`).
* ``SnapshotStore`` — durable ``RunState`` + ``SandboxSessionState`` blobs
  that let a worker resume a crashed run from the last confirmed step.
* ``ResumeProvider`` — locator layer that hands a ``SnapshotRef`` back to
  the runtime when a previous run needs to be continued.

This module implements the latter two on top of Mnemo checkpoints so a
single DuckDB file (or Postgres deployment) becomes the system of record
for both chat history and run-state snapshots. Payloads above
``inline_threshold_bytes`` are offloaded to an object-storage backend
(local filesystem, S3, R2, GCS, or Azure Blob); Mnemo stores the pointer
and a SHA-256 content digest for integrity.

Example::

    from mnemo.openai_sessions_ga import MnemoSnapshotStore

    store = MnemoSnapshotStore(
        db_path="agent.mnemo.db",
        agent_id="user-42",
        session_id="support-2026-04-20",
        workspace_backend="local",
        workspace_root="/var/mnemo/snapshots",
    )

    ref = await store.save_snapshot(run_state, sandbox_state)
    # ... process crashes ...
    run_state, sandbox_state = await store.load_snapshot(ref)

Install::

    pip install mnemo[openai-sandbox]
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import shutil
import threading
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal, Optional

WorkspaceBackend = Literal["local", "s3", "r2", "gcs", "azure"]

_SNAPSHOT_TAG_PREFIX = "_snapshot:"
_INLINE_THRESHOLD_DEFAULT = 64 * 1024  # 64 KiB — above this we offload


def _snapshot_tag(session_id: str) -> str:
    return f"{_SNAPSHOT_TAG_PREFIX}{session_id}"


def _sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


@dataclass(frozen=True)
class SnapshotRef:
    """Opaque reference returned by :meth:`save_snapshot`.

    Callers must treat this as opaque; the concrete encoding is an
    implementation detail and may change between releases.
    """

    session_id: str
    snapshot_id: str
    created_at: str

    def as_uri(self) -> str:
        """Stable URI the Mnemo MCP resource layer can surface."""
        return f"snapshot://{self.session_id}/{self.created_at}"


class WorkspaceStorage:
    """Pluggable backend used when snapshot payloads exceed the inline
    threshold. Non-local backends are stubbed here and raise on use — the
    real adapter plugs a third-party SDK (boto3, google-cloud-storage,
    etc.) into the ``_put``/``_get`` methods.
    """

    def __init__(
        self,
        backend: WorkspaceBackend,
        workspace_root: Optional[Path] = None,
        bucket: Optional[str] = None,
    ) -> None:
        self.backend = backend
        self.workspace_root = Path(workspace_root) if workspace_root else None
        self.bucket = bucket
        if backend == "local":
            if self.workspace_root is None:
                raise ValueError("workspace_root is required for backend='local'")
            self.workspace_root.mkdir(parents=True, exist_ok=True)
        elif backend not in ("s3", "r2", "gcs", "azure"):
            raise ValueError(f"unknown workspace_backend: {backend}")
        else:
            if self.bucket is None:
                raise ValueError(f"bucket is required for backend='{backend}'")

    # --------------------------------------------------------------- writes
    def put(self, key: str, payload: bytes) -> str:
        """Persist ``payload`` under ``key`` and return the resolved locator."""
        if self.backend == "local":
            assert self.workspace_root is not None
            path = self.workspace_root / key
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(payload)
            return str(path)
        raise NotImplementedError(
            f"workspace_backend='{self.backend}' requires the optional "
            f"mnemo[openai-sandbox-{self.backend}] extra which is not installed "
            f"in this environment"
        )

    def get(self, locator: str) -> bytes:
        if self.backend == "local":
            return Path(locator).read_bytes()
        raise NotImplementedError(
            f"workspace_backend='{self.backend}' get() is stubbed; plug in "
            f"the object-storage SDK in a subclass or install the matching "
            f"mnemo[openai-sandbox-{self.backend}] extra"
        )

    def delete(self, locator: str) -> None:
        if self.backend == "local":
            try:
                Path(locator).unlink()
            except FileNotFoundError:
                pass
            return
        raise NotImplementedError(
            f"workspace_backend='{self.backend}' delete() is stubbed"
        )


class MnemoSnapshotStore:
    """Durable ``RunState`` + ``SandboxSessionState`` store for the GA SDK.

    Args:
        session_id: Stable identifier for this run.
        db_path: DuckDB path or SQLAlchemy URL for Mnemo storage.
        agent_id: Mnemo namespacing.
        workspace_backend: "local" | "s3" | "r2" | "gcs" | "azure".
        workspace_root: Filesystem root when ``workspace_backend="local"``.
        bucket: Bucket/container name for non-local backends.
        inline_threshold_bytes: Payloads at or below this size are stored
            inline in Mnemo; above it they go to the workspace backend and
            Mnemo keeps only the pointer + SHA-256.
    """

    def __init__(
        self,
        session_id: str,
        db_path: str = "mnemo.db",
        agent_id: str = "default",
        workspace_backend: WorkspaceBackend = "local",
        workspace_root: Optional[Path] = None,
        bucket: Optional[str] = None,
        inline_threshold_bytes: int = _INLINE_THRESHOLD_DEFAULT,
        openai_api_key: Optional[str] = None,
        embedding_model: str = "text-embedding-3-small",
        dimensions: int = 1536,
    ) -> None:
        if not session_id:
            raise ValueError("session_id is required")
        self.session_id = session_id
        self.db_path = db_path
        self.agent_id = agent_id
        self.openai_api_key = openai_api_key
        self.embedding_model = embedding_model
        self.dimensions = dimensions
        self.inline_threshold_bytes = inline_threshold_bytes
        self.workspace = WorkspaceStorage(
            backend=workspace_backend,
            workspace_root=workspace_root,
            bucket=bucket,
        )
        self._client = None
        self._lock = threading.Lock()
        self._tag = _snapshot_tag(session_id)
        self._listing_query = f"_snapshot_listing_{session_id}"

    def _ensure_client(self):
        if self._client is not None:
            return self._client
        try:
            from mnemo._mnemo import MnemoClient  # type: ignore[attr-defined]
        except ImportError as exc:  # pragma: no cover
            from mnemo.availability import MnemoClientUnavailable

            raise MnemoClientUnavailable(
                "MnemoSnapshotStore needs the native mnemo._mnemo extension"
            ) from exc
        self._client = MnemoClient(
            db_path=self.db_path,
            agent_id=self.agent_id,
            openai_api_key=self.openai_api_key,
            embedding_model=self.embedding_model,
            dimensions=self.dimensions,
        )
        return self._client

    # --------------------------------------------------------- marshalling
    @staticmethod
    def _serialize(obj: Any) -> bytes:
        return json.dumps(obj, ensure_ascii=False, sort_keys=True).encode("utf-8")

    @staticmethod
    def _deserialize(data: bytes) -> Any:
        return json.loads(data.decode("utf-8"))

    def _store_payload(self, kind: str, snapshot_id: str, payload: bytes) -> dict[str, Any]:
        """Return the descriptor we persist in Mnemo for a single payload.

        For inline payloads the raw bytes live in the descriptor as base64;
        for offloaded payloads only the locator + SHA-256 stays in Mnemo.
        """
        digest = _sha256_hex(payload)
        if len(payload) <= self.inline_threshold_bytes:
            import base64 as _b64

            return {
                "storage": "inline",
                "kind": kind,
                "size": len(payload),
                "sha256": digest,
                "data_b64": _b64.b64encode(payload).decode("ascii"),
            }
        key = f"{self.session_id}/{snapshot_id}/{kind}.json"
        locator = self.workspace.put(key, payload)
        return {
            "storage": "workspace",
            "kind": kind,
            "size": len(payload),
            "sha256": digest,
            "backend": self.workspace.backend,
            "locator": locator,
        }

    def _load_payload(self, descriptor: dict[str, Any]) -> Any:
        if descriptor["storage"] == "inline":
            import base64 as _b64

            raw = _b64.b64decode(descriptor["data_b64"])
        else:
            raw = self.workspace.get(descriptor["locator"])
        if _sha256_hex(raw) != descriptor["sha256"]:
            raise ValueError(
                f"snapshot payload SHA-256 mismatch for {descriptor.get('kind')}"
            )
        return self._deserialize(raw)

    # ---------------------------------------------------- SnapshotStore API
    async def save_snapshot(
        self,
        run_state: Any,
        sandbox_state: Any,
    ) -> SnapshotRef:
        """Persist a new snapshot and return its opaque :class:`SnapshotRef`."""

        def _run() -> SnapshotRef:
            client = self._ensure_client()
            with self._lock:
                snapshot_id = uuid.uuid4().hex
                run_desc = self._store_payload(
                    "run_state", snapshot_id, self._serialize(run_state)
                )
                sandbox_desc = self._store_payload(
                    "sandbox_state", snapshot_id, self._serialize(sandbox_state)
                )
                created_at = _utc_now_iso()
                body = {
                    "_snapshot_id": snapshot_id,
                    "_session_id": self.session_id,
                    "_created_at": created_at,
                    "run_state": run_desc,
                    "sandbox_state": sandbox_desc,
                }
                client.remember(
                    content=json.dumps(body, ensure_ascii=False),
                    memory_type="episodic",
                    importance=0.5,
                    tags=[self._tag],
                    metadata={
                        "session_id": self.session_id,
                        "snapshot_id": snapshot_id,
                        "created_at": created_at,
                    },
                )
                return SnapshotRef(
                    session_id=self.session_id,
                    snapshot_id=snapshot_id,
                    created_at=created_at,
                )

        return await asyncio.to_thread(_run)

    async def load_snapshot(self, ref: SnapshotRef) -> tuple[Any, Any]:
        """Return ``(run_state, sandbox_state)`` for the given ref."""
        snap = await asyncio.to_thread(lambda: self._load_snapshot_record(ref.snapshot_id))
        if snap is None:
            raise LookupError(f"no snapshot with id={ref.snapshot_id}")
        return (
            self._load_payload(snap["run_state"]),
            self._load_payload(snap["sandbox_state"]),
        )

    async def list_snapshots(
        self,
        limit: Optional[int] = 100,
    ) -> list[SnapshotRef]:
        """Return snapshots for this session, newest-first."""

        def _run() -> list[SnapshotRef]:
            entries = self._load_all_snapshots()
            entries.sort(key=lambda e: e["_created_at"], reverse=True)
            if limit is not None and limit > 0:
                entries = entries[:limit]
            return [
                SnapshotRef(
                    session_id=self.session_id,
                    snapshot_id=e["_snapshot_id"],
                    created_at=e["_created_at"],
                )
                for e in entries
            ]

        return await asyncio.to_thread(_run)

    # ---------------------------------------------------- ResumeProvider API
    async def resume(
        self,
        from_ref: SnapshotRef | Literal["latest"] = "latest",
    ) -> tuple[SnapshotRef, Any, Any]:
        """Return ``(ref, run_state, sandbox_state)`` for resumption.

        Pass ``"latest"`` (the default) to resume from the most recent
        snapshot, or a specific :class:`SnapshotRef` for deterministic
        resumption at a known point.
        """
        if from_ref == "latest":
            refs = await self.list_snapshots(limit=1)
            if not refs:
                raise LookupError(
                    f"no snapshots exist for session_id={self.session_id!r}"
                )
            ref = refs[0]
        else:
            ref = from_ref
        run, sandbox = await self.load_snapshot(ref)
        return ref, run, sandbox

    # ---------------------------------------------------------- internals
    def _load_all_snapshots(self) -> list[dict[str, Any]]:
        client = self._ensure_client()
        result = client.recall(
            query=self._listing_query,
            limit=1000,
            tags=[self._tag],
            strategy="exact",
        )
        memories = result.get("memories", []) if isinstance(result, dict) else []
        entries: list[dict[str, Any]] = []
        for m in memories:
            try:
                entries.append(json.loads(m["content"]))
            except (ValueError, KeyError):
                continue
        return entries

    def _load_snapshot_record(self, snapshot_id: str) -> Optional[dict[str, Any]]:
        for entry in self._load_all_snapshots():
            if entry.get("_snapshot_id") == snapshot_id:
                return entry
        return None


def _utc_now_iso() -> str:
    from datetime import datetime, timezone

    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")


__all__ = [
    "MnemoSnapshotStore",
    "SnapshotRef",
    "WorkspaceStorage",
    "WorkspaceBackend",
]


# `shutil` is imported for future workspace-clear helpers; referenced here
# so import-time side effects stay consistent across linters.
_ = shutil
