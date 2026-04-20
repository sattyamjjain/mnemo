"""Smoke tests for the LoCoMo / LongMemEval benchmark harness.

These tests do not pull the real datasets — they exercise the harness
plumbing against an in-memory fake client so the scoring, percentile,
and report formatting paths stay trustworthy.
"""

from __future__ import annotations

from mnemo.benches.locomo_runner import (
    QueryExample,
    StrategyResult,
    evaluate_strategy,
    format_report,
    seed_store,
)


class _FakeClient:
    """Minimal stand-in for `mnemo._mnemo.MnemoClient` — records seeds
    and serves recall results from in-memory content matching.
    """

    def __init__(self, fixed_results: dict[str, list[str]]) -> None:
        self._results = fixed_results
        self.seeded: list[dict] = []

    def remember(self, **kwargs):
        self.seeded.append(kwargs)
        return {"id": f"mem-{len(self.seeded)}"}

    def recall(self, *, query, limit=10, strategy=None, tags=None, **_):
        memories = [
            {"content": c, "id": f"r-{i}"}
            for i, c in enumerate(self._results.get(query, []))
        ]
        return {"memories": memories[:limit], "total": len(memories)}


def _examples() -> list[QueryExample]:
    return [
        QueryExample(
            conversation_id="conv-a",
            turns=[{"role": "user", "content": "I prefer dark mode"}],
            query="what does the user prefer?",
            gold_answers=["dark mode"],
        ),
        QueryExample(
            conversation_id="conv-b",
            turns=[{"role": "user", "content": "My favourite language is Rust"}],
            query="favourite language?",
            gold_answers=["rust"],
        ),
    ]


def test_seed_store_tags_conversations() -> None:
    client = _FakeClient(fixed_results={})
    seed_store(client, _examples())
    assert len(client.seeded) == 2
    assert any("conv:conv-a" in (s["tags"] or []) for s in client.seeded)


def test_evaluate_strategy_counts_hits() -> None:
    client = _FakeClient(
        fixed_results={
            "what does the user prefer?": [
                "noise",
                "the user prefers dark mode for the UI",
            ],
            "favourite language?": ["their favourite language is Rust"],
        }
    )
    result = evaluate_strategy(client, _examples(), strategy="auto", limit=10)
    assert result.strategy == "auto"
    assert result.recall_at_5 == 1.0
    assert result.recall_at_10 == 1.0
    assert result.mrr > 0.0


def test_evaluate_strategy_missing_gold_is_zero() -> None:
    client = _FakeClient(
        fixed_results={
            "what does the user prefer?": ["nothing useful", "other stuff"],
            "favourite language?": ["unrelated"],
        }
    )
    result = evaluate_strategy(client, _examples(), strategy="vector_only", limit=10)
    assert result.recall_at_5 == 0.0
    assert result.mrr == 0.0


def test_strategy_result_percentiles() -> None:
    r = StrategyResult(
        strategy="auto",
        recall_at_5=0.5,
        recall_at_10=0.5,
        mrr=0.5,
        latencies_ms=[1.0, 2.0, 3.0, 4.0, 100.0],
    )
    assert r.p(0.50) in (2.0, 3.0)
    assert r.p(0.95) == 100.0


def test_format_report_has_header_and_rows() -> None:
    results = [
        StrategyResult("auto", 0.66, 0.72, 0.55, [50.0, 60.0, 120.0]),
        StrategyResult("vector_only", 0.61, 0.68, 0.50, [40.0, 55.0, 110.0]),
    ]
    md = format_report("locomo", "test", "abc1234", results)
    assert "recall@5" in md
    assert "| `auto` |" in md
    assert "| `vector_only` |" in md
