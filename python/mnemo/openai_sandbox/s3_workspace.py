"""`boto3`-backed S3 workspace backend for Mnemo snapshots.

Opt-in dependency: ``pip install mnemo[openai-sandbox-s3]`` pulls
`boto3`. The object-storage layout mirrors what R2 / GCS / Azure will
later reuse so the shape generalises:

::

    s3://<bucket>/<key_prefix>/manifest.json
    s3://<bucket>/<key_prefix>/manifest.sig
    s3://<bucket>/<key_prefix>/files/<rel_path>

`save_workspace` uploads one file per content blob; `load_workspace`
pulls the manifest first, verifies the Ed25519 signature, then streams
each file back with per-blob digest verification.
"""

from __future__ import annotations

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
    import boto3  # type: ignore[import-not-found]
except ImportError as _boto_exc:  # pragma: no cover
    raise ImportError(
        "S3Workspace requires `boto3`. Install with "
        "`pip install mnemo[openai-sandbox-s3]`."
    ) from _boto_exc


_MANIFEST_KEY = "manifest.json"
_SIGNATURE_KEY = "manifest.sig"
_FILES_PREFIX = "files/"


class S3Workspace:
    """Real S3-backed workspace storage.

    Accepts an already-configured ``boto3`` client so tests can inject
    moto's in-memory S3. Production callers typically construct one
    via ``boto3.client("s3", region_name=...)``.

    The optional ``endpoint_url`` / ``region`` / ``addressing_style``
    kwargs let subclasses target S3-compatible providers (Cloudflare R2,
    Backblaze B2, MinIO, etc.) without re-implementing this class.
    Setting any of them to ``None`` keeps default-AWS behaviour exactly,
    which means existing call-sites that did not pass these kwargs
    behave identically to v0.3.3.
    """

    backend_name: WorkspaceBackend = "s3"
    """Subclasses override to set the canonical
    :class:`~mnemo.openai_sandbox.spec.RemoteSnapshotSpec` ``backend``
    string (e.g. ``"r2"``)."""

    def __init__(
        self,
        bucket: str,
        client: Any | None = None,
        *,
        key_prefix_root: str = "",
        endpoint_url: str | None = None,
        region: str | None = None,
        addressing_style: str | None = None,
        signature_version: str = "s3v4",
    ) -> None:
        self.bucket = bucket
        self.endpoint_url = endpoint_url
        self.region = region
        self.addressing_style = addressing_style
        self.signature_version = signature_version
        if client is not None:
            self.client = client
        else:
            self.client = self._build_default_client()
        self.key_prefix_root = key_prefix_root.rstrip("/")

    def _build_default_client(self) -> Any:
        """Construct a boto3 client honouring endpoint / region / addressing.

        Kept on the class so subclasses that don't take a pre-built
        client (the R2 happy path) inherit the right configuration
        without copying boto3 wiring.
        """
        kwargs: dict[str, Any] = {}
        if self.endpoint_url:
            kwargs["endpoint_url"] = self.endpoint_url
        if self.region:
            kwargs["region_name"] = self.region
        config_kwargs: dict[str, Any] = {}
        if self.addressing_style:
            config_kwargs["s3"] = {"addressing_style": self.addressing_style}
        if self.signature_version:
            config_kwargs["signature_version"] = self.signature_version
        if config_kwargs:
            from botocore.config import Config  # type: ignore[import-not-found]

            kwargs["config"] = Config(**config_kwargs)
        return boto3.client("s3", **kwargs)

    # ------------------------------------------------------------- helpers
    def _full_key(self, *parts: str) -> str:
        bits: Iterable[str] = filter(None, (self.key_prefix_root, *parts))
        return "/".join(bits)

    def _put(self, key: str, body: bytes) -> None:
        self.client.put_object(Bucket=self.bucket, Key=key, Body=body)

    def _get(self, key: str) -> bytes:
        resp = self.client.get_object(Bucket=self.bucket, Key=key)
        return resp["Body"].read()

    def _delete(self, key: str) -> None:
        try:
            self.client.delete_object(Bucket=self.bucket, Key=key)
        except Exception:  # noqa: BLE001 — best effort on teardown
            pass

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
        `RemoteSnapshotSpec` the caller should hand back to the GA SDK.
        """
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

        import hashlib as _hash

        digest = _hash.sha256(bundle["manifest"]).hexdigest()  # type: ignore[arg-type]
        return RemoteSnapshotSpec(
            backend=self.backend_name,
            bucket=self.bucket,
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
        if spec.bucket != self.bucket:
            raise ValueError(
                f"spec references bucket {spec.bucket!r}, this client is on {self.bucket!r}"
            )

        base = spec.key_prefix.rstrip("/") + "/"
        manifest_bytes = self._get(base + _MANIFEST_KEY)
        signature = self._get(base + _SIGNATURE_KEY)

        # Independent integrity check against the spec's digest so callers
        # can detect post-save tamper even if the signer's key rotated.
        import hashlib as _hash

        if _hash.sha256(manifest_bytes).hexdigest() != spec.manifest_sha256:
            raise ValueError(
                "manifest SHA-256 mismatch — spec.manifest_sha256 "
                "does not match what S3 served"
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
        """Best-effort cleanup. Lists keys under the prefix and deletes
        in batches. Leaves the bucket itself alone.
        """
        base = self._full_key(key_prefix).rstrip("/") + "/"
        paginator = self.client.get_paginator("list_objects_v2")
        batch: list[dict[str, str]] = []
        for page in paginator.paginate(Bucket=self.bucket, Prefix=base):
            for obj in page.get("Contents") or ():
                batch.append({"Key": obj["Key"]})
                if len(batch) >= 1000:
                    self.client.delete_objects(
                        Bucket=self.bucket, Delete={"Objects": batch}
                    )
                    batch = []
        if batch:
            self.client.delete_objects(
                Bucket=self.bucket, Delete={"Objects": batch}
            )
