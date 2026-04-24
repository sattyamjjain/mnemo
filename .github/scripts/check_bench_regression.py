#!/usr/bin/env python3
"""Compare the latest benchmark run to baseline.json and fail on regression.

Invoked by `.github/workflows/benchmarks-nightly.yml` after the runner
produces a sidecar `docs/benchmarks/YYYY-MM-DD-<dataset>.json`.

Policy:
  * If `baseline.json` has no stored results for this dataset yet, this
    run becomes the first baseline — succeeds with a log line, and the
    workflow commits the updated baseline on main.
  * Otherwise, for each `strategy` present in the latest run, compare
    `recall@10` against the matching strategy in the baseline. If the
    latest value is more than `--max-regression-pp` percentage points
    below the baseline value, exit non-zero.
  * Strategies present in the baseline but missing from the latest run
    are treated as a regression (they were dropped).
  * Strategies present in the latest run but not in the baseline are
    ignored (additive = fine).
"""

from __future__ import annotations

import argparse
import glob
import json
import sys
from pathlib import Path


def _find_latest_run(run_dir: Path, dataset: str) -> Path | None:
    pattern = str(run_dir / f"*-{dataset}.json")
    matches = sorted(glob.glob(pattern))
    return Path(matches[-1]) if matches else None


def _extract_recall_map(
    result_payload: dict[str, object],
) -> dict[str, float]:
    results = result_payload.get("results", [])
    out: dict[str, float] = {}
    if not isinstance(results, list):
        return out
    for r in results:
        if not isinstance(r, dict):
            continue
        strat = r.get("strategy")
        recall = r.get("recall_at_10")
        if isinstance(strat, str) and isinstance(recall, (int, float)):
            out[strat] = float(recall)
    return out


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--dataset", required=True, choices=["locomo", "longmemeval"])
    p.add_argument("--baseline", required=True, type=Path)
    p.add_argument("--run-dir", required=True, type=Path)
    p.add_argument(
        "--max-regression-pp",
        type=float,
        default=3.0,
        help="Max allowed drop in recall@10 (percentage points).",
    )
    args = p.parse_args()

    run_path = _find_latest_run(args.run_dir, args.dataset)
    if run_path is None:
        print(
            f"no run sidecar for dataset={args.dataset} under {args.run_dir}; "
            f"did the runner fail? exiting non-zero",
            file=sys.stderr,
        )
        return 2

    run_payload = json.loads(run_path.read_text(encoding="utf-8"))
    baseline_payload = json.loads(args.baseline.read_text(encoding="utf-8"))

    ds_block = baseline_payload.get("datasets", {}).get(args.dataset, {})
    baseline_results = ds_block.get("results", [])

    if not baseline_results:
        print(
            f"no baseline stored yet for dataset={args.dataset}; "
            f"this run becomes the baseline (first-run exception)."
        )
        print(f"run file: {run_path}")
        return 0

    run_map = _extract_recall_map(run_payload)
    baseline_map = _extract_recall_map({"results": baseline_results})

    failed: list[str] = []
    for strat, base_recall in baseline_map.items():
        if strat not in run_map:
            failed.append(f"  * {strat}: DROPPED from run (baseline={base_recall:.3f})")
            continue
        new_recall = run_map[strat]
        drop_pp = (base_recall - new_recall) * 100.0
        if drop_pp > args.max_regression_pp:
            failed.append(
                f"  * {strat}: recall@10 {base_recall:.3f} -> {new_recall:.3f} "
                f"(Δ -{drop_pp:.2f}pp > {args.max_regression_pp}pp gate)"
            )
        else:
            print(
                f"  ok {strat}: {base_recall:.3f} -> {new_recall:.3f} "
                f"(Δ {(new_recall - base_recall) * 100.0:+.2f}pp)"
            )

    if failed:
        print(
            f"::error title=Benchmark regression::{args.dataset} regressed vs baseline",
            file=sys.stderr,
        )
        print("\n".join(failed), file=sys.stderr)
        return 1

    print(f"no regressions detected for dataset={args.dataset}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
