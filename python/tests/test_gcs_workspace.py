"""Tests for the GCS workspace backend (#39).

Two suites, mirroring the shape ``test_r2_workspace.py`` established:

* **fake-client** — exercises the full save/load/delete cycle against an
  in-memory stand-in for ``google.cloud.storage.Client``. There is no
  moto equivalent for GCS, and the official emulator
  (``fake-gcs-server``) is a Docker image, so a fake is the practical
  unit-test substrate. It is a genuine test rather than a tautology: the
  interesting logic under test is the manifest signing, the per-blob
  digest verification and the spec plumbing — all of which run for real
  here. Only the ~5-method object-store surface is faked.
* **live GCS** — opt-in, runs only when ``GCS_BUCKET`` is exported (plus
  whatever Application Default Credentials the environment resolves).
  Skipped by default so CI never burns against a live project.
"""

from __future__ import annotations

import importlib.util
import os
from pathlib import Path

import pytest

from mnemo.openai_sandbox.manifest import WorkspaceSigner
from mnemo.openai_sandbox.spec import RemoteSnapshotSpec

_HAS_GCS = importlib.util.find_spec("google.cloud.storage") is not None

pytestmark = pytest.mark.skipif(
    not _HAS_GCS,
    reason="google-cloud-storage not installed (pip install mnemo-db[openai-sandbox-gcs])",
)


def _build_tree(root: Path) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / "a.txt").write_text("alpha", encoding="utf-8")
    sub = root / "nested" / "deep"
    sub.mkdir(parents=True)
    (sub / "b.bin").write_bytes(b"\x01\x02" * 50_000)


# ---------------------------------------------------------------- fake GCS
class _FakeBlob:
    def __init__(self, store: dict[str, bytes], name: str) -> None:
        self._store = store
        self.name = name

    def upload_from_string(self, data: bytes, content_type: str | None = None) -> None:
        self._store[self.name] = bytes(data)

    def download_as_bytes(self) -> bytes:
        try:
            return self._store[self.name]
        except KeyError as exc:  # mirrors google.cloud.exceptions.NotFound
            raise FileNotFoundError(f"no such blob: {self.name}") from exc

    def delete(self) -> None:
        self._store.pop(self.name, None)


class _FakeBucket:
    def __init__(self, store: dict[str, bytes]) -> None:
        self._store = store

    def blob(self, name: str) -> _FakeBlob:
        return _FakeBlob(self._store, name)


class _FakeGCSClient:
    """Minimal in-memory stand-in for ``google.cloud.storage.Client``.

    Implements exactly the surface :class:`GCSWorkspace` uses:
    ``bucket()`` and ``list_blobs()``. Anything else is deliberately
    absent so a future change that reaches for a new client method fails
    loudly here instead of silently passing against a permissive mock.
    """

    def __init__(self) -> None:
        self.store: dict[str, bytes] = {}

    def bucket(self, name: str) -> _FakeBucket:
        return _FakeBucket(self.store)

    def list_blobs(self, bucket_or_name: str, prefix: str = "") -> list[_FakeBlob]:
        return [
            _FakeBlob(self.store, k)
            for k in sorted(self.store)
            if k.startswith(prefix)
        ]


# ------------------------------------------------------------------- tests
def test_gcs_workspace_round_trip(tmp_path: Path) -> None:
    """Full save -> load -> verify -> delete cycle."""
    from mnemo.openai_sandbox.gcs_workspace import GCSWorkspace

    client = _FakeGCSClient()
    src, dst = tmp_path / "src", tmp_path / "dst"
    _build_tree(src)
    signer = WorkspaceSigner.generate_ephemeral()

    ws = GCSWorkspace(bucket="mnemo-gcs-test", client=client)
    spec = ws.save_workspace(
        workspace_root=src,
        signer=signer,
        workspace_id="wid-gcs",
        created_at="2026-08-15T00:00:00Z",
        key_prefix="sessions/g1",
    )
    assert isinstance(spec, RemoteSnapshotSpec)
    assert spec.backend == "gcs"
    assert spec.bucket == "mnemo-gcs-test"
    assert spec.key_prefix == "sessions/g1"
    assert len(spec.manifest_sha256) == 64

    # Object layout must match the S3/Azure backends byte-for-byte so a
    # snapshot is portable between providers by copying objects.
    assert "sessions/g1/manifest.json" in client.store
    assert "sessions/g1/manifest.sig" in client.store
    assert any(k.startswith("sessions/g1/files/") for k in client.store)

    ws.load_workspace(
        spec=spec, workspace_root=dst, verifying_key_raw=signer.verifying_key_raw()
    )
    assert (dst / "a.txt").read_text() == "alpha"
    assert (dst / "nested" / "deep" / "b.bin").read_bytes() == b"\x01\x02" * 50_000

    ws.delete_workspace(key_prefix="sessions/g1")
    assert not [k for k in client.store if k.startswith("sessions/g1/")]


def test_gcs_workspace_honours_key_prefix_root(tmp_path: Path) -> None:
    """``key_prefix_root`` must prefix stored keys AND the returned spec."""
    from mnemo.openai_sandbox.gcs_workspace import GCSWorkspace

    client = _FakeGCSClient()
    src = tmp_path / "src"
    _build_tree(src)

    ws = GCSWorkspace(bucket="b", client=client, key_prefix_root="tenant-a/")
    spec = ws.save_workspace(
        workspace_root=src,
        signer=WorkspaceSigner.generate_ephemeral(),
        workspace_id="w",
        created_at="2026-08-15T00:00:00Z",
        key_prefix="s/1",
    )
    assert spec.key_prefix == "tenant-a/s/1"
    assert "tenant-a/s/1/manifest.json" in client.store


def test_gcs_workspace_rejects_foreign_backend_spec(tmp_path: Path) -> None:
    """Loading an s3-flavoured spec through GCS must error, not silently
    do the wrong thing."""
    from mnemo.openai_sandbox.gcs_workspace import GCSWorkspace

    ws = GCSWorkspace(bucket="b", client=_FakeGCSClient())
    bad = RemoteSnapshotSpec(
        backend="s3", bucket="b", key_prefix="x", manifest_sha256="0" * 64
    )
    with pytest.raises(ValueError, match="backend='gcs'"):
        ws.load_workspace(
            spec=bad, workspace_root=tmp_path, verifying_key_raw=b"\x00" * 32
        )


def test_gcs_workspace_rejects_foreign_bucket(tmp_path: Path) -> None:
    from mnemo.openai_sandbox.gcs_workspace import GCSWorkspace

    ws = GCSWorkspace(bucket="mine", client=_FakeGCSClient())
    bad = RemoteSnapshotSpec(
        backend="gcs", bucket="theirs", key_prefix="x", manifest_sha256="0" * 64
    )
    with pytest.raises(ValueError, match="theirs"):
        ws.load_workspace(
            spec=bad, workspace_root=tmp_path, verifying_key_raw=b"\x00" * 32
        )


def test_gcs_workspace_detects_tampered_manifest(tmp_path: Path) -> None:
    """A manifest mutated after save must fail the spec-digest check.

    This is the property that makes the backend trustworthy: the digest
    is recorded at save time and re-checked on load, so tampering in the
    bucket is caught even if the attacker also re-signs.
    """
    from mnemo.openai_sandbox.gcs_workspace import GCSWorkspace

    client = _FakeGCSClient()
    src = tmp_path / "src"
    _build_tree(src)
    signer = WorkspaceSigner.generate_ephemeral()

    ws = GCSWorkspace(bucket="b", client=client)
    spec = ws.save_workspace(
        workspace_root=src,
        signer=signer,
        workspace_id="w",
        created_at="2026-08-15T00:00:00Z",
        key_prefix="s/1",
    )
    client.store["s/1/manifest.json"] = b'{"tampered": true}'

    with pytest.raises(ValueError, match="SHA-256 mismatch"):
        ws.load_workspace(
            spec=spec,
            workspace_root=tmp_path / "dst",
            verifying_key_raw=signer.verifying_key_raw(),
        )


def test_gcs_workspace_requires_bucket() -> None:
    from mnemo.openai_sandbox.gcs_workspace import GCSWorkspace

    with pytest.raises(ValueError, match="bucket is required"):
        GCSWorkspace(bucket="", client=_FakeGCSClient())


# --------------------------------------------------------- live GCS (opt-in)
@pytest.mark.skipif(
    not os.environ.get("GCS_BUCKET"),
    reason="set GCS_BUCKET (+ Application Default Credentials) to run live GCS",
)
def test_live_gcs_round_trip(tmp_path: Path) -> None:  # pragma: no cover — live network
    """Pushes a tree against a real GCS bucket. Skipped by default."""
    from mnemo.openai_sandbox.gcs_workspace import GCSWorkspace

    src, dst = tmp_path / "src", tmp_path / "dst"
    _build_tree(src)
    signer = WorkspaceSigner.generate_ephemeral()

    ws = GCSWorkspace(bucket=os.environ["GCS_BUCKET"])
    spec = ws.save_workspace(
        workspace_root=src,
        signer=signer,
        workspace_id="wid-live-gcs",
        created_at="2026-08-15T00:00:00Z",
        key_prefix="mnemo-tests/live-gcs",
    )
    try:
        ws.load_workspace(
            spec=spec, workspace_root=dst, verifying_key_raw=signer.verifying_key_raw()
        )
        assert (dst / "a.txt").read_text() == "alpha"
    finally:
        ws.delete_workspace(key_prefix="mnemo-tests/live-gcs")
