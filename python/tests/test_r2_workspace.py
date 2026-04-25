"""Tests for the Cloudflare R2 workspace backend (v0.3.4 Task A3).

Two suites:

* **moto-driven** — exercises the full save/load/delete cycle through
  the same in-memory S3 emulator we use for ``S3Workspace``. Validates
  that the R2 subclass correctly threads its credentials, addressing
  style, and ``backend="r2"`` spec output without requiring real R2.
* **live R2** — opt-in marker that runs only when
  ``R2_ACCOUNT_ID`` + ``R2_ACCESS_KEY_ID`` + ``R2_SECRET_ACCESS_KEY``
  are present in the environment. Skipped by default so CI doesn't
  burn against an unset live account.
"""

from __future__ import annotations

import importlib.util
import os
from pathlib import Path

import pytest

from mnemo.openai_sandbox.manifest import WorkspaceSigner
from mnemo.openai_sandbox.spec import RemoteSnapshotSpec


def _build_tree(root: Path) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / "a.txt").write_text("alpha", encoding="utf-8")
    sub = root / "nested" / "deep"
    sub.mkdir(parents=True)
    (sub / "b.bin").write_bytes(b"\x01\x02" * 50_000)


_HAS_MOTO = importlib.util.find_spec("moto") is not None


@pytest.mark.skipif(not _HAS_MOTO, reason="moto not installed")
def test_r2_workspace_round_trip_via_moto(tmp_path: Path) -> None:
    """The R2 subclass round-trips against an in-memory S3 emulator.

    moto's S3 mock implements the same wire protocol R2 exposes, which
    is the practical thing — we exercise the full code path including
    `backend="r2"` plumbing in :class:`RemoteSnapshotSpec` without
    requiring a Cloudflare account.
    """
    import boto3
    import moto  # type: ignore[import-not-found]

    from mnemo.openai_sandbox.r2_workspace import CloudflareR2Workspace

    with moto.mock_aws():
        # Seed a bucket via the AWS-region endpoint moto serves; the
        # subclass under test will hit moto's same in-memory S3 via the
        # injected client below.
        s3 = boto3.client("s3", region_name="us-east-1")
        s3.create_bucket(Bucket="mnemo-r2-test")

        src = tmp_path / "src"
        dst = tmp_path / "dst"
        _build_tree(src)
        signer = WorkspaceSigner.generate_ephemeral()

        ws = CloudflareR2Workspace(
            bucket="mnemo-r2-test",
            account_id="abc123",
            access_key_id="ignored-by-moto",
            secret_access_key="ignored-by-moto",
            client=s3,  # bypass _build_default_client so moto sees the calls
        )

        # Spec output must carry backend="r2" so downstream callers can
        # route to the right loader.
        spec = ws.save_workspace(
            workspace_root=src,
            signer=signer,
            workspace_id="wid-r2",
            created_at="2026-04-25T00:00:00Z",
            key_prefix="sessions/r1",
        )
        assert isinstance(spec, RemoteSnapshotSpec)
        assert spec.backend == "r2"
        assert spec.bucket == "mnemo-r2-test"
        assert spec.key_prefix == "sessions/r1"
        assert len(spec.manifest_sha256) == 64

        ws.load_workspace(
            spec=spec,
            workspace_root=dst,
            verifying_key_raw=signer.verifying_key_raw(),
        )
        assert (dst / "a.txt").read_text() == "alpha"
        assert (dst / "nested" / "deep" / "b.bin").read_bytes() == b"\x01\x02" * 50_000

        ws.delete_workspace(key_prefix="sessions/r1")
        remaining = s3.list_objects_v2(Bucket="mnemo-r2-test", Prefix="sessions/r1/")
        assert "Contents" not in remaining or not remaining["Contents"]


def test_r2_workspace_rejects_s3_spec(tmp_path: Path) -> None:
    """Loading an ``s3``-flavoured spec through the R2 subclass must
    error rather than silently doing the wrong thing."""
    import boto3
    import importlib.util as _impl

    if not _impl.find_spec("moto"):
        pytest.skip("moto not installed")
    import moto  # type: ignore[import-not-found]

    from mnemo.openai_sandbox.r2_workspace import CloudflareR2Workspace

    with moto.mock_aws():
        s3 = boto3.client("s3", region_name="us-east-1")
        s3.create_bucket(Bucket="mnemo-rejects-test")
        ws = CloudflareR2Workspace(
            bucket="mnemo-rejects-test",
            account_id="abc",
            access_key_id="k",
            secret_access_key="s",
            client=s3,
        )
        bad_spec = RemoteSnapshotSpec(
            backend="s3",
            bucket="mnemo-rejects-test",
            key_prefix="x",
            manifest_sha256="0" * 64,
        )
        with pytest.raises(ValueError, match="backend='r2'"):
            ws.load_workspace(
                spec=bad_spec,
                workspace_root=tmp_path,
                verifying_key_raw=b"\x00" * 32,
            )


def test_r2_workspace_requires_account_id() -> None:
    from mnemo.openai_sandbox.r2_workspace import CloudflareR2Workspace

    with pytest.raises(ValueError, match="account_id is required"):
        CloudflareR2Workspace(
            bucket="mnemo-rejects-test",
            account_id="",
            access_key_id="k",
            secret_access_key="s",
        )


# ---------------------------------------------------------- live R2 (opt-in)
_LIVE_R2_AVAILABLE = all(
    os.environ.get(k)
    for k in ("R2_ACCOUNT_ID", "R2_ACCESS_KEY_ID", "R2_SECRET_ACCESS_KEY", "R2_BUCKET")
)


@pytest.mark.skipif(
    not _LIVE_R2_AVAILABLE,
    reason="set R2_ACCOUNT_ID / R2_ACCESS_KEY_ID / R2_SECRET_ACCESS_KEY / R2_BUCKET to run live R2",
)
def test_live_r2_round_trip(tmp_path: Path) -> None:  # pragma: no cover — live network
    """Pushes a tree against a real Cloudflare R2 bucket. Skipped by
    default; opt in by exporting the four R2_* env vars above."""
    from mnemo.openai_sandbox.r2_workspace import CloudflareR2Workspace

    src = tmp_path / "src"
    dst = tmp_path / "dst"
    _build_tree(src)
    signer = WorkspaceSigner.generate_ephemeral()

    ws = CloudflareR2Workspace(
        bucket=os.environ["R2_BUCKET"],
        account_id=os.environ["R2_ACCOUNT_ID"],
        access_key_id=os.environ["R2_ACCESS_KEY_ID"],
        secret_access_key=os.environ["R2_SECRET_ACCESS_KEY"],
    )
    spec = ws.save_workspace(
        workspace_root=src,
        signer=signer,
        workspace_id="wid-live-r2",
        created_at="2026-04-25T00:00:00Z",
        key_prefix="mnemo-tests/live-r2",
    )
    try:
        ws.load_workspace(
            spec=spec,
            workspace_root=dst,
            verifying_key_raw=signer.verifying_key_raw(),
        )
        assert (dst / "a.txt").read_text() == "alpha"
    finally:
        ws.delete_workspace(key_prefix="mnemo-tests/live-r2")
