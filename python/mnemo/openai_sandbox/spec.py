"""GA-shape snapshot specs.

The OpenAI Agents SDK 2026-04-16 GA release split the preview's single
opaque snapshot pointer into two explicit shapes:

* ``LocalSnapshotSpec`` — references a workspace tree on the local
  filesystem (fastest path; used for in-process crash-and-resume on a
  single machine).
* ``RemoteSnapshotSpec`` — references a workspace tree in object storage
  (``backend ∈ {"s3", "r2", "gcs", "azure"}``). Mnemo stores the
  ``bucket`` + ``key`` verbatim plus a SHA-256 manifest digest so we
  can replay the GA SDK's resume call sequence losslessly.

Both variants are frozen dataclasses so they pickle cleanly and can
round-trip through Mnemo's `remember` content body without surprises.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal, Union

WorkspaceBackend = Literal["s3", "r2", "gcs", "azure"]


@dataclass(frozen=True)
class LocalSnapshotSpec:
    """Workspace contents live on the local filesystem under ``root``."""

    root: str
    manifest_sha256: str

    def kind(self) -> Literal["local"]:
        return "local"


@dataclass(frozen=True)
class RemoteSnapshotSpec:
    """Workspace contents live in a bucket / container accessible through
    ``backend``."""

    backend: WorkspaceBackend
    bucket: str
    key_prefix: str
    manifest_sha256: str

    def kind(self) -> Literal["remote"]:
        return "remote"


SnapshotSpec = Union[LocalSnapshotSpec, RemoteSnapshotSpec]
