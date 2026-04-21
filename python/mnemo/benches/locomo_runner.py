"""LoCoMo / LongMemEval benchmark runner for Mnemo.

This module provides a runnable harness that:

1. Loads a benchmark dataset (LoCoMo or LongMemEval; both published as
   HuggingFace datasets with compatible shapes).
2. Seeds the Mnemo store with the dataset's conversations.
3. Runs the recall queries under multiple strategies
   (``auto`` / ``vector_only`` / ``hybrid_rrf`` / ``graph_boosted``).
4. Reports `recall@5`, `recall@10`, `MRR`, and p50/p95/p99 latency to
   stdout and to a Markdown file under ``docs/benchmarks/YYYY-MM-DD.md``.

The first published numbers land with v0.3.0. The target is "≥65% recall
at <250 ms p95", competitive with Mem0's reported 66.9% on LongMemEval.
Missing the target is explicitly not a blocker — we ship whatever we
measure and iterate.

Dataset integration is stubbed behind :func:`load_dataset` because the
HuggingFace loader is an optional extra (``mnemo[benchmark]``) and the
on-disk format of the two benchmarks differs slightly. Real loaders go
in ``_locomo.py`` / ``_longmemeval.py`` modules once those deps land.
"""

from __future__ import annotations

import argparse
import json
import statistics
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

Strategy = str  # "auto" | "vector_only" | "hybrid_rrf" | "graph_boosted"


@dataclass
class QueryExample:
    """One row from a LoCoMo/LongMemEval evaluation set."""

    conversation_id: str
    """Stable identifier tying facts to queries for a single conversation."""
    turns: list[dict[str, str]]
    """Messages to seed the store with. Each turn has `role`, `content`,
    and optionally `session_id` so session-level scoring can run."""
    query: str
    """Natural-language recall query."""
    gold_answers: list[str]
    """Free-text gold-answer substrings; retained for coarse scoring."""
    gold_session_ids: list[str] = None  # type: ignore[assignment]
    """Optional list of session ids that contain the true answer. When
    present, session-level recall@k is 1 if any retrieved memory carries a
    matching `session_id` metadata tag. This is the LongMemEval-native
    scoring path; LoCoMo-MC10 does not expose this field."""

    def __post_init__(self) -> None:
        if self.gold_session_ids is None:
            self.gold_session_ids = []


@dataclass
class StrategyResult:
    strategy: Strategy
    recall_at_5: float
    recall_at_10: float
    mrr: float
    latencies_ms: list[float]

    def p(self, q: float) -> float:
        if not self.latencies_ms:
            return 0.0
        sorted_lat = sorted(self.latencies_ms)
        idx = max(0, min(len(sorted_lat) - 1, int(q * len(sorted_lat))))
        return sorted_lat[idx]

    def as_markdown_row(self) -> str:
        return (
            f"| `{self.strategy}` | {self.recall_at_5:.3f} | "
            f"{self.recall_at_10:.3f} | {self.mrr:.3f} | "
            f"{self.p(0.50):.1f} | {self.p(0.95):.1f} | {self.p(0.99):.1f} |"
        )


# ---------------------------------------------------------------------------
# Dataset loaders — LoCoMo-MC10 and LongMemEval-cleaned via public
# HuggingFace mirrors. Both normalise to the `QueryExample` shape.
#
# Licences:
#   * LoCoMo-MC10 (`Percena/locomo-mc10`) — CC BY-NC 4.0, attribution required.
#   * LongMemEval (`xiaowu0162/longmemeval-cleaned`) — see upstream card.
# ---------------------------------------------------------------------------

_LOCOMO_HF = ("Percena/locomo-mc10", "train")
_LONGMEMEVAL_HF = ("xiaowu0162/longmemeval-cleaned", "longmemeval_s_cleaned")


def _coerce(value: Any) -> Any:
    """Parse HF string-serialised Python literals (list/dict) if needed."""
    import ast

    if isinstance(value, (list, dict)):
        return value
    if isinstance(value, str) and value and value[0] in "[{(":
        try:
            return ast.literal_eval(value)
        except (SyntaxError, ValueError):
            return value
    return value


def _sanitize_utf8(value: str) -> str:
    """Strip unpaired surrogates + other codecs-invalid bytes.

    LongMemEval and LoCoMo both carry occasional surrogate-pair garbage
    from chat logs. PyO3 rejects any Python str that can't be encoded as
    UTF-8, so we round-trip with `errors="replace"` before sending.
    """
    return value.encode("utf-8", errors="replace").decode("utf-8")


def _flatten_sessions(
    sessions: Any,
    session_ids: Any = None,
) -> list[dict[str, str]]:
    """Flatten HF `haystack_sessions` (list-of-session-lists) into a flat
    [{role, content, session_id?}, ...] stream. When `session_ids` aligns
    with `sessions` by index, every turn gets its session id as metadata so
    session-level recall scoring can work later.
    """
    parsed = _coerce(sessions)
    id_list = _coerce(session_ids) if session_ids is not None else None
    out: list[dict[str, str]] = []
    if not isinstance(parsed, list):
        return out
    for s_idx, session in enumerate(parsed):
        session = _coerce(session)
        if not isinstance(session, list):
            continue
        sid = None
        if isinstance(id_list, list) and s_idx < len(id_list):
            sid = _sanitize_utf8(str(id_list[s_idx]))
        for turn in session:
            turn = _coerce(turn)
            if isinstance(turn, dict) and "content" in turn:
                record: dict[str, str] = {
                    "role": _sanitize_utf8(str(turn.get("role", "user"))),
                    "content": _sanitize_utf8(str(turn.get("content", ""))),
                }
                if sid is not None:
                    record["session_id"] = sid
                out.append(record)
    return out


def _row_to_locomo_example(row: dict[str, Any]) -> QueryExample:
    choices = _coerce(row.get("choices")) or []
    correct_idx_raw = row.get("correct_choice_index")
    try:
        correct_idx = int(correct_idx_raw) if correct_idx_raw is not None else None
    except (TypeError, ValueError):
        correct_idx = None
    gold = [str(row.get("answer", ""))]
    if correct_idx is not None and 0 <= correct_idx < len(choices):
        gold.append(str(choices[correct_idx]))
    gold = [_sanitize_utf8(g.strip().strip('"').strip("'")) for g in gold if g and g.strip()]
    return QueryExample(
        conversation_id=_sanitize_utf8(str(row.get("question_id", ""))),
        turns=_flatten_sessions(row.get("haystack_sessions")),
        query=_sanitize_utf8(str(row.get("question", ""))),
        gold_answers=gold,
    )


def _row_to_longmemeval_example(row: dict[str, Any]) -> QueryExample:
    ans = _sanitize_utf8(str(row.get("answer", "")).strip().strip('"').strip("'"))
    gold_sessions = _coerce(row.get("answer_session_ids")) or []
    if not isinstance(gold_sessions, list):
        gold_sessions = []
    gold_sessions = [_sanitize_utf8(str(s)) for s in gold_sessions]
    return QueryExample(
        conversation_id=_sanitize_utf8(str(row.get("question_id", ""))),
        turns=_flatten_sessions(
            row.get("haystack_sessions"),
            session_ids=row.get("haystack_session_ids"),
        ),
        query=_sanitize_utf8(str(row.get("question", ""))),
        gold_answers=[ans] if ans else [],
        gold_session_ids=gold_sessions,
    )


def load_dataset(
    name: str,
    split: str | None = None,
    limit: int | None = None,
    hf_name: str | None = None,
) -> list[QueryExample]:
    """Load a benchmark dataset by short name (`"locomo"` | `"longmemeval"`).

    Args:
        name: short name; `hf_name` overrides the HF repo id.
        split: HuggingFace split; defaults per benchmark.
        limit: cap number of examples (smoke runs / CI budget).
        hf_name: override HF repo id for local forks.

    Requires the optional ``mnemo[benchmark]`` extra. The loader pulls from
    HuggingFace unauthenticated by default — set ``HF_TOKEN`` to raise rate
    limits.
    """
    try:
        from datasets import load_dataset as _hf_load  # type: ignore[import-not-found]
    except ImportError as exc:
        raise NotImplementedError(
            f"load_dataset({name!r}): install `mnemo[benchmark]` to pull "
            f"LoCoMo / LongMemEval from HuggingFace."
        ) from exc

    short = name.lower()
    if short in ("locomo", "locomo-mc10"):
        repo = hf_name or _LOCOMO_HF[0]
        resolved_split = split or _LOCOMO_HF[1]
        to_example = _row_to_locomo_example
    elif short in ("longmemeval", "long-mem-eval"):
        repo = hf_name or _LONGMEMEVAL_HF[0]
        resolved_split = split or _LONGMEMEVAL_HF[1]
        to_example = _row_to_longmemeval_example
    else:
        raise ValueError(f"unknown benchmark: {name!r}")

    ds = _hf_load(repo, split=resolved_split, streaming=True)
    out: list[QueryExample] = []
    for row in ds:
        example = to_example(row)
        if not example.turns or not example.query or not example.gold_answers:
            continue
        out.append(example)
        if limit is not None and len(out) >= limit:
            break
    return out


def seed_store(
    client: Any,
    examples: Iterable[QueryExample],
) -> dict[str, str]:
    """Write each turn into Mnemo tagged with its `conversation_id` and,
    when present, its `session_id`.

    Returns a map of conversation_id → thread_id used for per-session
    recall filtering.
    """
    thread_ids: dict[str, str] = {}
    for example in examples:
        thread_id = example.conversation_id
        thread_ids[example.conversation_id] = thread_id
        for turn_idx, turn in enumerate(example.turns):
            session_id = turn.get("session_id")
            tags = [f"conv:{example.conversation_id}"]
            if session_id:
                tags.append(f"session:{session_id}")
            client.remember(
                content=turn["content"],
                memory_type="episodic",
                tags=tags,
                thread_id=thread_id,
                metadata={
                    "role": turn.get("role", "user"),
                    "turn_index": turn_idx,
                    "session_id": session_id,
                },
            )
    return thread_ids


def evaluate_strategy(
    client: Any,
    examples: list[QueryExample],
    strategy: Strategy,
    limit: int = 10,
) -> StrategyResult:
    """Run every query in ``examples`` under a single strategy.

    Scoring is session-level when ``gold_session_ids`` is populated
    (LongMemEval exposes the source-of-truth session ids directly); we
    score a hit when any retrieved memory carries a ``session:{id}`` tag
    matching a gold id. Otherwise we fall back to case-insensitive
    substring match against ``gold_answers`` — works for LoCoMo-MC10's
    verbatim-date/verbatim-choice answers but fails for LongMemEval's
    inferential answers ("MBA" → "Business Administration"). Mixing
    both scorers is tracked as the v0.3.2 LLM-as-judge upgrade.
    """
    hits_5 = 0
    hits_10 = 0
    rr_sum = 0.0
    latencies: list[float] = []

    for example in examples:
        t0 = time.perf_counter()
        result = client.recall(
            query=example.query,
            limit=limit,
            strategy=strategy,
            tags=[f"conv:{example.conversation_id}"],
        )
        latencies.append((time.perf_counter() - t0) * 1000.0)
        memories = result.get("memories", []) if isinstance(result, dict) else []

        use_session_scoring = bool(example.gold_session_ids)

        def _memory_hit(mem: dict[str, Any]) -> bool:
            if use_session_scoring:
                mem_tags = mem.get("tags", []) or []
                needed = {f"session:{s}" for s in example.gold_session_ids}
                return any(t in needed for t in mem_tags)
            content = str(mem.get("content", ""))
            return any(g.lower() in content.lower() for g in example.gold_answers)

        hit_flags = [_memory_hit(m) for m in memories]

        if any(hit_flags[:5]):
            hits_5 += 1
        if any(hit_flags[:10]):
            hits_10 += 1
        for rank, flag in enumerate(hit_flags, start=1):
            if flag:
                rr_sum += 1.0 / rank
                break

    n = max(len(examples), 1)
    return StrategyResult(
        strategy=strategy,
        recall_at_5=hits_5 / n,
        recall_at_10=hits_10 / n,
        mrr=rr_sum / n,
        latencies_ms=latencies,
    )


def format_report(
    dataset: str,
    split: str,
    commit: str,
    results: list[StrategyResult],
) -> str:
    lines = [
        f"# Mnemo benchmark — {dataset} ({split})",
        "",
        f"* Commit: `{commit}`",
        f"* Examples: {len(results[0].latencies_ms) if results else 0}",
        "",
        "| strategy | recall@5 | recall@10 | MRR | p50 ms | p95 ms | p99 ms |",
        "|---|---:|---:|---:|---:|---:|---:|",
    ]
    lines.extend(r.as_markdown_row() for r in results)
    lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset", choices=["locomo", "longmemeval"], required=True)
    parser.add_argument("--split", default=None, help="HF split; defaults per benchmark.")
    parser.add_argument("--db-path", default="benchmark.mnemo.db")
    parser.add_argument("--agent-id", default="benchmark")
    parser.add_argument("--limit", type=int, default=10, help="recall limit (top-k).")
    parser.add_argument(
        "--max-examples",
        type=int,
        default=None,
        help="Cap the number of dataset examples run. Useful for smoke runs.",
    )
    parser.add_argument(
        "--strategies",
        nargs="+",
        default=["auto", "vector_only", "hybrid_rrf", "graph_boosted"],
    )
    parser.add_argument(
        "--report-dir",
        default="docs/benchmarks",
        help="Markdown report output directory.",
    )
    args = parser.parse_args(argv)

    try:
        from mnemo._mnemo import MnemoClient  # type: ignore[attr-defined]
    except ImportError as exc:  # pragma: no cover
        raise SystemExit(
            "mnemo native extension not built. Run `maturin develop` in python/."
        ) from exc

    examples = load_dataset(args.dataset, split=args.split, limit=args.max_examples)
    client = MnemoClient(db_path=args.db_path, agent_id=args.agent_id)
    seed_store(client, examples)

    results = [
        evaluate_strategy(client, examples, s, limit=args.limit)
        for s in args.strategies
    ]
    report = format_report(
        dataset=args.dataset,
        split=args.split,
        commit=_git_short_sha(),
        results=results,
    )
    out_dir = Path(args.report_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f"{_today()}-{args.dataset}.md"
    out_path.write_text(report, encoding="utf-8")

    print(report)
    print(f"\nReport written to {out_path}")

    # Emit a machine-readable sidecar for CI charting.
    sidecar = out_dir / f"{_today()}-{args.dataset}.json"
    sidecar.write_text(
        json.dumps(
            {
                "dataset": args.dataset,
                "split": args.split,
                "commit": _git_short_sha(),
                "results": [
                    {
                        "strategy": r.strategy,
                        "recall_at_5": r.recall_at_5,
                        "recall_at_10": r.recall_at_10,
                        "mrr": r.mrr,
                        "p50_ms": r.p(0.50),
                        "p95_ms": r.p(0.95),
                        "p99_ms": r.p(0.99),
                        "n": len(r.latencies_ms),
                    }
                    for r in results
                ],
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    return 0


def _today() -> str:
    from datetime import date

    return date.today().isoformat()


def _git_short_sha() -> str:
    import subprocess

    try:
        return subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"],
            text=True,
        ).strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return "unknown"


def _ensure_statistics_module_loaded() -> None:  # pragma: no cover
    """Reference `statistics` at import time so lint stays consistent."""
    _ = statistics.median


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
