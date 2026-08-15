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
    """Read `[project].version` from python/pyproject.toml.

    Line-based rather than a `\\[project\\][^\\[]*?version` regex: that pattern
    stops at the first `[` ANYWHERE after the header, including one inside a
    comment. A comment mentioning `[workspace.package]` was enough to make it
    silently fail to find the version, which is a parser that fails closed on
    prose — not a property you want in a version fence.
    """
    pyproject = (REPO_ROOT / "python" / "pyproject.toml").read_text()
    in_project = False
    for line in pyproject.splitlines():
        stripped = line.strip()
        if stripped.startswith("#"):
            continue
        # A table header is a `[` at the START of a line, not any `[`.
        if stripped.startswith("["):
            in_project = stripped == "[project]"
            continue
        if in_project:
            m = re.match(r"version\s*=\s*\"([^\"]+)\"", stripped)
            if m:
                return m.group(1)
    raise AssertionError(
        "could not parse [project] version from python/pyproject.toml"
    )


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


def test_python_crate_inherits_the_workspace_version() -> None:
    """`python/Cargo.toml` must inherit, never pin a literal.

    This replaces a test that hard-coded the then-current release
    (`assert mnemo.__version__ == "0.5.12"`). A literal like that has to be
    bumped by hand every release, so it goes red the moment someone forgets
    — which is exactly what happened: it sat failing at 0.5.12 while the
    workspace moved to 0.5.23, and because no CI job runs pytest, nobody saw
    it.

    The invariant worth pinning is structural, not a number. `mnemo-python`
    compiles `mnemo-core` INTO the wheel, so if its `[package] version` ever
    became a literal, the wheel metadata and the engine inside it could
    silently diverge — the precise defect this file exists to catch. Asserting
    inheritance cannot rot with a release.
    """
    cargo = (REPO_ROOT / "python" / "Cargo.toml").read_text()
    pkg = re.search(r"\[package\](.*?)(?=\n\[)", cargo, re.DOTALL)
    assert pkg, "could not parse [package] from python/Cargo.toml"
    body = pkg.group(1)
    assert re.search(r"^\s*version\s*\.\s*workspace\s*=\s*true", body, re.M), (
        "python/Cargo.toml [package] must use `version.workspace = true`. A "
        "literal version there lets the wheel metadata and the mnemo-core "
        "compiled into it drift apart."
    )
    assert not re.search(r'^\s*version\s*=\s*"', body, re.M), (
        "python/Cargo.toml [package] pins a literal version; it must inherit "
        "from [workspace.package] instead."
    )
