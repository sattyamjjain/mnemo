"""Tests for the Azure Blob workspace backend (#39).

Two suites, mirroring ``test_r2_workspace.py`` / ``test_gcs_workspace.py``:

* **fake-client** — full save/load/delete cycle against an in-memory
  stand-in for ``ContainerClient``. Azurite is the official emulator but
  needs Docker/npm, so a fake is the practical unit-test substrate. The
  logic actually under test — manifest signing, per-blob digest
  verification, spec plumbing, and Azure's overwrite semantics — all run
  for real; only the ~4-method container surface is faked.
* **live Azure** — opt-in, runs only when ``AZURE_STORAGE_CONNECTION_STRING``
  and ``AZURE_CONTAINER`` are exported. Skipped by default. Works
  unmodified against Azurite, whose connection string is well-known.
"""

from __future__ import annotations

import importlib.util
import os
from pathlib import Path

import pytest

from mnemo.openai_sandbox.manifest import WorkspaceSigner
from mnemo.openai_sandbox.spec import RemoteSnapshotSpec

_HAS_AZURE = importlib.util.find_spec("azure.storage.blob") is not None

pytestmark = pytest.mark.skipif(
    not _HAS_AZURE,
    reason="azure-storage-blob not installed (pip install mnemo-db[openai-sandbox-azure])",
)


def _build_tree(root: Path) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / "a.txt").write_text("alpha", encoding="utf-8")
    sub = root / "nested" / "deep"
    sub.mkdir(parents=True)
    (sub / "b.bin").write_bytes(b"\x01\x02" * 50_000)


class _ResourceExists(Exception):
    """Stand-in for azure.core.exceptions.ResourceExistsError."""


class _FakeDownload:
    def __init__(self, data: bytes) -> None:
        self._data = data

    def readall(self) -> bytes:
        return self._data


class _FakeBlobProps:
    def __init__(self, name: str) -> None:
        self.name = name


class _FakeContainerClient:
    """Minimal in-memory stand-in for ``ContainerClient``.

    Reproduces the one Azure behaviour that differs materially from
    S3/GCS: ``upload_blob`` REJECTS an existing name unless
    ``overwrite=True``. Getting that wrong would make ``save_workspace``
    non-idempotent across retries, so the fake enforces it rather than
    quietly accepting every write.
    """

    def __init__(self) -> None:
        self.store: dict[str, bytes] = {}

    def upload_blob(self, name: str, data: bytes, overwrite: bool = False) -> None:
        if name in self.store and not overwrite:
            raise _ResourceExists(f"blob already exists: {name}")
        self.store[name] = bytes(data)

    def download_blob(self, name: str) -> _FakeDownload:
        try:
            return _FakeDownload(self.store[name])
        except KeyError as exc:
            raise FileNotFoundError(f"no such blob: {name}") from exc

    def list_blobs(self, name_starts_with: str = "") -> list[_FakeBlobProps]:
        return [
            _FakeBlobProps(k) for k in sorted(self.store) if k.startswith(name_starts_with)
        ]

    def delete_blob(self, name: str) -> None:
        self.store.pop(name, None)


# ------------------------------------------------------------------- tests
def test_azure_workspace_round_trip(tmp_path: Path) -> None:
    from mnemo.openai_sandbox.azure_workspace import AzureBlobWorkspace

    cc = _FakeContainerClient()
    src, dst = tmp_path / "src", tmp_path / "dst"
    _build_tree(src)
    signer = WorkspaceSigner.generate_ephemeral()

    ws = AzureBlobWorkspace(container="snap", container_client=cc)
    spec = ws.save_workspace(
        workspace_root=src,
        signer=signer,
        workspace_id="wid-az",
        created_at="2026-08-15T00:00:00Z",
        key_prefix="sessions/a1",
    )
    assert spec.backend == "azure"
    # The container name lands in spec.bucket — one spec shape across all
    # four backends, so consumers need no provider conditional.
    assert spec.bucket == "snap"
    assert spec.key_prefix == "sessions/a1"
    assert len(spec.manifest_sha256) == 64

    assert "sessions/a1/manifest.json" in cc.store
    assert "sessions/a1/manifest.sig" in cc.store
    assert any(k.startswith("sessions/a1/files/") for k in cc.store)

    ws.load_workspace(
        spec=spec, workspace_root=dst, verifying_key_raw=signer.verifying_key_raw()
    )
    assert (dst / "a.txt").read_text() == "alpha"
    assert (dst / "nested" / "deep" / "b.bin").read_bytes() == b"\x01\x02" * 50_000

    ws.delete_workspace(key_prefix="sessions/a1")
    assert not [k for k in cc.store if k.startswith("sessions/a1/")]


def test_azure_save_is_idempotent_across_retries(tmp_path: Path) -> None:
    """Re-saving to the same key_prefix must succeed.

    Azure's ``upload_blob`` defaults to rejecting an existing blob, so a
    backend that forgot ``overwrite=True`` would raise on the second
    save. That is a retry-path bug that would only surface in production,
    which is exactly why it is pinned here.
    """
    from mnemo.openai_sandbox.azure_workspace import AzureBlobWorkspace

    cc = _FakeContainerClient()
    src = tmp_path / "src"
    _build_tree(src)
    signer = WorkspaceSigner.generate_ephemeral()
    ws = AzureBlobWorkspace(container="snap", container_client=cc)

    kwargs = dict(
        workspace_root=src,
        signer=signer,
        workspace_id="w",
        created_at="2026-08-15T00:00:00Z",
        key_prefix="s/1",
    )
    first = ws.save_workspace(**kwargs)  # type: ignore[arg-type]
    second = ws.save_workspace(**kwargs)  # type: ignore[arg-type]
    assert first.manifest_sha256 == second.manifest_sha256


def test_azure_workspace_honours_key_prefix_root(tmp_path: Path) -> None:
    from mnemo.openai_sandbox.azure_workspace import AzureBlobWorkspace

    cc = _FakeContainerClient()
    src = tmp_path / "src"
    _build_tree(src)

    ws = AzureBlobWorkspace(container="c", container_client=cc, key_prefix_root="tenant-b/")
    spec = ws.save_workspace(
        workspace_root=src,
        signer=WorkspaceSigner.generate_ephemeral(),
        workspace_id="w",
        created_at="2026-08-15T00:00:00Z",
        key_prefix="s/1",
    )
    assert spec.key_prefix == "tenant-b/s/1"
    assert "tenant-b/s/1/manifest.json" in cc.store


def test_azure_workspace_rejects_foreign_backend_spec(tmp_path: Path) -> None:
    from mnemo.openai_sandbox.azure_workspace import AzureBlobWorkspace

    ws = AzureBlobWorkspace(container="c", container_client=_FakeContainerClient())
    bad = RemoteSnapshotSpec(
        backend="gcs", bucket="c", key_prefix="x", manifest_sha256="0" * 64
    )
    with pytest.raises(ValueError, match="backend='azure'"):
        ws.load_workspace(
            spec=bad, workspace_root=tmp_path, verifying_key_raw=b"\x00" * 32
        )


def test_azure_workspace_detects_tampered_manifest(tmp_path: Path) -> None:
    from mnemo.openai_sandbox.azure_workspace import AzureBlobWorkspace

    cc = _FakeContainerClient()
    src = tmp_path / "src"
    _build_tree(src)
    signer = WorkspaceSigner.generate_ephemeral()

    ws = AzureBlobWorkspace(container="c", container_client=cc)
    spec = ws.save_workspace(
        workspace_root=src,
        signer=signer,
        workspace_id="w",
        created_at="2026-08-15T00:00:00Z",
        key_prefix="s/1",
    )
    cc.store["s/1/manifest.json"] = b'{"tampered": true}'

    with pytest.raises(ValueError, match="SHA-256 mismatch"):
        ws.load_workspace(
            spec=spec,
            workspace_root=tmp_path / "dst",
            verifying_key_raw=signer.verifying_key_raw(),
        )


def test_azure_workspace_requires_container() -> None:
    from mnemo.openai_sandbox.azure_workspace import AzureBlobWorkspace

    with pytest.raises(ValueError, match="container is required"):
        AzureBlobWorkspace(container="", container_client=_FakeContainerClient())


def test_azure_workspace_requires_a_way_to_build_a_client() -> None:
    """No client and no account_url is a constructor error, not a
    late AttributeError at first upload."""
    from mnemo.openai_sandbox.azure_workspace import AzureBlobWorkspace

    with pytest.raises(ValueError, match="account_url"):
        AzureBlobWorkspace(container="c")


# ------------------------------------------------------- live Azure (opt-in)
_LIVE_AZURE = bool(
    os.environ.get("AZURE_STORAGE_CONNECTION_STRING") and os.environ.get("AZURE_CONTAINER")
)


@pytest.mark.skipif(
    not _LIVE_AZURE,
    reason="set AZURE_STORAGE_CONNECTION_STRING + AZURE_CONTAINER to run live Azure "
    "(works against Azurite too)",
)
def test_live_azure_round_trip(tmp_path: Path) -> None:  # pragma: no cover — live network
    """Pushes a tree against real Azure Blob (or Azurite). Skipped by default."""
    from mnemo.openai_sandbox.azure_workspace import AzureBlobWorkspace

    src, dst = tmp_path / "src", tmp_path / "dst"
    _build_tree(src)
    signer = WorkspaceSigner.generate_ephemeral()

    ws = AzureBlobWorkspace.from_connection_string(
        os.environ["AZURE_STORAGE_CONNECTION_STRING"],
        container=os.environ["AZURE_CONTAINER"],
    )
    spec = ws.save_workspace(
        workspace_root=src,
        signer=signer,
        workspace_id="wid-live-az",
        created_at="2026-08-15T00:00:00Z",
        key_prefix="mnemo-tests/live-azure",
    )
    try:
        ws.load_workspace(
            spec=spec, workspace_root=dst, verifying_key_raw=signer.verifying_key_raw()
        )
        assert (dst / "a.txt").read_text() == "alpha"
    finally:
        ws.delete_workspace(key_prefix="mnemo-tests/live-azure")
