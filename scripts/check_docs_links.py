#!/usr/bin/env python3
"""Internal-link checker for the mdBook under docs/.

Fails (exit 1) if any of these is true, so a dead internal link blocks the docs
build in CI instead of shipping a 404 into the published site:

  1. A relative Markdown link or image in docs/src/**/*.md points at a file that
     does not exist (anchors are stripped before the existence check).
  2. A `[title](path.md)` entry in docs/src/SUMMARY.md points at a missing chapter.

External links (http/https/mailto) and pure in-page anchors (`#section`) are out
of scope by design: this guard is about internal wiring, not link-rot on the web,
so it stays deterministic and offline. Run:

    python3 scripts/check_docs_links.py
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC = REPO_ROOT / "docs" / "src"

# [text](target) and ![alt](target) — capture the target.
LINK_RE = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")


def is_external(target: str) -> bool:
    return target.startswith(("http://", "https://", "mailto:", "tel:", "//"))


def check_file(md: Path) -> list[str]:
    problems: list[str] = []
    text = md.read_text(encoding="utf-8")
    for lineno, line in enumerate(text.splitlines(), 1):
        for m in LINK_RE.finditer(line):
            target = m.group(1).strip()
            # Strip an optional `"title"` after the URL: [x](path "t")
            target = target.split()[0] if target else target
            if not target or target.startswith("#") or is_external(target):
                continue
            path_part = target.split("#", 1)[0]
            if not path_part:  # was a pure anchor after all
                continue
            resolved = (md.parent / path_part).resolve()
            if not resolved.exists():
                problems.append(
                    f"{md.relative_to(REPO_ROOT)}:{lineno}: dead link -> {target}"
                )
    return problems


def main() -> int:
    if not SRC.is_dir():
        print(f"::error::docs source dir not found: {SRC}", file=sys.stderr)
        return 1

    problems: list[str] = []
    for md in sorted(SRC.rglob("*.md")):
        problems.extend(check_file(md))

    for p in problems:
        print(f"::error::{p}")

    total = len(list(SRC.rglob("*.md")))
    if problems:
        print(f"\nchecked {total} markdown files under docs/src — "
              f"{len(problems)} dead internal link(s)")
        return 1
    print(f"checked {total} markdown files under docs/src — all internal links resolve")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
