"""U1 regression test — Python SDK version stays aligned with Cargo workspace.

The Cargo workspace `workspace.package.version` and
`python/pyproject.toml` `[project] version` are bumped together at every
release. `mnemo.__version__` MUST track them so users running
`pip install mnemo-db` get a SDK whose self-reported version matches the
underlying compiled core.

See [docs/compat/version-skew-matrix.md](../../docs/compat/version-skew-matrix.md)
for the canonical matrix.
"""

from __future__ import annotations

import pathlib
import re

import mnemo


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]


def _read_workspace_version() -> str:
    cargo_toml = (REPO_ROOT / "Cargo.toml").read_text()
    # Match the FIRST `version = "x.y.z"` after `[workspace.package]`.
    block_match = re.search(
        r"\[workspace\.package\][^\[]*?version\s*=\s*\"([^\"]+)\"",
        cargo_toml,
        re.DOTALL,
    )
    assert block_match, "could not parse [workspace.package] version from Cargo.toml"
    return block_match.group(1)


def _read_pyproject_version() -> str:
    pyproject = (REPO_ROOT / "python" / "pyproject.toml").read_text()
    block_match = re.search(
        r"\[project\][^\[]*?version\s*=\s*\"([^\"]+)\"",
        pyproject,
        re.DOTALL,
    )
    assert block_match, "could not parse [project] version from python/pyproject.toml"
    return block_match.group(1)


def test_python_sdk_version_matches_cargo_workspace() -> None:
    workspace_version = _read_workspace_version()
    assert mnemo.__version__ == workspace_version, (
        f"mnemo.__version__={mnemo.__version__!r} drifted from Cargo "
        f"workspace.package.version={workspace_version!r}. Update "
        f"python/mnemo/__init__.py to match."
    )


def test_python_sdk_version_matches_pyproject() -> None:
    pyproject_version = _read_pyproject_version()
    assert mnemo.__version__ == pyproject_version, (
        f"mnemo.__version__={mnemo.__version__!r} drifted from "
        f"python/pyproject.toml [project] version={pyproject_version!r}."
    )


def test_v0_4_8_pinned() -> None:
    """Sanity-check this exact release. Bump alongside CHANGELOG."""
    assert mnemo.__version__ == "0.4.8"
