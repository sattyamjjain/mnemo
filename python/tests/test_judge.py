"""Unit tests for the v0.3.3 LLM-as-judge scorer.

These tests do NOT hit the Anthropic API. A fake `messages_create` is
injected so we can exercise the parsing contract, the UNSURE fallback,
and the no-memories early-exit without network or credentials.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

import pytest

from mnemo.benches.judge import JudgeUnavailableError, JudgeVerdict, LlmJudge


@dataclass
class _Block:
    text: str


@dataclass
class _FakeResponse:
    content: list[_Block]


class _FakeClient:
    """Records the last request and replays a scripted response text."""

    def __init__(self, text: str) -> None:
        self.text = text
        self.last_model: str | None = None
        self.last_system: str | None = None
        self.last_messages: list[dict[str, Any]] | None = None

    def messages_create(
        self,
        *,
        model: str,
        max_tokens: int,
        system: str,
        messages: list[dict[str, Any]],
    ) -> _FakeResponse:
        self.last_model = model
        self.last_system = system
        self.last_messages = messages
        assert max_tokens > 0
        return _FakeResponse(content=[_Block(text=self.text)])


class _ExplodingClient:
    def messages_create(self, **_kwargs: Any) -> Any:
        raise TimeoutError("fake network timeout")


def _mem(content: str, score: float = 0.9) -> dict[str, Any]:
    return {"content": content, "score": score}


def test_yes_verdict_grades_correct() -> None:
    judge = LlmJudge(client=_FakeClient("YES\nThe MBA memory supports it."))
    v = judge.grade(
        question="What degree did Priya earn?",
        gold="Master of Business Administration",
        memories=[_mem("Priya earned her MBA from Wharton in 2019.")],
    )
    assert v.correct is True
    assert v.raw == "YES"
    assert "MBA" in v.rationale or "support" in v.rationale.lower()


def test_no_verdict_grades_incorrect() -> None:
    judge = LlmJudge(client=_FakeClient("NO\nNothing mentions the degree."))
    v = judge.grade(
        question="What degree did Priya earn?",
        gold="Master of Business Administration",
        memories=[_mem("Priya joined the platform team on 2026-05-02.")],
    )
    assert v.correct is False
    assert v.raw == "NO"


def test_unsure_scored_as_miss() -> None:
    judge = LlmJudge(client=_FakeClient("UNSURE\nAmbiguous memory."))
    v = judge.grade(
        question="q",
        gold="g",
        memories=[_mem("ambiguous content")],
    )
    assert v.correct is False
    assert v.raw == "UNSURE"


def test_unparseable_first_line_falls_back_to_unsure() -> None:
    judge = LlmJudge(client=_FakeClient("The answer is plausibly yes maybe."))
    v = judge.grade(question="q", gold="g", memories=[_mem("m")])
    assert v.correct is False
    assert v.raw == "UNSURE"


def test_bullet_prefix_tolerated() -> None:
    # Claude occasionally prepends "- " even when told not to.
    judge = LlmJudge(client=_FakeClient("- YES\nGood support."))
    v = judge.grade(question="q", gold="g", memories=[_mem("m")])
    assert v.correct is True
    assert v.raw == "YES"


def test_no_memories_short_circuits() -> None:
    client = _FakeClient("YES\n")
    judge = LlmJudge(client=client)
    v = judge.grade(question="q", gold="g", memories=[])
    assert v.correct is False
    assert v.raw == "NO"
    # The fake client must NOT have been called.
    assert client.last_model is None


def test_sdk_failure_surfaces_as_unavailable() -> None:
    judge = LlmJudge(client=_ExplodingClient())
    with pytest.raises(JudgeUnavailableError, match="TimeoutError"):
        judge.grade(question="q", gold="g", memories=[_mem("m")])


def test_prompt_shape_includes_gold_and_memories() -> None:
    client = _FakeClient("YES\nok")
    judge = LlmJudge(client=client)
    judge.grade(
        question="What's the capital of France?",
        gold="Paris",
        memories=[_mem("France's capital is Paris.", score=0.91), _mem("Unrelated.")],
    )
    assert client.last_messages is not None
    prompt = client.last_messages[0]["content"]
    assert "Question:" in prompt
    assert "Gold answer: Paris" in prompt
    assert "France's capital" in prompt
    assert "#1" in prompt and "#2" in prompt


def test_long_memory_content_is_truncated() -> None:
    client = _FakeClient("YES\nok")
    judge = LlmJudge(client=client, content_truncate=32)
    giant = "x" * 5000
    judge.grade(question="q", gold="g", memories=[_mem(giant)])
    assert client.last_messages is not None
    prompt = client.last_messages[0]["content"]
    # 32 x's + ellipsis is the expected truncation footprint.
    assert "x" * 32 in prompt
    assert "x" * 5000 not in prompt


def test_default_model_resolves_from_env(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("MNEMO_JUDGE_MODEL", "claude-some-experimental-build")
    client = _FakeClient("YES\nok")
    judge = LlmJudge(client=client)
    judge.grade(question="q", gold="g", memories=[_mem("m")])
    assert client.last_model == "claude-some-experimental-build"


def test_verdict_dataclass_is_frozen() -> None:
    v = JudgeVerdict(correct=True, raw="YES", rationale="ok")
    with pytest.raises(Exception):
        v.correct = False  # type: ignore[misc]
