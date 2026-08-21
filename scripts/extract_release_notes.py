#!/usr/bin/env python3
"""Print the CHANGELOG section for one released version.

WHY THIS EXISTS

Tags v0.5.23, v0.5.24 and v0.5.25 shipped without a GitHub Release object. The
newest Release was v0.5.22 for eleven days while three versions went out, so
anyone watching the repo's releases feed saw nothing.

The cause was not a broken automation. There was never any: no workflow in this
repo has ever created a Release. Every Release object up to v0.5.22 was made by
hand, and the hand stopped. Backfilling the three missing objects without fixing
that is how it comes back on the next tag.

So `release-crate.yml` now creates the Release itself, and this script is the
part worth testing separately: pulling the right section out of CHANGELOG.md,
and failing loudly rather than publishing an empty release body when the section
is missing.

Usage:
    extract_release_notes.py 0.5.25          # print the section body
    extract_release_notes.py --self-test     # verify the parser
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CHANGELOG = REPO_ROOT / "CHANGELOG.md"

# `## [0.5.25] - 2026-08-18` and also `## [Unreleased]`
HEADING = re.compile(r"^## \[([^\]]+)\]")


def sections(text: str) -> dict[str, str]:
    """Map every `## [X]` heading to its body, in file order."""
    lines = text.split("\n")
    heads = [(i, m.group(1)) for i, l in enumerate(lines) if (m := HEADING.match(l))]
    out: dict[str, str] = {}
    for idx, (line_no, name) in enumerate(heads):
        end = heads[idx + 1][0] if idx + 1 < len(heads) else len(lines)
        out[name] = "\n".join(lines[line_no + 1 : end]).strip()
    return out


def notes_for(version: str, text: str) -> str:
    """Body of the `## [version]` section, or raise with a usable message."""
    found = sections(text)
    if version not in found:
        available = ", ".join(k for k in found if k != "Unreleased") or "(none)"
        raise SystemExit(
            f"CHANGELOG.md has no `## [{version}]` section, so a release for that "
            f"version would ship an empty body.\n"
            f"Promote the `[Unreleased]` entries into `## [{version}] - <date>` "
            f"before tagging.\nSections present: {available}"
        )
    body = found[version]
    if not body:
        raise SystemExit(
            f"`## [{version}]` exists in CHANGELOG.md but its body is empty. A "
            f"release with no notes is worse than no release: it looks answered."
        )
    return body


def self_test() -> int:
    sample = "\n".join(
        [
            "# Changelog",
            "",
            "## [Unreleased]",
            "",
            "### Landing trace",
            "pending work",
            "",
            "## [0.9.1] - 2026-01-02",
            "",
            "### Fixed",
            "the thing",
            "",
            "## [0.9.0] - 2026-01-01",
            "",
            "first",
            "",
        ]
    )
    empty = "# Changelog\n\n## [0.9.1] - 2026-01-02\n\n## [0.9.0] - 2026-01-01\n\nfirst\n"
    passed = failed = 0

    def check(label: str, cond: bool) -> None:
        nonlocal passed, failed
        if cond:
            print(f"  ok   {label}")
            passed += 1
        else:
            print(f"  FAIL {label}")
            failed += 1

    body = notes_for("0.9.1", sample)
    check("extracts the named section", body == "### Fixed\nthe thing")
    check("stops at the next heading", "first" not in body)
    check("does not leak Unreleased", "pending work" not in body)
    check("extracts the oldest section", notes_for("0.9.0", sample) == "first")

    try:
        notes_for("0.9.2", sample)
        check("missing version raises", False)
    except SystemExit as e:
        check("missing version raises", "no `## [0.9.2]` section" in str(e))

    try:
        notes_for("0.9.1", empty)
        check("empty body raises", False)
    except SystemExit as e:
        check("empty body raises", "body is empty" in str(e))

    # Real CHANGELOG: the three versions that shipped without a Release object.
    real = CHANGELOG.read_text()
    for v in ("0.5.23", "0.5.24", "0.5.25"):
        check(f"real CHANGELOG has a usable [{v}] body", len(notes_for(v, real)) > 200)

    print(f"\nself-test: {passed} passed, {failed} failed")
    return 1 if failed else 0


def main() -> int:
    args = sys.argv[1:]
    if not args:
        raise SystemExit(f"usage: {Path(__file__).name} <version> | --self-test")
    if args[0] == "--self-test":
        return self_test()
    version = args[0].lstrip("v")
    print(notes_for(version, CHANGELOG.read_text()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
