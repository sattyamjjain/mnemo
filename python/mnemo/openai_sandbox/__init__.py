"""OpenAI Agents SDK sandbox-backed workspace backends for Mnemo.

Ships two sides of the v0.3.2 roadmap commitment:

* `LocalSnapshotSpec` / `RemoteSnapshotSpec` — the split the 2026-04-16
  GA release introduced. Pickle-friendly, so the Agents SDK can hand
  them back to us across a worker restart.
* `S3Workspace` — real `boto3`-backed implementation of the workspace
  put/get/delete contract. Opt-in; install with
  `pip install mnemo[openai-sandbox-s3]`.

A workspace payload is a directory tree. We walk it with
`pathlib.PurePosixPath`, record every file's SHA-256 digest, record every
symlink separately, and package the whole thing into a `manifest.json`
that carries an Ed25519 signature (via
`cryptography.hazmat.primitives.asymmetric.ed25519`). `restore()`
verifies the signature and every per-file digest before writing anything
to disk, so a tampered snapshot fails closed.
"""

from __future__ import annotations

from mnemo.openai_sandbox.manifest import (
    SnapshotManifest,
    WorkspaceSigner,
    dump_workspace,
    load_workspace,
)
from mnemo.openai_sandbox.spec import (
    LocalSnapshotSpec,
    RemoteSnapshotSpec,
    SnapshotSpec,
)

__all__ = [
    "LocalSnapshotSpec",
    "RemoteSnapshotSpec",
    "SnapshotSpec",
    "SnapshotManifest",
    "WorkspaceSigner",
    "dump_workspace",
    "load_workspace",
]

try:
    from mnemo.openai_sandbox.s3_workspace import S3Workspace

    __all__.append("S3Workspace")
except ImportError:
    pass
