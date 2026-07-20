#!/usr/bin/env python3
"""Offline learnings extraction for the STATE-Bench Agent Learning Track.

Reads the train trajectories for a domain and writes one deterministic
procedural learning per trajectory into an embedded mnemo DuckDB store, which
becomes the learnings artifact `retrieve_learnings` reads at inference time.

Credential-free (uses lexical/BM25 storage unless an embedding key is set); no
LLM. See ./README.md.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "agents"))
import mnemo_learnings  # noqa: E402  (no state_bench dependency — credential-free)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--train-dir", required=True, help="datasets/train_task_trajectories/<domain>/")
    ap.add_argument("--domain", required=True, choices=["travel", "customer_support", "shopping_assistant"])
    ap.add_argument("--db-path", default=None, help="mnemo DuckDB store to write (default: MNEMO_STATEBENCH_DB_DIR/<domain>.mnemo.db)")
    args = ap.parse_args()

    n = mnemo_learnings.build_learnings(args.train_dir, args.domain, args.db_path)
    print(f"built {n} learnings for domain={args.domain}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
