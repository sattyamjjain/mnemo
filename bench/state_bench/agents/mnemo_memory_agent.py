"""STATE-Bench Agent Learning Track adapter — mnemo as the memory backend.

The **only** integration code: a subclass of STATE-Bench's built-in
`StateBenchAgent` that exposes the read-only hook
`retrieve_learnings(query, top_k) -> list[str]`, backed by mnemo's **public
Python SDK** (`MnemoClient`). All mnemo logic lives in
[`mnemo_learnings.py`](mnemo_learnings.py) (which has no `state_bench` dependency
and is therefore smoke-testable offline). Nothing in `mnemo-core` is modified.

Place a copy of this file (and `mnemo_learnings.py`) under the STATE-Bench
checkout's repo-root `agents/` folder — `run_state_bench.sh` does this — so the
harness discovers `--agent-class MnemoMemoryAgent`.

Note: `from state_bench...` resolves only inside the STATE-Bench venv where the
agent actually runs; a bare static-check of this file will flag it, which is
expected. See ../README.md for the full write-up and honest framing.
"""

from __future__ import annotations

from pathlib import Path

from state_bench.agents.state_bench import StateBenchAgent

import mnemo_learnings


class MnemoMemoryAgent(StateBenchAgent):
    """Agent Learning Track agent that retrieves procedural learnings from mnemo."""

    def retrieve_learnings(self, query: str, top_k: int = 3) -> list[str]:
        return mnemo_learnings.retrieve(query, top_k)

    # Optional offline extraction hook (protocol: user-owned artifact).
    @staticmethod
    def build_learnings(train_dir: str | Path, domain: str, db_path: str | Path | None = None) -> int:
        return mnemo_learnings.build_learnings(train_dir, domain, db_path)
