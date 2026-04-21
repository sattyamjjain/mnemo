"""Tests for the OpenAI Agents GA workspace backend — manifest,
signature chain, and S3 round-trip via moto."""

from __future__ import annotations

import importlib.util
import os
from pathlib import Path

import pytest

from mnemo.openai_sandbox.manifest import (
    SnapshotManifest,
    WorkspaceSigner,
    dump_workspace,
    load_workspace,
)
from mnemo.openai_sandbox.spec import LocalSnapshotSpec, RemoteSnapshotSpec


# ------------------------------------------------------------- pure-Python
def _build_tree(root: Path) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / "a.txt").write_text("alpha", encoding="utf-8")
    sub = root / "nested" / "deep"
    sub.mkdir(parents=True)
    (sub / "b.bin").write_bytes(b"\x01\x02" * 50_000)  # > 64 KiB
    # A relative symlink that must survive the round-trip.
    link_src = root / "nested" / "link-to-a.txt"
    os.symlink("../a.txt", link_src)


def test_snapshot_specs_are_pickle_friendly() -> None:
    import pickle

    local = LocalSnapshotSpec(root="/tmp/xx", manifest_sha256="deadbeef")
    remote = RemoteSnapshotSpec(
        backend="s3",
        bucket="mnemo-test",
        key_prefix="sessions/s1",
        manifest_sha256="deadbeef",
    )
    assert pickle.loads(pickle.dumps(local)) == local
    assert pickle.loads(pickle.dumps(remote)) == remote


def test_dump_and_load_round_trip_local(tmp_path: Path) -> None:
    src = tmp_path / "src"
    dst = tmp_path / "dst"
    _build_tree(src)

    signer = WorkspaceSigner.generate_ephemeral()
    bundle = dump_workspace(
        workspace_root=src,
        signer=signer,
        workspace_id="wid-test",
        created_at="2026-04-21T00:00:00Z",
    )
    assert b"wid-test" in bundle["manifest"]
    assert b"a.txt" in bundle["manifest"]
    assert b"link-to-a.txt" in bundle["manifest"]

    manifest = load_workspace(
        workspace_root=dst,
        manifest_bytes=bundle["manifest"],
        signature=bundle["signature"],
        verifying_key_raw=signer.verifying_key_raw(),
        fetch_file=lambda p: bundle["files"][p],
    )
    assert isinstance(manifest, SnapshotManifest)
    assert (dst / "a.txt").read_text() == "alpha"
    assert (dst / "nested" / "deep" / "b.bin").stat().st_size == 100_000
    link = dst / "nested" / "link-to-a.txt"
    assert link.is_symlink()
    assert os.readlink(link) == "../a.txt"


def test_tampered_manifest_rejects_signature(tmp_path: Path) -> None:
    from cryptography.exceptions import InvalidSignature

    src = tmp_path / "src"
    dst = tmp_path / "dst"
    _build_tree(src)
    signer = WorkspaceSigner.generate_ephemeral()
    bundle = dump_workspace(
        workspace_root=src,
        signer=signer,
        workspace_id="wid-test",
        created_at="2026-04-21T00:00:00Z",
    )
    # Flip a byte in the manifest.
    tampered = bytearray(bundle["manifest"])
    tampered[50] ^= 0x01
    with pytest.raises(InvalidSignature):
        load_workspace(
            workspace_root=dst,
            manifest_bytes=bytes(tampered),
            signature=bundle["signature"],
            verifying_key_raw=signer.verifying_key_raw(),
            fetch_file=lambda p: bundle["files"][p],
        )


def test_tampered_file_rejects_digest(tmp_path: Path) -> None:
    src = tmp_path / "src"
    dst = tmp_path / "dst"
    _build_tree(src)
    signer = WorkspaceSigner.generate_ephemeral()
    bundle = dump_workspace(
        workspace_root=src,
        signer=signer,
        workspace_id="wid-test",
        created_at="2026-04-21T00:00:00Z",
    )

    def _fetch(p: str) -> bytes:
        if p == "a.txt":
            return b"TAMPERED"
        return bundle["files"][p]

    with pytest.raises(ValueError, match="digest mismatch"):
        load_workspace(
            workspace_root=dst,
            manifest_bytes=bundle["manifest"],
            signature=bundle["signature"],
            verifying_key_raw=signer.verifying_key_raw(),
            fetch_file=_fetch,
        )


# --------------------------------------------------------------- moto / S3
_HAS_MOTO = importlib.util.find_spec("moto") is not None


@pytest.mark.skipif(not _HAS_MOTO, reason="moto not installed")
def test_s3_workspace_round_trip(tmp_path: Path) -> None:
    import boto3
    import moto  # type: ignore[import-not-found]

    from mnemo.openai_sandbox.s3_workspace import S3Workspace

    with moto.mock_aws():
        s3 = boto3.client("s3", region_name="us-east-1")
        s3.create_bucket(Bucket="mnemo-test")

        src = tmp_path / "src"
        dst = tmp_path / "dst"
        _build_tree(src)
        signer = WorkspaceSigner.generate_ephemeral()

        ws = S3Workspace(bucket="mnemo-test", client=s3)
        spec = ws.save_workspace(
            workspace_root=src,
            signer=signer,
            workspace_id="wid-s3",
            created_at="2026-04-21T00:00:00Z",
            key_prefix="sessions/s1",
        )
        assert isinstance(spec, RemoteSnapshotSpec)
        assert spec.backend == "s3"
        assert spec.bucket == "mnemo-test"
        assert spec.key_prefix == "sessions/s1"
        assert len(spec.manifest_sha256) == 64

        ws.load_workspace(
            spec=spec,
            workspace_root=dst,
            verifying_key_raw=signer.verifying_key_raw(),
        )
        assert (dst / "a.txt").read_text() == "alpha"
        link = dst / "nested" / "link-to-a.txt"
        assert link.is_symlink()
        assert os.readlink(link) == "../a.txt"

        ws.delete_workspace(key_prefix="sessions/s1")
        remaining = s3.list_objects_v2(Bucket="mnemo-test", Prefix="sessions/s1/")
        assert "Contents" not in remaining or not remaining["Contents"]
