#!/usr/bin/env python3
"""Generate the GPM clause-by-clause table from the clause manifest, so the five
statuses are *generated, not typed*.

The gap table in `docs/research/governed-persistent-memory-2608.12476.md` names
five clauses and, for each, whether mnemo ships it, ships a weaker form, or does
not implement it at all. That table was hand-written prose. It will drift the
moment one of the five moves — and the whole value of the page is that it is
honest about which is which.

Source of truth: `docs/research/governed-persistent-memory-clauses.toml`.
This script rewrites the block between the markers:

    <!-- BEGIN generated: gpm-clauses -->
    ...table...
    <!-- END generated: gpm-clauses -->

Modes (same interface as `scripts/gen_published_versions.py`):

    python3 scripts/gen_gpm_clause_table.py            # --write (default)
    python3 scripts/gen_gpm_clause_table.py --print
    python3 scripts/gen_gpm_clause_table.py --check    # CI: non-zero if stale

The *symbol* claims in the manifest are not checked here — that is
`crates/mnemo-compliance/tests/gpm_clause_manifest.rs`, which asserts a shipped
clause points at a symbol that exists and an absent one points at nothing. This
script only owns rendering.
"""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MANIFEST = REPO / "docs/research/governed-persistent-memory-clauses.toml"
DOC = REPO / "docs/research/governed-persistent-memory-2608.12476.md"

BEGIN = "<!-- BEGIN generated: gpm-clauses -->"
END = "<!-- END generated: gpm-clauses -->"

# The glyph carries the status in the rendered table. Kept here rather than in
# the manifest so the data file stays about *facts*, not presentation.
GLYPH = {
    "ships": "✅",
    "partial": "➖",
    "absent": "❌",
    "conflicts": "❌",
}


def render() -> str:
    data = tomllib.loads(MANIFEST.read_text())
    clauses = data["clause"]

    out = [BEGIN]
    out.append(
        "<!-- Generated from governed-persistent-memory-clauses.toml — "
        "do not hand-edit. Regenerate with: python3 scripts/gen_gpm_clause_table.py -->"
    )
    out.append("")
    out.append("| GPM clause | mnemo | what is actually there, and what is missing |")
    out.append("|---|---|---|")
    for c in clauses:
        status = c["status"]
        glyph = GLYPH[status]
        notes = " ".join(c["notes"].split())
        if status == "conflicts":
            cw = c["conflicts_with"]
            notes = f"{notes} **Conflicts with {cw['feature']}:** {cw['detail']}"
        out.append(f"| **{c['name']}** | {glyph} | {notes} |")
    out.append("")

    shipped = [c["id"] for c in clauses if c["status"] == "ships"]
    partial = [c["id"] for c in clauses if c["status"] == "partial"]
    missing = [c["id"] for c in clauses if c["status"] in ("absent", "conflicts")]
    out.append(
        f"_{len(shipped)} of {len(clauses)} clauses shipped, {len(partial)} partial, "
        f"{len(missing)} not implemented. Table generated from "
        "[`governed-persistent-memory-clauses.toml`](governed-persistent-memory-clauses.toml) "
        "by [`scripts/gen_gpm_clause_table.py`](../../scripts/gen_gpm_clause_table.py); "
        "the symbol each row points at is asserted to exist (or not to exist) by "
        "`crates/mnemo-compliance/tests/gpm_clause_manifest.rs`._"
    )
    out.append(END)
    return "\n".join(out)


def main() -> int:
    mode = sys.argv[1] if len(sys.argv) > 1 else "--write"
    block = render()
    if mode == "--print":
        print(block)
        return 0

    text = DOC.read_text()
    if BEGIN not in text or END not in text:
        raise SystemExit(
            f"{DOC.relative_to(REPO)} is missing the markers {BEGIN} / {END}; "
            "add them where the generated table should live."
        )
    pattern = re.compile(re.escape(BEGIN) + r".*?" + re.escape(END), re.DOTALL)
    new = pattern.sub(lambda _: block, text)

    if mode == "--check":
        if new != text:
            print(
                "GPM clause table is STALE — run: "
                "python3 scripts/gen_gpm_clause_table.py",
                file=sys.stderr,
            )
            return 1
        print("GPM clause table is up to date.")
        return 0

    DOC.write_text(new)
    print(f"Rewrote GPM clause table in {DOC.relative_to(REPO)}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
