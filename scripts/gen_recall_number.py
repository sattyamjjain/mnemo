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
    python3 scripts/gen_recall_number.py --self-test

The self-test exists because the interesting branch is the one that does not
run: when the paired gap *fails* to separate, this file must say so and say
what n would fix it. That path renders on no committed result today, so without
a fixture it would be dead code that first executes on the day it matters.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
RESULT = REPO / "bench/results/locomo_v1.json"
README = REPO / "README.md"
BENCH_INDEX = REPO / "docs/benchmarks/index.md"

BEGIN = "<!-- BEGIN generated: recall-number -->"
END = "<!-- END generated: recall-number -->"

# The intro paragraph near the top of the README restated the same point
# estimates by hand, fifteen lines above the generated block that produces them.
# That is the exact duplication this file exists to remove, so it is generated
# from the same result rather than retyped.
HEAD_BEGIN = "<!-- BEGIN generated: recall-headline -->"
HEAD_END = "<!-- END generated: recall-headline -->"


def pct(x: float) -> str:
    return f"{x:.3f}"


def signed(x: float) -> str:
    return f"{x:+.3f}"


def fmt_p(p: float) -> str:
    """Render a p-value without pretending to precision it does not have."""
    if p < 1e-3:
        return f"{p:.1e}".replace("e-0", "e-")
    return f"{p:.4f}".rstrip("0")


PRIMARY = "semantic_vs_lexical"

# The comparison against the strategy a user actually gets. `recall()` resolves
# an unset strategy to "auto" (crates/mnemo-core/src/query/recall.rs:357,
# `request.strategy.as_deref().unwrap_or("auto")`), so `auto` is not one option
# among five — it is the shipped default and the path every caller who does not
# name a strategy takes.
#
# The bench has measured semantic-vs-auto for as long as the keyed result shape
# has existed, and this file rendered only semantic-vs-lexical. That published a
# +0.267 gap over a control nobody runs while staying silent on the +0.058
# [-0.031, 0.160] gap over the path everyone runs — the second one does not
# separate. Publishing only the separating comparison is the exact selective
# reporting docs/BENCH_POISONING.md and docs/security/known-limitations.md exist
# to forbid, so both are rendered now and the non-separating one is labelled.
SECONDARY = "semantic_vs_auto"
DEFAULT_STRATEGY = "auto"


def primary_paired(d: dict) -> dict | None:
    """The headline paired comparison, tolerating both result-file shapes.

    Early files stored a single flat comparison; current ones store a map
    keyed by comparison name because the bench now also pairs semantic against
    the default `auto` fusion.
    """
    p = d.get("paired")
    if not p:
        return None
    return p.get(PRIMARY, p if "mean_diff" in p else None)


def secondary_paired(d: dict) -> dict | None:
    """The comparison against the shipped default, when the bench recorded one.

    Only the keyed result shape carries it. Early flat files have a single
    comparison and no notion of a second one, so they render nothing here
    rather than having a claim invented for them.
    """
    p = d.get("paired")
    if not isinstance(p, dict):
        return None
    sec = p.get(SECONDARY)
    return sec if isinstance(sec, dict) and "mean_diff" in sec else None


def crosses_zero(ci: list) -> bool:
    """True when a 95% interval spans zero, i.e. the sign is not established."""
    return len(ci) == 2 and ci[0] <= 0.0 <= ci[1]


def n_for_separation(d: dict, better: str = "semantic", baseline: str = "lexical") -> int | None:
    """Approximate paired n needed for the mean difference to clear zero at 95%.

    Only meaningful when the current sample does *not* separate; the README
    then says what it would take instead of leaving the reader to guess. Uses
    the normal approximation n >= (1.96 * sd / mean)^2 on the per-query
    differences, which is the standard paired sample-size form. Returns None
    when the bench recorded no per-query vectors, or when the observed mean
    difference is zero and no n can rescue it.
    """
    strat = d.get("strategies", {})
    a = strat.get(better, {}).get("hits1_by_query")
    b = strat.get(baseline, {}).get("hits1_by_query")
    if not a or not b or len(a) != len(b):
        return None
    reps = max(int(d.get("repeats", 1)), 1)
    diffs = [(x - y) / reps for x, y in zip(a, b)]
    n = len(diffs)
    mean = sum(diffs) / n
    if mean == 0 or n < 2:
        return None
    var = sum((x - mean) ** 2 for x in diffs) / (n - 1)
    sd = var**0.5
    if sd == 0:
        return None
    import math

    return max(2, math.ceil((1.959963984540054 * sd / abs(mean)) ** 2))


def render(result_path: Path | None = None) -> str:
    d = json.loads((result_path or RESULT).read_text())
    emb = d["embedder"]
    sem = d["strategies"]["semantic"]
    lex = d["strategies"]["lexical"]
    # Older result files predate the paired block. Render nothing rather than
    # inventing a comparison the bench did not measure.
    paired = primary_paired(d)
    vs_default = secondary_paired(d)
    dflt = d.get("strategies", {}).get(DEFAULT_STRATEGY)

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
        + (
            # The comparison against the control belongs in the same breath as
            # the number, not in a footnote: the headline is only interesting
            # relative to the control, and a reader who stops after the first
            # sentence should already know whether the gap survives its own
            # interval.
            (
                f" Against the lexical control **on the same {paired['n']} queries** "
                f"the paired gap is **{signed(paired['mean_diff'])}** "
                f"[95% {pct(paired['mean_diff_ci95'][0])}, "
                f"{pct(paired['mean_diff_ci95'][1])}], "
                f"McNemar exact p={fmt_p(paired['mcnemar']['exact_p'])} "
                f"({paired['mcnemar']['b_better_only']} queries won, "
                f"{paired['mcnemar']['c_baseline_only']} lost) — "
                + (
                    "the gap separates at 95%."
                    if paired.get("separates_at_95")
                    else "**the gap does not separate at 95%**"
                    + (
                        f"; roughly n={n_needed} paired queries would be needed "
                        "at this effect size."
                        if (n_needed := n_for_separation(d))
                        else "."
                    )
                )
            )
            if paired
            else ""
        )
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
    if paired:
        mc = paired["mcnemar"]
        out.append(
            f"| **paired gap vs control** | {signed(paired['mean_diff'])} "
            f"[{pct(paired['mean_diff_ci95'][0])}, {pct(paired['mean_diff_ci95'][1])}], "
            f"McNemar b={mc['b_better_only']}/c={mc['c_baseline_only']}, "
            f"exact p={fmt_p(mc['exact_p'])} |"
        )
    # The default's own recall@1 belongs next to the control's, or the paired
    # row below has no anchor a reader can check it against.
    if dflt and "recall@1" in dflt:
        dc = dflt.get("recall@1_ci95")
        out.append(
            f"| **shipped default (`{DEFAULT_STRATEGY}`)** | recall@1 {pct(dflt['recall@1'])}"
            + (f" [{pct(dc[0])}, {pct(dc[1])}]" if dc else "")
            + " |"
        )
    if vs_default:
        mc = vs_default["mcnemar"]
        ci = vs_default["mean_diff_ci95"]
        out.append(
            f"| **paired gap vs default** | {signed(vs_default['mean_diff'])} "
            f"[{pct(ci[0])}, {pct(ci[1])}], "
            f"McNemar b={mc['b_better_only']}/c={mc['c_baseline_only']}, "
            f"exact p={fmt_p(mc['exact_p'])} — "
            + (
                "separates at 95%"
                if vs_default.get("separates_at_95")
                else "**does not separate at 95%**"
            )
            + " |"
        )
    out.append(f"| hardware | {d.get('hardware', 'not recorded')} |")
    out.append(
        f"| measured | {d.get('generated_at_utc', 'not recorded')} "
        f"at `{d.get('commit', 'unknown')}` |"
    )
    out.append("")
    out.append(
        "The lexical row is the control, not a second headline: it is the same "
        "corpus and the same harness with the vector lane switched off, so what "
        "the embedder buys is the difference between them."
        + (
            # Two marginal intervals cannot be subtracted, and these two in
            # particular overlap. Saying so explicitly is cheaper than letting a
            # reader do the arithmetic that does not work.
            " **Do not read that difference off the two intervals** — they "
            f"overlap ({pct(sem_ci[0])} sits below {pct(lex_ci[1])}), and "
            "overlapping intervals neither establish nor rule out a difference. "
            "The paired row is the one that answers it: the same queries scored "
            "both ways, so each query is its own control."
            if paired and sem_ci[0] <= lex_ci[1]
            else ""
        )
    )
    out.append("")

    # The comparison a reader actually needs. The headline gap is measured
    # against a control nobody runs; this one is measured against the path
    # every caller takes by default, and it does not separate. Publishing the
    # first without the second is selective reporting, not brevity.
    if vs_default:
        mc = vs_default["mcnemar"]
        ci = vs_default["mean_diff_ci95"]
        seps = vs_default.get("separates_at_95")
        nd = n_for_separation(d, "semantic", DEFAULT_STRATEGY)
        out.append(
            f"**Against the strategy you actually get.** `{DEFAULT_STRATEGY}` is the "
            f"default: `recall()` resolves an unset `strategy` to `{DEFAULT_STRATEGY}`, "
            "so unless a caller names one, this is the path they are on"
            + (
                f" — and it scores recall@1 {pct(dflt['recall@1'])} here, not "
                f"{pct(sem['recall@1'])}."
                if dflt and "recall@1" in dflt
                else "."
            )
            + f" On the same {vs_default.get('n', d['n'])} queries semantic beats it by "
            f"**{signed(vs_default['mean_diff'])}** "
            f"[95% {pct(ci[0])}, {pct(ci[1])}], McNemar exact "
            f"p={fmt_p(mc['exact_p'])} ({mc['b_better_only']} queries won, "
            f"{mc['c_baseline_only']} lost)."
            + (
                f" That interval **separates at 95%**."
                if seps
                else (
                    f" **That interval runs from {pct(ci[0])} to {pct(ci[1])}: it "
                    "contains 0, so the gap does not separate at 95% and the sign "
                    "of the difference is not established by this sample.**"
                    if crosses_zero(ci)
                    else " **The gap does not separate at 95%.**"
                )
                + (
                    f" Roughly n={nd} paired queries would be needed at this effect "
                    "size."
                    if nd
                    else ""
                )
            )
            + f" The {signed(paired['mean_diff'])} headline above is against the "
            "lexical control, which is not the default and which nobody runs by "
            "choice; it is the embedder's contribution, not the improvement a user "
            "sees over stock behaviour."
            if paired
            else ""
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
    records = d.get("corpus", {}).get("records")
    out.append(
        "**What this number is not.** It is not a LoCoMo leaderboard score and is "
        "not comparable to one: the corpus is the bundled LongMemEval_M slice, not "
        "full LoCoMo. It is retrieval quality only, with no LLM in the loop and no "
        "answer-correctness judge. It says nothing about poisoning resistance or "
        "audit integrity, which are measured separately."
        + (
            f" **n={d['n']} is below 100, so the bench marks it `preliminary`;** "
            "treat the interval, not the point estimate, as the claim."
            # `preliminary` is set by `n < 100` in locomo_v1_bench.rs, and the
            # bundled corpus is the whole input, not a sample of it. So the label
            # can never clear by re-running — it is a statement about the corpus,
            # not about unfinished work, and a reader cannot tell those apart
            # from the word alone.
            + (
                f" **This label cannot clear by re-running.** The corpus is the "
                f"bundled `{Path(d['corpus']['dataset']).name}`, which is "
                f"{records} records in full — not a sample of a larger local file "
                f"— and the bench sets `preliminary` whenever n < 100. So it "
                "reflects a corpus-size decision, not work in progress. Clearing "
                "it honestly needs the larger public LongMemEval slice, which is "
                "access-gated (see "
                "[#44](https://github.com/sattyamjjain/mnemo/issues/44)); until "
                "that is vendored under a recorded licence, this number stays "
                f"`preliminary` at n={records}."
                if records is not None and records == d.get("n")
                else ""
            )
            if prelim
            else ""
        )
    )
    out.append(END)
    return "\n".join(out)


def render_headline(result_path: Path | None = None) -> str:
    """One sentence naming n and the paired verdict, for the top of the README."""
    d = json.loads((result_path or RESULT).read_text())
    emb = d["embedder"]
    sem = d["strategies"]["semantic"]
    paired = primary_paired(d)

    verdict = ""
    if paired:
        mc = paired["mcnemar"]
        verdict = (
            f" On the same {paired['n']} queries the paired gap over that control "
            f"is **{signed(paired['mean_diff'])}** "
            f"[95% {pct(paired['mean_diff_ci95'][0])}, {pct(paired['mean_diff_ci95'][1])}], "
            f"McNemar exact p={fmt_p(mc['exact_p'])}"
            + (
                " — it separates."
                if paired.get("separates_at_95")
                else " — **it does not separate**"
                + (
                    f", and would need roughly n={n} to."
                    if (n := n_for_separation(d))
                    else "."
                )
            )
        )

    return "\n".join(
        [
            HEAD_BEGIN,
            "<!-- Generated from bench/results/locomo_v1.json by "
            "scripts/gen_recall_number.py — do not hand-edit. -->",
            "",
            f"The one headline real-embedder number is the generated block below: "
            # Prefer the citable checkpoint id over the directory name scraped
            # from the path: two different checkpoints can share a folder name.
            f"**{(emb.get('model_id') or emb['model']).split(' (')[0]} "
            f"{emb['dim']}-dim, n={d['n']}, "
            f"recall@1 {pct(sem['recall@1'])}** "
            f"[{pct(sem['recall@1_ci95'][0])}, {pct(sem['recall@1_ci95'][1])}], "
            f"against a lexical control on the same corpus and harness."
            + verdict,
            HEAD_END,
        ]
    )


def _fixture(
    sem_hits: list[int],
    lex_hits: list[int],
    paired: dict,
    auto_hits: list[int] | None = None,
) -> dict:
    """Minimal result file shaped like the real one, for the self-test."""
    n = len(sem_hits)
    return {
        "n": n,
        "repeats": 5,
        "preliminary": n < 100,
        "corpus": {"dataset": "fixture.jsonl"},
        "embedder": {"dim": 384, "backend": "onnx", "model": "fixture"},
        "paired": paired,
        "strategies": {
            "semantic": {
                "recall@1": sum(sem_hits) / (5 * n),
                "recall@1_ci95": [0.5, 0.8],
                "recall@5": 0.9,
                "recall@10": 0.9,
                "mrr": 0.7,
                "hits1_by_query": sem_hits,
            },
            "lexical": {
                "recall@1": sum(lex_hits) / (5 * n),
                "recall@1_ci95": [0.3, 0.6],
                "hits1_by_query": lex_hits,
            },
            **(
                {
                    "auto": {
                        "recall@1": sum(auto_hits) / (5 * n),
                        "recall@1_ci95": [0.45, 0.7],
                        "hits1_by_query": auto_hits,
                    }
                }
                if auto_hits
                else {}
            ),
        },
    }


def self_test() -> int:
    import tempfile

    failures: list[str] = []

    def check(name: str, cond: bool, detail: str = "") -> None:
        if cond:
            print(f"  ok   {name}")
        else:
            failures.append(f"{name}{': ' + detail if detail else ''}")
            print(f"  FAIL {name} {detail}")

    def render_fixture(d: dict) -> str:
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
            json.dump(d, fh)
            path = Path(fh.name)
        try:
            return render(path)
        finally:
            path.unlink(missing_ok=True)

    # 1. A clear win must be reported as separating.
    sep = render_fixture(
        _fixture(
            [5] * 30 + [0] * 15,
            [5] * 18 + [0] * 27,
            {
                "n": 45,
                "mean_diff": 0.267,
                "mean_diff_ci95": [0.133, 0.4],
                "mcnemar": {"b_better_only": 12, "c_baseline_only": 0, "exact_p": 0.000488},
                "separates_at_95": True,
            },
        )
    )
    check("separating result says so", "separates at 95%" in sep)
    check("separating result keeps precision", "4.9e-4" in sep, sep[:0])

    # 2. THE BRANCH THAT MATTERS: a gap that does not clear zero must say so
    #    plainly AND say what n would be needed, not go quiet.
    noisy_sem = [5 if i % 2 == 0 else 0 for i in range(45)]
    noisy_lex = [5 if i % 3 == 0 else 0 for i in range(45)]
    nosep = render_fixture(
        _fixture(
            noisy_sem,
            noisy_lex,
            {
                "n": 45,
                "mean_diff": 0.067,
                "mean_diff_ci95": [-0.09, 0.22],
                "mcnemar": {"b_better_only": 11, "c_baseline_only": 8, "exact_p": 0.6476},
                "separates_at_95": False,
            },
        )
    )
    check("non-separating result says so", "does not separate at 95%" in nosep)
    check(
        "non-separating result states the n it would need",
        "queries would be needed" in nosep,
        "the whole point of the branch",
    )
    check("non-separating result does not claim a win", "separates at 95%." not in nosep)

    # 3. A result file with no paired block must render nothing rather than
    #    inventing a comparison.
    bare = _fixture([5] * 30 + [0] * 15, [5] * 18 + [0] * 27, {})
    del bare["paired"]
    out = render_fixture(bare)
    check("no paired block -> no paired claim", "paired gap" not in out)
    check("no paired block still renders the headline", "recall@1" in out)

    # 4. The overlap warning must appear only when the intervals actually
    #    overlap — a guard that always fires is not checking anything.
    check("overlap warning fires when they overlap", "Do not read that difference" in sep)
    disjoint = _fixture(
        [5] * 40 + [0] * 5,
        [5] * 5 + [0] * 40,
        {
            "n": 45,
            "mean_diff": 0.778,
            "mean_diff_ci95": [0.65, 0.9],
            "mcnemar": {"b_better_only": 35, "c_baseline_only": 0, "exact_p": 1e-10},
            "separates_at_95": True,
        },
    )
    disjoint["strategies"]["semantic"]["recall@1_ci95"] = [0.75, 0.95]
    disjoint["strategies"]["lexical"]["recall@1_ci95"] = [0.05, 0.25]
    dj = render_fixture(disjoint)
    check("overlap warning silent when they do not overlap", "Do not read that difference" not in dj)

    # 4b. Both result-file shapes must resolve to the same headline comparison.
    #     The flat shape is what early files carry; the keyed shape is current.
    flat = _fixture(
        [5] * 30 + [0] * 15,
        [5] * 18 + [0] * 27,
        {
            "n": 45,
            "mean_diff": 0.267,
            "mean_diff_ci95": [0.133, 0.4],
            "mcnemar": {"b_better_only": 12, "c_baseline_only": 0, "exact_p": 0.000488},
            "separates_at_95": True,
        },
    )
    keyed = json.loads(json.dumps(flat))
    keyed["paired"] = {
        PRIMARY: flat["paired"],
        "semantic_vs_auto": {
            "n": 45,
            "mean_diff": 0.058,
            "mean_diff_ci95": [-0.031, 0.16],
            "mcnemar": {"b_better_only": 4, "c_baseline_only": 2, "exact_p": 0.688},
            "separates_at_95": False,
        },
    }
    # This assertion used to read "flat and keyed render the same output", which
    # was true only because the keyed shape's second comparison was never
    # rendered. That is the bug, encoded as a test. The real invariant is
    # narrower: the keyed shape must resolve the HEADLINE to the primary
    # comparison rather than to whichever key iterates first — while also
    # rendering the default comparison that a flat file cannot carry.
    flat_out, keyed_out = render_fixture(flat), render_fixture(keyed)
    headline_claim = "the paired gap is **+0.267**"
    check(
        "keyed shape resolves the headline to the primary comparison",
        headline_claim in keyed_out and headline_claim in flat_out,
        "must not pick the first key",
    )
    check(
        "keyed shape additionally publishes the default comparison",
        "paired gap vs default" in keyed_out,
    )
    check(
        "flat shape claims nothing about a default it never measured",
        "paired gap vs default" not in flat_out,
    )

    # 5. The headline sentence at the top of the README renders from the same
    #    result and must carry the same verdict — a top-of-file claim that
    #    disagrees with the block below it is worse than no claim.
    def head(d: dict) -> str:
        import tempfile

        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
            json.dump(d, fh)
            path = Path(fh.name)
        try:
            return render_headline(path)
        finally:
            path.unlink(missing_ok=True)

    sep_fx = _fixture(
        [5] * 30 + [0] * 15,
        [5] * 18 + [0] * 27,
        {
            "n": 45,
            "mean_diff": 0.267,
            "mean_diff_ci95": [0.133, 0.4],
            "mcnemar": {"b_better_only": 12, "c_baseline_only": 0, "exact_p": 0.000488},
            "separates_at_95": True,
        },
    )
    nosep_fx = _fixture(
        noisy_sem,
        noisy_lex,
        {
            "n": 45,
            "mean_diff": 0.067,
            "mean_diff_ci95": [-0.09, 0.22],
            "mcnemar": {"b_better_only": 11, "c_baseline_only": 8, "exact_p": 0.6476},
            "separates_at_95": False,
        },
    )
    check("headline reports separation", "it separates." in head(sep_fx))
    check("headline reports non-separation", "does not separate" in head(nosep_fx))
    check("headline names the n it would need", "would need roughly n=" in head(nosep_fx))
    check(
        "headline agrees with the block",
        ("it separates." in head(sep_fx)) == ("separates at 95%" in sep),
    )

    # 6. THE ROW THIS CHANGE EXISTS FOR: the comparison against the shipped
    #    default. On the committed result it does NOT separate, and a null that
    #    is rendered vaguely is worse than one rendered not at all — so the
    #    phrasing for the null is tested, not just the code path.
    auto_hits = [5 if i % 5 else 0 for i in range(45)]
    with_default = _fixture(
        [5] * 30 + [0] * 15,
        [5] * 18 + [0] * 27,
        {
            PRIMARY: {
                "n": 45,
                "mean_diff": 0.267,
                "mean_diff_ci95": [0.133, 0.4],
                "mcnemar": {"b_better_only": 12, "c_baseline_only": 0, "exact_p": 0.000488},
                "separates_at_95": True,
            },
            SECONDARY: {
                "n": 45,
                "mean_diff": 0.058,
                "mean_diff_ci95": [-0.031, 0.16],
                "mcnemar": {"b_better_only": 4, "c_baseline_only": 2, "exact_p": 0.6875},
                "separates_at_95": False,
            },
        },
        auto_hits=auto_hits,
    )
    wd = render_fixture(with_default)
    check("default comparison is rendered at all", "paired gap vs default" in wd)
    check(
        "default comparison names the default strategy plainly",
        "`auto` is the default" in wd and "resolves an unset `strategy`" in wd,
        "a reader must not have to infer which strategy they get",
    )
    check(
        "non-separating default SHOWS the interval crossing zero",
        "runs from -0.031 to 0.160" in wd and "contains 0" in wd,
        "shown, not merely described",
    )
    check(
        "non-separating default is labelled as not separating at 95%",
        "does not separate at 95%" in wd,
    )
    check(
        "non-separating default does not claim the sign is established",
        "sign of the difference is not established" in wd,
    )
    check(
        "the separating headline is not weakened by the null row",
        "the gap separates at 95%." in wd,
        "both verdicts must coexist without contaminating each other",
    )

    # 6b. The positive branch of the same row: if the default gap ever DOES
    #     separate, it must say so rather than reusing the null phrasing.
    sep_default = json.loads(json.dumps(with_default))
    sep_default["paired"][SECONDARY] = {
        "n": 45,
        "mean_diff": 0.21,
        "mean_diff_ci95": [0.09, 0.33],
        "mcnemar": {"b_better_only": 14, "c_baseline_only": 1, "exact_p": 0.001},
        "separates_at_95": True,
    }
    sd = render_fixture(sep_default)
    check("separating default says it separates", "That interval **separates at 95%**." in sd)
    check("separating default drops the crosses-zero language", "contains 0" not in sd)

    # 6c. A result with no default comparison must stay silent about one.
    check("no default comparison -> no default claim", "paired gap vs default" not in sep)

    # 7. The benchmark entry point must lead with the headline. Verified in the
    #    FAILING direction against the real pre-fix ordering, because a guard
    #    only ever exercised on a good page is not evidence of anything.
    import tempfile as _tf

    def idx_probe(rows: list[str]) -> list[str]:
        body = "## The table\n\n| Benchmark | Headline |\n|---|---|\n" + "\n".join(rows)
        with _tf.NamedTemporaryFile("w", suffix=".md", delete=False) as fh:
            fh.write(body)
            q = Path(fh.name)
        try:
            return check_bench_index(index_path=q)
        finally:
            q.unlink(missing_ok=True)

    good = ["| **HEADLINE** — MiniLM n=45 | recall@1 0.689 |", "| **Earlier** — nomic n=23 | 0.739 |"]
    bad = ["| **Semantic recall** — nomic n=23 | 0.739 |", "| **ONNX MiniLM** n=45 | recall@1 0.689 |"]
    check("bench index: headline-first page passes", idx_probe(good) == [])
    check(
        "bench index: headline-second page FAILS",
        idx_probe(bad) != [],
        "this is the exact ordering that shipped",
    )
    check(
        "bench index: the failure names the ordering, not a typo",
        any("must be row 1" in p for p in idx_probe(bad)),
    )

    if failures:
        print(f"\nself-test FAILED ({len(failures)}): " + "; ".join(failures), file=sys.stderr)
        return 1
    print("\nself-test passed")
    return 0


def check_bench_index(
    result_path: Path | None = None, index_path: Path | None = None
) -> list[str]:
    """The benchmark entry point must lead with the README's headline number.

    README.md calls docs/benchmarks/index.md "the single benchmark entry point"
    and says of itself: "There is deliberately only one headline, and an older
    measurement is kept below it under its own heading rather than beside it."
    That page did the opposite — its first row was an *earlier* nomic-embed-text
    measurement at recall@1 0.739 on n=23, with the n=45 headline of 0.689
    second. A reader following the README's own pointer met a higher number from
    a smaller sample first.

    Unlike the README block, this page is hand-written prose with per-row
    caveats, so it is not generated. This check is what stops the ordering
    silently inverting again: the first data row of the table must carry the
    headline recall@1 that the result file records.
    """
    problems: list[str] = []
    ip = index_path or BENCH_INDEX
    if not ip.exists():
        return [f"{ip} is missing"]
    d = json.loads((result_path or RESULT).read_text())
    headline = pct(d["strategies"]["semantic"]["recall@1"])

    rows = [ln for ln in ip.read_text().split("\n") if ln.startswith("| **")]
    if not rows:
        return [f"{ip.name}: no benchmark table rows found — has the table moved?"]
    if headline not in rows[0]:
        problems.append(
            f"{ip.name}: the first table row does not carry the headline "
            f"recall@1 {headline} from {RESULT.name}. README calls this page the "
            "single benchmark entry point and states there is only one headline, "
            "so the headline measurement must be row 1. Found instead: "
            f"{rows[0][:110]}..."
        )
    for i, r in enumerate(rows[1:], start=2):
        if headline in r and "arlier" not in r:
            problems.append(
                f"{ip.name}: the headline number {headline} also appears in row "
                f"{i} without being marked as an earlier measurement."
            )
    return problems


def main() -> int:
    mode = sys.argv[1] if len(sys.argv) > 1 else "--write"
    if mode == "--self-test":
        return self_test()
    block = render()
    if mode == "--print":
        print(block)
        return 0

    text = README.read_text()
    new = text
    for begin, end, body in (
        (BEGIN, END, block),
        (HEAD_BEGIN, HEAD_END, render_headline()),
    ):
        if begin not in new or end not in new:
            raise SystemExit(
                f"README.md is missing the markers {begin} / {end}; add them where "
                "the generated text should live."
            )
        new = re.compile(re.escape(begin) + r".*?" + re.escape(end), re.DOTALL).sub(
            lambda _, b=body: b, new
        )
    if mode == "--check":
        idx_problems = check_bench_index()
        for prob in idx_problems:
            print(prob, file=sys.stderr)
        if new != text:
            print(
                "README recall block(s) STALE vs bench/results/locomo_v1.json "
                "— run: python3 scripts/gen_recall_number.py",
                file=sys.stderr,
            )
            return 1
        if idx_problems:
            return 1
        print("README recall blocks are up to date; benchmark index leads with the headline.")
        return 0
    README.write_text(new)
    print("Rewrote README recall-number block.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
