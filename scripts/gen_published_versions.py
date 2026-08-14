#!/usr/bin/env python3
"""Generate the README "Published versions (per registry)" table from the live
registries, so the numbers are *generated, not typed* (WI1).

Three registries publish mnemo artifacts and they drift independently:
  - crates.io: the Rust library line + the `mnemo` server binary.
  - PyPI:      `mnemo-db`, the Python SDK (versioned independently — see
               python/pyproject.toml).
  - npm:       `@mndfreek/mnemo-sdk`, the TypeScript SDK (independent too).

This script queries each for the currently-published version and its publish
date, then rewrites the block between the README markers:

    <!-- BEGIN generated: published-versions -->
    ...table...
    <!-- END generated: published-versions -->

Version cells are written `v`-prefixed on purpose: the README version fence
(crates/mnemo-cli/tests/readme_crates_version_matches_workspace.rs) flags any
*bare* current-band literal that is not the workspace version, and a published
crate (e.g. 0.5.22 while the workspace is 0.5.23) is exactly such a literal. The
`v` prefix is the fence's sanctioned exemption for true statements about a
specific release, which is what these are.

Usage:
    python3 scripts/gen_published_versions.py            # rewrite the README block
    python3 scripts/gen_published_versions.py --check     # fail if it would change
    python3 scripts/gen_published_versions.py --print     # print the block only

Network is required. If a registry cannot be reached the cell is rendered as
`unknown` rather than silently omitted, so a fetch failure is visible.
"""

from __future__ import annotations

import json
import re
import sys
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
README = REPO / "README.md"
BEGIN = "<!-- BEGIN generated: published-versions -->"
END = "<!-- END generated: published-versions -->"
UA = "mnemo-gen-published-versions (https://github.com/sattyamjjain/mnemo)"

# crates on crates.io worth surfacing: the entry libs, the server binary (the
# one that stranded), and the new bench crate (still uncreated).
CRATES = [
    ("mnemo-core", "engine + hash-chain verify"),
    ("mnemo-mcp-server", "the `mnemo` server binary"),
    ("mnemo-embeddings-bench", "bench crate the server binary depends on"),
]


def _get_json(url: str) -> dict | None:
    try:
        req = urllib.request.Request(url, headers={"User-Agent": UA})
        with urllib.request.urlopen(req, timeout=20) as r:
            return json.load(r)
    except Exception:
        return None


def workspace_version() -> str:
    text = (REPO / "Cargo.toml").read_text()
    in_wp = False
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("["):
            in_wp = s == "[workspace.package]"
            continue
        if in_wp and s.startswith("version"):
            m = re.search(r'"([^"]+)"', s)
            if m:
                return m.group(1)
    raise SystemExit("could not read [workspace.package].version")


def crates_io(name: str) -> tuple[str, str]:
    d = _get_json(f"https://crates.io/api/v1/crates/{name}")
    if not d or "crate" not in d:
        return ("absent", "—")  # 404 → crate never created
    ver = d["crate"].get("max_version", "unknown")
    date = "—"
    for v in d.get("versions", []):
        if v.get("num") == ver:
            date = (v.get("created_at") or "")[:10] or "—"
            break
    if date == "—":
        date = (d["crate"].get("updated_at") or "")[:10] or "—"
    return (ver, date)


def pypi(name: str) -> tuple[str, str]:
    d = _get_json(f"https://pypi.org/pypi/{name}/json")
    if not d:
        return ("unknown", "—")
    ver = d["info"]["version"]
    files = d.get("releases", {}).get(ver, [])
    date = (files[0].get("upload_time") if files else "")[:10] or "—"
    return (ver, date)


def npm(name: str) -> tuple[str, str]:
    # scoped name @scope/pkg must be URL-encoded as %2F.
    d = _get_json(f"https://registry.npmjs.org/{name.replace('/', '%2F')}")
    if not d:
        return ("unknown", "—")
    ver = d.get("dist-tags", {}).get("latest", "unknown")
    date = (d.get("time", {}).get(ver, "") or "")[:10] or "—"
    return (ver, date)


def vcell(ver: str) -> str:
    if ver in ("absent", "unknown"):
        return f"_{ver}_"
    return f"`v{ver}`"


def render() -> str:
    ws = workspace_version()
    rows = []
    # Track whether the Rust line has actually shipped the workspace version, so
    # the header says "released" or "unreleased target" from live registry truth
    # rather than a hand-maintained word that goes stale the moment a release
    # lands (which is exactly what happened to the 0.4.4 heads-up this block
    # replaced).
    shipped = True
    for name, note in CRATES:
        ver, date = crates_io(name)
        if ver != ws:
            shipped = False
        rows.append((f"crates.io", f"`{name}` — {note}", vcell(ver), date))
    pv, pd = pypi("mnemo-db")
    rows.append(("PyPI", "`mnemo-db` — Python SDK (independent)", vcell(pv), pd))
    nv, nd = npm("@mndfreek/mnemo-sdk")
    rows.append(("npm", "`@mndfreek/mnemo-sdk` — TypeScript SDK (independent)", vcell(nv), nd))

    out = [BEGIN]
    out.append(f"<!-- Regenerate with: python3 scripts/gen_published_versions.py -->")
    out.append("")
    state = "released" if shipped else "unreleased target"
    out.append(f"Workspace `[workspace.package].version` ({state}): **`v{ws}`**. "
               "The Rust library line tracks the workspace; the Python and TypeScript SDKs "
               "version independently. Published, per registry:")
    out.append("")
    out.append("| Registry | Artifact | Published version | Published |")
    out.append("|---|---|---|---|")
    for reg, art, ver, date in rows:
        out.append(f"| {reg} | {art} | {ver} | {date} |")
    out.append("")
    out.append("_Table generated from the live registries by "
               "[`scripts/gen_published_versions.py`](scripts/gen_published_versions.py); "
               "`scripts/registry_parity.sh` fails a release if these drift from what "
               "the release actually published._")
    out.append(END)
    return "\n".join(out)


def main() -> int:
    mode = sys.argv[1] if len(sys.argv) > 1 else "--write"
    block = render()
    if mode == "--print":
        print(block)
        return 0
    text = README.read_text()
    if BEGIN not in text or END not in text:
        raise SystemExit(
            f"README.md is missing the markers {BEGIN} / {END}; add them where the "
            "generated table should live."
        )
    pattern = re.compile(re.escape(BEGIN) + r".*?" + re.escape(END), re.DOTALL)
    new = pattern.sub(lambda _: block, text)
    if mode == "--check":
        if new != text:
            print("README published-versions block is STALE — run: "
                  "python3 scripts/gen_published_versions.py", file=sys.stderr)
            return 1
        print("README published-versions block is up to date.")
        return 0
    README.write_text(new)
    print("Rewrote README published-versions block.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
