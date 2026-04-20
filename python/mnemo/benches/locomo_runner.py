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
    """Messages to seed the store with. Each turn has `role` and `content`."""
    query: str
    """Natural-language recall query."""
    gold_answers: list[str]
    """List of substrings; recall@k is 1 if any appears in a top-k result."""


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


def load_dataset(name: str, split: str = "test") -> list[QueryExample]:
    """Load a benchmark dataset by short name.

    Args:
        name: ``"locomo"`` or ``"longmemeval"``.
        split: HuggingFace split name.

    Returns:
        Iterable of :class:`QueryExample`.

    Raises:
        NotImplementedError: when the optional ``mnemo[benchmark]`` extra
            is not installed. The real loader plugs in ``datasets`` and
            normalises the two benchmarks to the :class:`QueryExample`
            shape.
    """
    try:
        import datasets  # type: ignore[import-not-found]  # noqa: F401
    except ImportError as exc:
        raise NotImplementedError(
            f"load_dataset({name!r}): install `mnemo[benchmark]` to pull "
            f"the LoCoMo / LongMemEval datasets from HuggingFace."
        ) from exc
    # Delegated to per-dataset loaders once the deps land.
    raise NotImplementedError(
        f"load_dataset({name!r}): per-dataset loaders not yet wired."
    )


def seed_store(
    client: Any,
    examples: Iterable[QueryExample],
) -> dict[str, str]:
    """Write each turn into Mnemo tagged with its `conversation_id`.

    Returns a map of conversation_id → thread_id used for per-session
    recall filtering.
    """
    thread_ids: dict[str, str] = {}
    for example in examples:
        thread_id = example.conversation_id
        thread_ids[example.conversation_id] = thread_id
        for turn_idx, turn in enumerate(example.turns):
            client.remember(
                content=turn["content"],
                memory_type="episodic",
                tags=[f"conv:{example.conversation_id}"],
                thread_id=thread_id,
                metadata={
                    "role": turn.get("role", "user"),
                    "turn_index": turn_idx,
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

    For each query:
    * Calls `client.recall(query, strategy=..., limit=10)`
    * Records latency in ms
    * Checks whether any `gold_answer` substring appears in the top-5 /
      top-10 results (recall@k) and computes the reciprocal rank (MRR).
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
        contents = [m.get("content", "") for m in memories]

        def _hit(k: int) -> bool:
            return any(
                any(g.lower() in c.lower() for g in example.gold_answers)
                for c in contents[:k]
            )

        if _hit(5):
            hits_5 += 1
        if _hit(10):
            hits_10 += 1

        for rank, content in enumerate(contents, start=1):
            if any(g.lower() in content.lower() for g in example.gold_answers):
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
    parser.add_argument("--split", default="test")
    parser.add_argument("--db-path", default="benchmark.mnemo.db")
    parser.add_argument("--agent-id", default="benchmark")
    parser.add_argument("--limit", type=int, default=10)
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

    examples = list(load_dataset(args.dataset, split=args.split))
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
