#!/usr/bin/env python3
"""Generate the README's real-embedder recall paragraph from the result file.

A number typed into a README is a number that rots. This repo has already
learned that twice: the crate-name collision was "verified once, manually" with
nothing to keep it verified, and the published-versions table sat stale from
2026-08-14 while `mnemo-core` moved on. The recall number is the most
load-bearing claim in the README, so it is generated, not typed.

Source of truth: `bench/results/locomo_v1.json`, written by
`bench/locomo/src/bin/locomo_v1_bench.rs`. This script never computes a metric;
it only renders what the bench measured. If the bench did not record a field,
this prints nothing for it rather than inventing a value.

Modes (same interface as scripts/gen_published_versions.py):

    python3 scripts/gen_recall_number.py            # --write (default)
    python3 scripts/gen_recall_number.py --print
    python3 scripts/gen_recall_number.py --check    # CI: non-zero if stale
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
RESULT = REPO / "bench/results/locomo_v1.json"
README = REPO / "README.md"

BEGIN = "<!-- BEGIN generated: recall-number -->"
END = "<!-- END generated: recall-number -->"


def pct(x: float) -> str:
    return f"{x:.3f}"


def render() -> str:
    d = json.loads(RESULT.read_text())
    emb = d["embedder"]
    sem = d["strategies"]["semantic"]
    lex = d["strategies"]["lexical"]

    sem_ci = sem["recall@1_ci95"]
    lex_ci = lex["recall@1_ci95"]
    sha = emb.get("model_sha256")
    sha_short = f"`{sha[:12]}…`" if sha else "_not recorded_"
    model_id = emb.get("model_id") or emb.get("model")
    src = emb.get("model_source")

    out = [BEGIN]
    out.append(
        "<!-- Generated from bench/results/locomo_v1.json by "
        "scripts/gen_recall_number.py — do not hand-edit. -->"
    )
    out.append("")
    out.append(
        f"**Real-embedder recall, measured on the supported backend.** "
        f"Gold-document **recall@1 = {pct(sem['recall@1'])}** "
        f"[Wilson 95% {pct(sem_ci[0])}, {pct(sem_ci[1])}], "
        f"recall@5 {pct(sem['recall@5'])}, recall@10 {pct(sem['recall@10'])}, "
        f"MRR {pct(sem['mrr'])}."
    )
    out.append("")
    out.append("| | |")
    out.append("|---|---|")
    out.append(f"| embedder | `{model_id}` ({emb['dim']}-dim, backend `{emb['backend']}`) |")
    out.append(f"| weights sha256 | {sha_short} |")
    if src:
        out.append(f"| weights source | {src} |")
    out.append(f"| storage backend | `{d.get('storage_backend', 'not recorded')}` |")
    out.append(
        f"| corpus | `{d['corpus']['dataset']}`, n={d['n']} queries, "
        f"mean of {d['repeats']} seeds |"
    )
    out.append(
        f"| **control (lexical)** | recall@1 {pct(lex['recall@1'])} "
        f"[{pct(lex_ci[0])}, {pct(lex_ci[1])}] |"
    )
    out.append(f"| hardware | {d.get('hardware', 'not recorded')} |")
    out.append(
        f"| measured | {d.get('generated_at_utc', 'not recorded')} "
        f"at `{d.get('commit', 'unknown')}` |"
    )
    out.append("")
    out.append(
        "The lexical row is the control, not a second headline: it is the same "
        "corpus and the same harness with the vector lane switched off, so the "
        "gap between the two rows is what the embedder is actually buying."
    )
    out.append("")
    out.append("Reproduce:")
    out.append("")
    out.append("```bash")
    out.append("# fetch the exact weights the sha256 above pins")
    if src:
        out.append(f"curl -sSL --fail -o model.onnx {src}")
    out.append(
        "MNEMO_ONNX_MODEL_PATH=./model.onnx cargo run --release --features onnx \\"
    )
    out.append("  -p mnemo-locomo-bench --bin locomo_v1_bench")
    out.append("```")
    out.append("")
    prelim = d.get("preliminary")
    out.append(
        "**What this number is not.** It is not a LoCoMo leaderboard score and is "
        "not comparable to one: the corpus is the bundled LongMemEval_M slice, not "
        "full LoCoMo. It is retrieval quality only, with no LLM in the loop and no "
        "answer-correctness judge. It says nothing about poisoning resistance or "
        "audit integrity, which are measured separately."
        + (
            f" **n={d['n']} is below 100, so the bench marks it `preliminary`;** "
            "treat the interval, not the point estimate, as the claim."
            if prelim
            else ""
        )
    )
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
            "generated recall paragraph should live."
        )
    new = re.compile(re.escape(BEGIN) + r".*?" + re.escape(END), re.DOTALL).sub(
        lambda _: block, text
    )
    if mode == "--check":
        if new != text:
            print(
                "README recall-number block is STALE vs bench/results/locomo_v1.json "
                "— run: python3 scripts/gen_recall_number.py",
                file=sys.stderr,
            )
            return 1
        print("README recall-number block is up to date.")
        return 0
    README.write_text(new)
    print("Rewrote README recall-number block.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
