"""Google Cloud Storage workspace backend for Mnemo snapshots (#39).

Opt-in dependency: ``pip install mnemo-db[openai-sandbox-gcs]`` pulls
`google-cloud-storage`. The object layout is byte-identical to the S3
backend so a snapshot is portable between providers by copying objects:

::

    gs://<bucket>/<key_prefix>/manifest.json
    gs://<bucket>/<key_prefix>/manifest.sig
    gs://<bucket>/<key_prefix>/files/<rel_path>

Why this is a standalone class and not an ``S3Workspace`` subclass
------------------------------------------------------------------

Cloudflare R2 subclasses :class:`~mnemo.openai_sandbox.s3_workspace.S3Workspace`
because R2 speaks the S3 wire protocol. **GCS does not.** Its
interoperability XML API is a partial S3 emulation that does not cover
the paginated ``list_objects_v2`` / batch ``delete_objects`` calls the S3
backend relies on, so subclassing would inherit methods that fail at
runtime against a real bucket. Per issue #39 ("single class per backend;
no abstract base layer") this uses the native
``google-cloud-storage`` client directly.

The signing contract is **unchanged** — `dump_workspace` /
`load_workspace` from :mod:`mnemo.openai_sandbox.manifest` do all the
Ed25519 and per-blob digest work. Only the storage adapter differs.

Install
-------

::

    pip install mnemo-db[openai-sandbox-gcs]

Construct
---------

::

    from mnemo.openai_sandbox.gcs_workspace import GCSWorkspace

    # Application Default Credentials (gcloud auth application-default login,
    # a service-account JSON via GOOGLE_APPLICATION_CREDENTIALS, or the
    # metadata server on GCE/GKE/Cloud Run):
    ws = GCSWorkspace(bucket="agent-snapshots")

    # Or inject a pre-built client (what the tests do):
    from google.cloud import storage
    ws = GCSWorkspace(bucket="agent-snapshots", client=storage.Client())
"""

from __future__ import annotations

import hashlib
from pathlib import Path
from typing import Any, Iterable

from mnemo.openai_sandbox.manifest import (
    SnapshotManifest,
    WorkspaceSigner,
    dump_workspace,
    load_workspace,
)
from mnemo.openai_sandbox.spec import RemoteSnapshotSpec, WorkspaceBackend

try:
    from google.cloud import storage as _gcs  # type: ignore[import-not-found]
except ImportError as _gcs_exc:  # pragma: no cover — exercised by the extra
    raise ImportError(
        "GCSWorkspace requires `google-cloud-storage`. Install with "
        "`pip install mnemo-db[openai-sandbox-gcs]`."
    ) from _gcs_exc


_MANIFEST_KEY = "manifest.json"
_SIGNATURE_KEY = "manifest.sig"
_FILES_PREFIX = "files/"


class GCSWorkspace:
    """Google Cloud Storage workspace storage.

    Accepts an already-configured ``google.cloud.storage.Client`` so
    tests can inject a fake or the ``fake-gcs-server`` emulator.
    Production callers usually let Application Default Credentials
    resolve and pass nothing but the bucket name.
    """

    backend_name: WorkspaceBackend = "gcs"

    def __init__(
        self,
        bucket: str,
        client: Any | None = None,
        *,
        key_prefix_root: str = "",
        project: str | None = None,
    ) -> None:
        if not bucket:
            raise ValueError("GCSWorkspace: bucket is required")
        self.bucket_name = bucket
        self.project = project
        self.client = client if client is not None else self._build_default_client()
        # `bucket()` is a local handle — it does not perform a network
        # round-trip, so constructing a workspace stays cheap and offline.
        self.bucket = self.client.bucket(bucket)
        self.key_prefix_root = key_prefix_root.rstrip("/")

    def _build_default_client(self) -> Any:
        """Resolve Application Default Credentials.

        Kept as its own method (mirroring ``S3Workspace``) so a subclass
        targeting a GCS-compatible endpoint can override credential
        wiring without re-implementing the storage contract.
        """
        if self.project:
            return _gcs.Client(project=self.project)
        return _gcs.Client()

    # ------------------------------------------------------------- helpers
    def _full_key(self, *parts: str) -> str:
        bits: Iterable[str] = filter(None, (self.key_prefix_root, *parts))
        return "/".join(bits)

    def _put(self, key: str, body: bytes) -> None:
        # `content_type` is set explicitly: GCS defaults to
        # application/octet-stream, which is right for the file blobs but
        # makes the manifest awkward to eyeball in the console.
        content_type = (
            "application/json" if key.endswith(".json") else "application/octet-stream"
        )
        self.bucket.blob(key).upload_from_string(body, content_type=content_type)

    def _get(self, key: str) -> bytes:
        return self.bucket.blob(key).download_as_bytes()

    def _prefix(self, key_prefix: str) -> str:
        return self._full_key(key_prefix).rstrip("/") + "/"

    # --------------------------------------------------------------- save
    def save_workspace(
        self,
        *,
        workspace_root: Path,
        signer: WorkspaceSigner,
        workspace_id: str,
        created_at: str,
        key_prefix: str,
    ) -> RemoteSnapshotSpec:
        """Dump + sign + upload a local workspace tree. Returns the
        `RemoteSnapshotSpec` the caller hands back to the GA SDK."""
        bundle = dump_workspace(
            workspace_root=workspace_root,
            signer=signer,
            workspace_id=workspace_id,
            created_at=created_at,
        )

        base = self._prefix(key_prefix)
        self._put(base + _MANIFEST_KEY, bundle["manifest"])
        self._put(base + _SIGNATURE_KEY, bundle["signature"])
        for rel_path, blob in bundle["files"].items():  # type: ignore[union-attr]
            self._put(base + _FILES_PREFIX + rel_path, blob)

        digest = hashlib.sha256(bundle["manifest"]).hexdigest()  # type: ignore[arg-type]
        return RemoteSnapshotSpec(
            backend=self.backend_name,
            bucket=self.bucket_name,
            key_prefix=self._full_key(key_prefix),
            manifest_sha256=digest,
        )

    # --------------------------------------------------------------- load
    def load_workspace(
        self,
        *,
        spec: RemoteSnapshotSpec,
        workspace_root: Path,
        verifying_key_raw: bytes,
    ) -> SnapshotManifest:
        """Pull the manifest + signature + every file, verify the whole
        chain, and materialise the workspace under ``workspace_root``."""
        if spec.backend != self.backend_name:
            raise ValueError(
                f"{type(self).__name__} can't load a {spec.backend!r} spec "
                f"(expected backend={self.backend_name!r})"
            )
        if spec.bucket != self.bucket_name:
            raise ValueError(
                f"spec references bucket {spec.bucket!r}, this client is on "
                f"{self.bucket_name!r}"
            )

        base = spec.key_prefix.rstrip("/") + "/"
        manifest_bytes = self._get(base + _MANIFEST_KEY)
        signature = self._get(base + _SIGNATURE_KEY)

        # Independent integrity check against the spec's digest, so a
        # post-save tamper is caught even if the signer's key rotated.
        if hashlib.sha256(manifest_bytes).hexdigest() != spec.manifest_sha256:
            raise ValueError(
                "manifest SHA-256 mismatch — spec.manifest_sha256 "
                "does not match what GCS served"
            )

        def _fetch(rel_path: str) -> bytes:
            return self._get(base + _FILES_PREFIX + rel_path)

        return load_workspace(
            workspace_root=workspace_root,
            manifest_bytes=manifest_bytes,
            signature=signature,
            verifying_key_raw=verifying_key_raw,
            fetch_file=_fetch,
        )

    # -------------------------------------------------------------- delete
    def delete_workspace(self, *, key_prefix: str) -> None:
        """Best-effort cleanup of every object under the prefix.

        ``list_blobs`` paginates transparently in the GCS client, so
        unlike the S3 backend there is no explicit paginator. Deletes are
        issued per blob and individually guarded: a snapshot that is
        already half-gone must not raise on teardown.
        """
        base = self._full_key(key_prefix).rstrip("/") + "/"
        for blob in self.client.list_blobs(self.bucket_name, prefix=base):
            try:
                blob.delete()
            except Exception:  # noqa: BLE001 — best effort on teardown
                pass
