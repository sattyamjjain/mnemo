"""Azure Blob Storage workspace backend for Mnemo snapshots (#39).

Opt-in dependency: ``pip install mnemo-db[openai-sandbox-azure]`` pulls
`azure-storage-blob`. The object layout is byte-identical to the S3 and
GCS backends, so a snapshot is portable between providers by copying
objects:

::

    <container>/<key_prefix>/manifest.json
    <container>/<key_prefix>/manifest.sig
    <container>/<key_prefix>/files/<rel_path>

Naming: container vs bucket
---------------------------

Azure calls the top-level namespace a **container**; S3 and GCS call it
a **bucket**. :class:`~mnemo.openai_sandbox.spec.RemoteSnapshotSpec` has
one field, ``bucket``, and this backend puts the container name there.
That keeps a spec a single shape across all four backends — the
alternative (a provider-specific field) would push a conditional into
every consumer of a spec. The constructor accepts ``container=`` as the
primary keyword and exposes ``.container_name``; ``spec.bucket`` is the
same string.

Why this is a standalone class and not an ``S3Workspace`` subclass
------------------------------------------------------------------

Azure Blob is not S3-wire-compatible in any form — different auth
(shared key / SAS / Entra ID), different REST surface, different
pagination. Per issue #39 ("single class per backend; no abstract base
layer") this uses the native ``azure-storage-blob`` client.

The signing contract is **unchanged** — `dump_workspace` /
`load_workspace` from :mod:`mnemo.openai_sandbox.manifest` do all the
Ed25519 and per-blob digest work. Only the storage adapter differs.

Install
-------

::

    pip install mnemo-db[openai-sandbox-azure]

Construct
---------

::

    from mnemo.openai_sandbox.azure_workspace import AzureBlobWorkspace

    # From a connection string (what Azurite and most CI setups use):
    ws = AzureBlobWorkspace.from_connection_string(
        "DefaultEndpointsProtocol=...", container="agent-snapshots"
    )

    # From an account URL + credential (Entra ID / managed identity):
    from azure.identity import DefaultAzureCredential
    ws = AzureBlobWorkspace(
        container="agent-snapshots",
        account_url="https://acct.blob.core.windows.net",
        credential=DefaultAzureCredential(),
    )
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
    from azure.storage.blob import (  # type: ignore[import-not-found]
        BlobServiceClient as _BlobServiceClient,
    )
except ImportError as _azure_exc:  # pragma: no cover — exercised by the extra
    raise ImportError(
        "AzureBlobWorkspace requires `azure-storage-blob`. Install with "
        "`pip install mnemo-db[openai-sandbox-azure]`."
    ) from _azure_exc


_MANIFEST_KEY = "manifest.json"
_SIGNATURE_KEY = "manifest.sig"
_FILES_PREFIX = "files/"


class AzureBlobWorkspace:
    """Azure Blob Storage workspace storage.

    Accepts an already-configured ``ContainerClient`` so tests can inject
    Azurite (the official emulator) or a fake. Production callers
    normally use :meth:`from_connection_string` or pass
    ``account_url`` + ``credential``.
    """

    backend_name: WorkspaceBackend = "azure"

    def __init__(
        self,
        container: str,
        container_client: Any | None = None,
        *,
        account_url: str | None = None,
        credential: Any | None = None,
        key_prefix_root: str = "",
    ) -> None:
        if not container:
            raise ValueError("AzureBlobWorkspace: container is required")
        self.container_name = container
        self.account_url = account_url
        if container_client is not None:
            self.container_client = container_client
        else:
            if not account_url:
                raise ValueError(
                    "AzureBlobWorkspace: pass container_client, or account_url "
                    "(+ credential), or use AzureBlobWorkspace.from_connection_string()"
                )
            service = _BlobServiceClient(
                account_url=account_url, credential=credential
            )
            self.container_client = service.get_container_client(container)
        self.key_prefix_root = key_prefix_root.rstrip("/")

    @classmethod
    def from_connection_string(
        cls,
        connection_string: str,
        *,
        container: str,
        key_prefix_root: str = "",
    ) -> "AzureBlobWorkspace":
        """Build from an Azure Storage connection string.

        This is the path Azurite and most CI setups use, and it is the
        one that carries its own credentials, so it needs no separate
        ``credential`` object.
        """
        service = _BlobServiceClient.from_connection_string(connection_string)
        return cls(
            container=container,
            container_client=service.get_container_client(container),
            key_prefix_root=key_prefix_root,
        )

    # ------------------------------------------------------------- helpers
    def _full_key(self, *parts: str) -> str:
        bits: Iterable[str] = filter(None, (self.key_prefix_root, *parts))
        return "/".join(bits)

    def _put(self, key: str, body: bytes) -> None:
        # `overwrite=True` matters: Azure's default is to REJECT a write to
        # an existing blob (unlike S3/GCS, which overwrite silently). Without
        # it, re-saving a workspace to the same key_prefix would raise
        # ResourceExistsError, and save_workspace is expected to be
        # idempotent across retries.
        self.container_client.upload_blob(name=key, data=body, overwrite=True)

    def _get(self, key: str) -> bytes:
        return self.container_client.download_blob(key).readall()

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
            bucket=self.container_name,
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
        if spec.bucket != self.container_name:
            raise ValueError(
                f"spec references container {spec.bucket!r}, this client is on "
                f"{self.container_name!r}"
            )

        base = spec.key_prefix.rstrip("/") + "/"
        manifest_bytes = self._get(base + _MANIFEST_KEY)
        signature = self._get(base + _SIGNATURE_KEY)

        # Independent integrity check against the spec's digest, so a
        # post-save tamper is caught even if the signer's key rotated.
        if hashlib.sha256(manifest_bytes).hexdigest() != spec.manifest_sha256:
            raise ValueError(
                "manifest SHA-256 mismatch — spec.manifest_sha256 "
                "does not match what Azure Blob served"
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
        """Best-effort cleanup of every blob under the prefix.

        ``list_blobs`` paginates transparently. Deletes are issued per
        blob and individually guarded so a snapshot that is already
        half-gone does not raise on teardown.
        """
        base = self._full_key(key_prefix).rstrip("/") + "/"
        for blob in self.container_client.list_blobs(name_starts_with=base):
            name = getattr(blob, "name", None) or blob["name"]
            try:
                self.container_client.delete_blob(name)
            except Exception:  # noqa: BLE001 — best effort on teardown
                pass
