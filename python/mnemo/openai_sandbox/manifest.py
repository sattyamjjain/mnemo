"""Signed snapshot manifest for workspace trees.

A workspace tree is serialised into a `manifest.json` plus one file
per content blob (so the manifest stays small even for large payloads).
Every file carries a SHA-256 digest; every symlink is captured as a
``{source, target}`` record separate from the file list. The manifest
itself is signed with Ed25519, and `load_workspace` verifies every
per-file digest before writing anything to disk — a tampered snapshot
fails closed.
"""

from __future__ import annotations

import dataclasses
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
from typing import Any, Literal

from cryptography.hazmat.primitives.asymmetric import ed25519

_MANIFEST_NAME = "manifest.json"
_SIGNATURE_NAME = "manifest.sig"
_CONTENT_DIR = "files"


def _sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(64 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


@dataclasses.dataclass(frozen=True)
class FileEntry:
    path: str  # POSIX-normalised relative path under the workspace root
    sha256: str
    size: int
    mode: int


@dataclasses.dataclass(frozen=True)
class SymlinkEntry:
    source: str
    target: str


@dataclasses.dataclass
class SnapshotManifest:
    """A structured view of the manifest.json file."""

    workspace_id: str
    created_at: str
    files: list[FileEntry]
    symlinks: list[SymlinkEntry]

    def to_json(self) -> str:
        return json.dumps(
            {
                "version": 1,
                "workspace_id": self.workspace_id,
                "created_at": self.created_at,
                "files": [dataclasses.asdict(f) for f in self.files],
                "symlinks": [dataclasses.asdict(s) for s in self.symlinks],
            },
            sort_keys=True,
        )

    @staticmethod
    def from_json(raw: bytes | str) -> "SnapshotManifest":
        data: dict[str, Any] = (
            json.loads(raw) if isinstance(raw, str) else json.loads(raw.decode("utf-8"))
        )
        if data.get("version") != 1:
            raise ValueError(
                f"unsupported manifest version {data.get('version')!r}"
            )
        files = [FileEntry(**f) for f in data.get("files", [])]
        symlinks = [SymlinkEntry(**s) for s in data.get("symlinks", [])]
        return SnapshotManifest(
            workspace_id=str(data["workspace_id"]),
            created_at=str(data["created_at"]),
            files=files,
            symlinks=symlinks,
        )


class WorkspaceSigner:
    """Thin wrapper around Ed25519 sign/verify.

    Operators are expected to keep the 32-byte secret behind an HSM /
    KMS. `generate_ephemeral` exists for tests.
    """

    def __init__(self, private_key: ed25519.Ed25519PrivateKey) -> None:
        self._sk = private_key

    @staticmethod
    def from_secret_bytes(raw: bytes) -> "WorkspaceSigner":
        if len(raw) != 32:
            raise ValueError("Ed25519 secret must be exactly 32 bytes")
        return WorkspaceSigner(ed25519.Ed25519PrivateKey.from_private_bytes(raw))

    @staticmethod
    def generate_ephemeral() -> "WorkspaceSigner":
        return WorkspaceSigner(ed25519.Ed25519PrivateKey.generate())

    def sign(self, message: bytes) -> bytes:
        return self._sk.sign(message)

    def verifying_key_raw(self) -> bytes:
        from cryptography.hazmat.primitives import serialization

        return self._sk.public_key().public_bytes(
            encoding=serialization.Encoding.Raw,
            format=serialization.PublicFormat.Raw,
        )


def _verify_signature(
    manifest_bytes: bytes,
    signature: bytes,
    verifying_key_raw: bytes,
) -> None:
    key = ed25519.Ed25519PublicKey.from_public_bytes(verifying_key_raw)
    key.verify(signature, manifest_bytes)  # raises InvalidSignature


def dump_workspace(
    *,
    workspace_root: Path,
    signer: WorkspaceSigner,
    workspace_id: str,
    created_at: str,
) -> dict[Literal["manifest", "signature", "files"], Any]:
    """Walk ``workspace_root``, hash every file, record every symlink,
    build a signed manifest, and return the serialisable pieces.

    The returned dict carries:
      * ``"manifest"`` — the manifest bytes (exact bytes the signer signed).
      * ``"signature"`` — the Ed25519 signature of the manifest bytes.
      * ``"files"`` — ``{rel_path: bytes}`` keyed content blobs. Callers
        upload these to the workspace backend under the
        ``{key_prefix}/files/{rel_path}`` convention; the manifest and
        signature go under ``{key_prefix}/manifest.json`` and
        ``{key_prefix}/manifest.sig``.
    """
    if not workspace_root.exists():
        raise FileNotFoundError(f"workspace_root does not exist: {workspace_root}")

    files: list[FileEntry] = []
    symlinks: list[SymlinkEntry] = []
    content: dict[str, bytes] = {}

    for dirpath, _dirs, filenames in os.walk(workspace_root, followlinks=False):
        dir_rel = (
            PurePosixPath(Path(dirpath).relative_to(workspace_root).as_posix())
            if Path(dirpath) != workspace_root
            else PurePosixPath("")
        )
        for name in filenames:
            full = Path(dirpath, name)
            rel = str(dir_rel / name) if str(dir_rel) else name
            if full.is_symlink():
                symlinks.append(SymlinkEntry(source=rel, target=os.readlink(full)))
                continue
            if not full.is_file():
                continue
            blob = full.read_bytes()
            files.append(
                FileEntry(
                    path=rel,
                    sha256=_sha256_hex(blob),
                    size=len(blob),
                    mode=full.stat().st_mode & 0o7777,
                )
            )
            content[rel] = blob

    files.sort(key=lambda f: f.path)
    symlinks.sort(key=lambda s: s.source)

    manifest = SnapshotManifest(
        workspace_id=workspace_id,
        created_at=created_at,
        files=files,
        symlinks=symlinks,
    )
    manifest_bytes = manifest.to_json().encode("utf-8")
    signature = signer.sign(manifest_bytes)
    return {"manifest": manifest_bytes, "signature": signature, "files": content}


def load_workspace(
    *,
    workspace_root: Path,
    manifest_bytes: bytes,
    signature: bytes,
    verifying_key_raw: bytes,
    fetch_file: "callable[[str], bytes]",  # type: ignore[valid-type]
) -> SnapshotManifest:
    """Verify the manifest signature, then verify every file's digest
    against the manifest, then rebuild the workspace tree under
    ``workspace_root``.

    ``fetch_file(rel_path) -> bytes`` is the callback the caller
    supplies to pull a content blob from whichever backend is hosting
    the snapshot. Symlinks are recreated verbatim (relative targets
    preserved).

    Raises on any tamper: the Ed25519 verifier raises
    ``cryptography.exceptions.InvalidSignature`` on manifest tamper;
    file-digest mismatch raises ``ValueError``.
    """
    _verify_signature(manifest_bytes, signature, verifying_key_raw)
    manifest = SnapshotManifest.from_json(manifest_bytes)

    workspace_root.mkdir(parents=True, exist_ok=True)
    for f in manifest.files:
        blob = fetch_file(f.path)
        observed = _sha256_hex(blob)
        if observed != f.sha256:
            raise ValueError(
                f"workspace file digest mismatch for {f.path!r}: "
                f"manifest={f.sha256} observed={observed}"
            )
        target = workspace_root / f.path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(blob)
        try:
            os.chmod(target, f.mode)
        except PermissionError:  # pragma: no cover
            pass

    for s in manifest.symlinks:
        link = workspace_root / s.source
        link.parent.mkdir(parents=True, exist_ok=True)
        if link.exists() or link.is_symlink():
            link.unlink()
        os.symlink(s.target, link)

    return manifest
