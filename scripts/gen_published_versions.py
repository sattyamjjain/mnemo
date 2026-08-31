#!/usr/bin/env python3
"""Generate the README "Published versions (per registry)" table from the live
registries, so the numbers are *generated, not typed* (WI1).

Three registries publish mnemo artifacts:
  - crates.io: the Rust library line + the `mnemo` server binary.
  - PyPI:      `mnemo-db`, the Python SDK. NOT independently versioned: it is
               PyO3 bindings that compile `mnemo-core` into the wheel, so the
               wheel version names the engine inside it, and
               `workspace_version_fence.rs::python_sdk_version_matches_the_workspace`
               fails CI if it drifts.
  - npm:       `@mndfreek/mnemo-sdk`, the TypeScript SDK. Genuinely independent:
               a thin MCP-over-STDIO client that embeds nothing.

This script generates TWO blocks, because a hand-written version in prose is the
root cause it exists to remove. The README used to carry "Its current release is
`mnemo-db` 0.5.12" in prose and then reason about wire compatibility from that
premise; PyPI was on 0.5.26 by then, and the reasoning had inverted along with
the number. Generating the table and leaving the paragraph typed just moves the
staleness somewhere less visible.

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
import subprocess
import re
import sys
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
README = REPO / "README.md"
BEGIN = "<!-- BEGIN generated: published-versions -->"
END = "<!-- END generated: published-versions -->"
ROSTER_BEGIN = "<!-- BEGIN generated: published-crate-roster -->"
ROSTER_END = "<!-- END generated: published-crate-roster -->"
COMPAT_BEGIN = "<!-- BEGIN generated: python-sdk-compat -->"
COMPAT_END = "<!-- END generated: python-sdk-compat -->"
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


# Crates that are publishable in cargo metadata but deliberately never reach
# crates.io. Mirrors `exemption_reason()` in scripts/check_publish_closure.sh;
# that script is the enforcing copy, this one only needs the names so the roster
# below counts what actually ships.
NEVER_PUBLISHED = {"mnemo-python", "mnemo-golem-host"}

# Crates whose presence in the roster needs a word of explanation. Keyed by name
# so the note disappears if the crate ever does.
ROSTER_NOTES = {
    "mnemo-db": (
        "One of them, **`mnemo-db`, ships no code**: it is a defensive name "
        "reservation whose entire contents are a doc comment pointing at "
        "`mnemo-core` and `mnemo-mcp`. It is counted because it is a real "
        "published artifact someone can `cargo add`, and they should learn that "
        "from the count rather than from an empty crate."
    ),
}


def shipping_crates() -> list[str]:
    """Publishable workspace members that actually go to crates.io.

    Derived from `cargo metadata`, not typed. The count and the list used to be
    hand-written prose next to a hand-written version table; the table was wrong
    within a day of the release it described.
    """
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1",
         "--manifest-path", str(REPO / "Cargo.toml")],
        capture_output=True, text=True, check=True,
    )
    m = json.loads(out.stdout)
    ids = set(m["workspace_members"])
    names = {
        p["name"]
        for p in m["packages"]
        if p["id"] in ids and p.get("publish") != [] and p["name"] not in NEVER_PUBLISHED
    }
    return sorted(names)


def registry_versions(names: list[str]) -> dict[str, str]:
    """Live max_version for each name, in ONE query.

    Deliberately not 21 separate calls: doing that got this session rate-limited
    by crates.io, after which the per-crate lookups simply hung and a naive
    caller would have read the timeouts as "crate absent".
    """
    d = _get_json("https://crates.io/api/v1/crates?q=mnemo&per_page=100") or {}
    found = {c["name"]: c.get("max_version", "unknown") for c in d.get("crates", [])}
    return {n: found.get(n, "absent") for n in names}


def render_crate_roster() -> str:
    ws = workspace_version()
    names = shipping_crates()
    vers = registry_versions(names)
    behind = sorted(n for n, v in vers.items() if v != ws)

    out = [ROSTER_BEGIN,
           "<!-- Regenerate with: python3 scripts/gen_published_versions.py -->",
           ""]
    if not behind:
        out.append(
            f"Installing the right *name* is only half of it: `cargo install` resolves "
            f"whatever crates.io actually has. All **{len(names)}** published `mnemo-*` "
            f"crates are on **`v{ws}`**, the current workspace version — verified against "
            f"the live registry when this block was generated, not asserted."
        )
    else:
        out.append(
            f"Installing the right *name* is only half of it: `cargo install` resolves "
            f"whatever crates.io actually has. Of the **{len(names)}** published "
            f"`mnemo-*` crates, **{len(behind)}** are not yet on the workspace version "
            f"`v{ws}`: "
            + ", ".join(f"`{n}` (`{vers[n]}`)" for n in behind)
            + ". That is either a release in flight or a stranded crate; "
            "[`scripts/check_version_drift.sh`](scripts/check_version_drift.sh) "
            "distinguishes the two by naming the crates rather than reporting a total."
        )
    out.append("")
    out.append("The " + str(len(names)) + " are " +
               ", ".join(f"`{n}`" for n in names[:-1]) + f" and `{names[-1]}`.")
    for name, note in ROSTER_NOTES.items():
        if name in names:
            out.append("")
            out.append(note)
    out.append("")
    out.append(
        "_Count and list generated from `cargo metadata` (publishable workspace members, "
        "less those with a written never-published exemption) and checked against the live "
        "registry by [`scripts/gen_published_versions.py`](scripts/gen_published_versions.py)._"
    )
    out.append(ROSTER_END)
    return "\n".join(out)


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
    rows.append(("PyPI", "`mnemo-db` — Python SDK (tracks the workspace)", vcell(pv), pd))
    nv, nd = npm("@mndfreek/mnemo-sdk")
    rows.append(("npm", "`@mndfreek/mnemo-sdk` — TypeScript SDK (independent)", vcell(nv), nd))

    out = [BEGIN]
    out.append(f"<!-- Regenerate with: python3 scripts/gen_published_versions.py -->")
    out.append("")
    state = "released" if shipped else "unreleased target"
    out.append(f"Workspace `[workspace.package].version` ({state}): **`v{ws}`**. "
               "The Rust library line and the Python SDK both track the workspace (the "
               "wheel compiles `mnemo-core` into itself, so its version names the engine "
               "inside it). Only the TypeScript SDK versions independently. Published, "
               "per registry:")
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


def _newer(a: str, b: str) -> bool:
    """True when semver-ish `a` is strictly newer than `b`. Unparseable -> False."""

    def parts(v: str):
        try:
            return tuple(int(x) for x in v.split("-")[0].split(".")[:3])
        except ValueError:
            return None

    pa, pb = parts(a), parts(b)
    return bool(pa and pb and pa > pb)


def render_python_compat() -> str:
    """Generate the Python SDK version-and-compatibility paragraph.

    This paragraph reasons *from* a version number, so it cannot be half
    generated: the number and the argument that depends on it are produced
    together or they go stale together.
    """
    ws = workspace_version()
    pv, _ = pypi("mnemo-db")
    core, _ = crates_io("mnemo-core")

    out = [COMPAT_BEGIN]
    out.append("<!-- Regenerate with: python3 scripts/gen_published_versions.py -->")
    out.append("")

    if pv in ("absent", "unknown"):
        out.append(
            "> **Version line & wire compatibility.** PyPI could not be reached when this "
            "block was generated, so no version is stated here rather than a stale one. "
            "Re-run `python3 scripts/gen_published_versions.py`."
        )
        out.append(COMPAT_END)
        return "\n".join(out)

    # The workspace being AHEAD of PyPI is the normal state of an open release
    # window: the version is bumped on merge and published on a tag, so between
    # those two events they differ by design. An earlier draft of this block
    # called that "a bug ... the fence above should have caught it", which would
    # have printed a false accusation into the README on every version bump —
    # and blamed a guard that is working correctly. Only PyPI being ahead of the
    # workspace is a real inversion.
    tracks = pv == ws
    pypi_ahead = _newer(pv, ws)
    lead = (
        f"> **Version line & wire compatibility.** `pip install mnemo-db` gives "
        f"**`v{pv}`**. The Python SDK is **not** independently versioned: `python/` is "
        f"PyO3 bindings that compile `mnemo-core` *into the wheel*, so the wheel version "
        f"names the engine inside it, and "
        f"[`workspace_version_fence.rs`](crates/mnemo-cli/tests/workspace_version_fence.rs) "
        f"fails CI if `pyproject.toml` and `mnemo/__init__.py` drift from "
        f"`[workspace.package].version`."
    )
    if pypi_ahead:
        lead += (
            f" **PyPI is AHEAD of the workspace right now: PyPI has `v{pv}`, the workspace "
            f"is `v{ws}`.** That inversion is a bug — a wheel cannot ship an engine the "
            f"source tree has not reached."
        )
    elif not tracks:
        lead += (
            f" The workspace is currently `v{ws}` and PyPI is `v{pv}`: that is an **open "
            f"release window**, not drift. The version is bumped on merge and published "
            f"on a tag, so the two differ between those events by design. `pip install "
            f"mnemo-db` gives `v{pv}` until `v{ws}` is tagged."
        )
    out.append(lead)
    out.append(">")
    out.append(
        f"> - **In-process, `MnemoClient` (the PyO3 extension).** `mnemo-db` `v{pv}` *is* "
        f"`mnemo-core` `v{pv}`. There is no version-skew question to answer: the engine "
        f"is the wheel."
    )

    if core not in ("absent", "unknown") and core != pv:
        out.append(
            f"> - **`pip install mnemo-db` and `cargo add mnemo-core` do not currently "
            f"resolve the same version.** PyPI has `v{pv}`; crates.io has `v{core}`. The "
            f"wheel publishes on merge to `main` while the crates publish on a tag, so "
            f"the Python side leads inside an open release window. Pin deliberately if "
            f"you embed both."
        )

    out.append(
        "> - **Over MCP, the `agno` / `camel` / `agno-memory` adapters.** These embed no "
        "engine; they spawn the external `mnemo` server binary you install and bind to "
        "its **MCP tool surface** (the 23 registered tools), not to a `mnemo-core` "
        "version. They are wire-compatible with any **0.5.x** `mnemo-mcp-server`. Server "
        "properties such as the rmcp 3.0 transport and the tool-catalog attestation come "
        "from **that binary**, not from the SDK, so run a current one to get them."
    )
    out.append(COMPAT_END)
    return "\n".join(out)


def main() -> int:
    mode = sys.argv[1] if len(sys.argv) > 1 else "--write"
    blocks = [
        ("published-versions", BEGIN, END, render()),
        ("published-crate-roster", ROSTER_BEGIN, ROSTER_END, render_crate_roster()),
        ("python-sdk-compat", COMPAT_BEGIN, COMPAT_END, render_python_compat()),
    ]
    if mode == "--print":
        for _, _, _, block in blocks:
            print(block)
            print()
        return 0

    text = README.read_text()
    new = text
    for label, begin, end, block in blocks:
        if begin not in new or end not in new:
            raise SystemExit(
                f"README.md is missing the markers {begin} / {end}; add them where the "
                f"generated `{label}` block should live."
            )
        pattern = re.compile(re.escape(begin) + r".*?" + re.escape(end), re.DOTALL)
        new = pattern.sub(lambda _, b=block: b, new)

    if mode == "--check":
        if new != text:
            stale = [
                label
                for label, begin, end, block in blocks
                if re.search(re.escape(begin) + r".*?" + re.escape(end), text, re.DOTALL).group(0)
                != block
            ]
            print(
                "README generated block(s) STALE: "
                + ", ".join(stale)
                + " — run: python3 scripts/gen_published_versions.py",
                file=sys.stderr,
            )
            return 1
        print(
            "README generated blocks are up to date ("
            + ", ".join(label for label, _, _, _ in blocks)
            + ")."
        )
        return 0
    README.write_text(new)
    print(
        "Rewrote README generated blocks ("
        + ", ".join(label for label, _, _, _ in blocks)
        + ")."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
