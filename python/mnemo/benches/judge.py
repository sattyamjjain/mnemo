"""LLM-as-judge scoring for LongMemEval-style benchmarks (v0.3.3 Task B).

The v0.3.2 LoCoMo runner scores recall by substring-matching each retrieved
memory against gold answers. That works for verbatim answers
("$42M", "2026-05-01") but fails on inferential gold answers: LongMemEval's
ground truth for a question like "What degree did Priya earn?" is
"Master of Business Administration", while the retrieved memory might
only say "MBA from Wharton". The retrieval succeeded, but the substring
check records a miss — the runner undercounts.

This module introduces an LLM-as-judge alternative that asks a small,
cheap model whether the retrieved memories support the gold answer.

## Defaults and fallback

* **Model:** `claude-haiku-4-5-20251001` via the official Anthropic
  Python SDK. Override via `MNEMO_JUDGE_MODEL` or the `model=` kwarg.
* **API key:** `ANTHROPIC_API_KEY`. No key → `LlmJudge` raises
  `JudgeUnavailableError`; callers are expected to fall back to the
  legacy exact-match scorer.
* **Timeout:** 30 s per grade. Network failures propagate
  `JudgeUnavailableError` so the caller can retry or fall back.

## Design notes

* The judge receives the original question, the gold answer, and up to
  the first 10 retrieved memories (truncated to 600 chars each) plus
  their retrieval score. No system prompt — Anthropic's SDK attaches a
  minimal instruction via `system=`.
* Judge output is constrained to the literal string `YES`, `NO`, or
  `UNSURE` as the first line; anything else is treated as `UNSURE` and
  logged. This matches the Hindsight / Mem0 "exact-match-like"
  grading contract and avoids the soft-acceptance drift that plagues
  free-form LLM judges.
* `UNSURE` is scored as a miss (conservative). When we publish we note
  the `UNSURE` rate alongside the recall; a high rate means the judge
  is being asked to make calls it can't, not that retrieval failed.
"""

from __future__ import annotations

import logging
import os
from dataclasses import dataclass
from typing import Any, Iterable, Protocol

logger = logging.getLogger(__name__)

_DEFAULT_MODEL_ENV = "MNEMO_JUDGE_MODEL"
_DEFAULT_MODEL = "claude-haiku-4-5-20251001"
_DEFAULT_MAX_MEMORIES = 10
_DEFAULT_CONTENT_TRUNCATE = 600
_JUDGE_TIMEOUT_S = 30.0

_SYSTEM = (
    "You are a strict grader for an information-retrieval benchmark. "
    "You will be given a question, the single canonical gold answer, and "
    "a ranked list of retrieved memories. Answer on the first line with "
    "exactly one of the tokens YES, NO, or UNSURE. Then, on the next line, "
    "give a one-sentence rationale. "
    "YES means at least one retrieved memory states or directly supports "
    "the gold answer (paraphrase and synonymy are allowed). "
    "NO means none of the memories support the gold answer. "
    "UNSURE means you cannot tell from the memories provided."
)


class JudgeUnavailableError(RuntimeError):
    """Raised when the judge cannot be constructed or invoked.

    Callers should catch this and fall back to the legacy exact-match
    scorer. We do NOT silently degrade inside the judge — a missing
    API key or a timed-out call needs to be visible to the operator.
    """


@dataclass(frozen=True)
class JudgeVerdict:
    correct: bool
    """True iff the judge returned YES."""
    raw: str
    """The literal first-line token the model returned (`YES`/`NO`/`UNSURE`
    after normalisation). Preserved so benchmark reports can surface the
    `UNSURE` rate separately."""
    rationale: str
    """One-sentence explanation the model produced, if any."""


class _AnthropicLike(Protocol):
    """Minimal surface of `anthropic.Anthropic()` we use.

    Typed as a protocol so tests can inject a fake without depending on
    the real Anthropic SDK types — the SDK's class hierarchy moves
    across versions.
    """

    def messages_create(
        self,
        *,
        model: str,
        max_tokens: int,
        system: str,
        messages: list[dict[str, Any]],
    ) -> Any: ...


class LlmJudge:
    """Wraps a small LLM to grade a recall response against gold.

    Construct with the defaults or inject a custom `client` for testing.
    `grade()` returns a `JudgeVerdict`; wire failures surface as
    `JudgeUnavailableError` so benchmark harnesses can fall back to the
    legacy substring scorer.
    """

    def __init__(
        self,
        model: str | None = None,
        api_key: str | None = None,
        client: _AnthropicLike | None = None,
        max_memories: int = _DEFAULT_MAX_MEMORIES,
        content_truncate: int = _DEFAULT_CONTENT_TRUNCATE,
    ) -> None:
        self.model = model or os.environ.get(_DEFAULT_MODEL_ENV) or _DEFAULT_MODEL
        self.max_memories = max_memories
        self.content_truncate = content_truncate
        if client is not None:
            self._client = client
            return
        try:
            import anthropic  # type: ignore[import-not-found]
        except ImportError as exc:  # pragma: no cover
            raise JudgeUnavailableError(
                "LlmJudge: install `anthropic>=0.40` to use the LLM judge "
                "(or fall back to --judge=exact)"
            ) from exc
        key = api_key or os.environ.get("ANTHROPIC_API_KEY")
        if not key:
            raise JudgeUnavailableError(
                "LlmJudge: set ANTHROPIC_API_KEY or pass api_key= "
                "(or fall back to --judge=exact)"
            )
        self._client = _AnthropicBridge(
            anthropic.Anthropic(api_key=key, timeout=_JUDGE_TIMEOUT_S)
        )

    def grade(
        self,
        question: str,
        gold: str,
        memories: Iterable[dict[str, Any]],
    ) -> JudgeVerdict:
        """Grade one (question, gold, retrieved memories) triple.

        `memories` is the iterable returned by `MnemoClient.recall`; we
        only read `content` and `score`. If fewer than one memory is
        retrieved we skip the LLM call and return NO to keep costs
        bounded.
        """
        mem_list = list(memories)[: self.max_memories]
        if not mem_list:
            return JudgeVerdict(correct=False, raw="NO", rationale="no memories retrieved")
        prompt = self._build_user_prompt(question, gold, mem_list)
        try:
            response = self._client.messages_create(
                model=self.model,
                max_tokens=160,
                system=_SYSTEM,
                messages=[{"role": "user", "content": prompt}],
            )
        except Exception as exc:  # noqa: BLE001 — surface any SDK error as unavailable
            raise JudgeUnavailableError(f"LlmJudge: {type(exc).__name__}: {exc}") from exc
        token, rationale = _parse_verdict(response)
        return JudgeVerdict(
            correct=(token == "YES"),
            raw=token,
            rationale=rationale,
        )

    def _build_user_prompt(
        self,
        question: str,
        gold: str,
        memories: list[dict[str, Any]],
    ) -> str:
        lines = [
            f"Question: {question}",
            f"Gold answer: {gold}",
            "",
            "Retrieved memories (ranked, most-relevant first):",
        ]
        for rank, mem in enumerate(memories, start=1):
            content = str(mem.get("content", ""))
            if len(content) > self.content_truncate:
                content = content[: self.content_truncate] + "…"
            score = mem.get("score")
            score_str = f" (score={score:.3f})" if isinstance(score, (int, float)) else ""
            lines.append(f"  #{rank}{score_str}: {content}")
        lines.append("")
        lines.append(
            "Does at least one retrieved memory state or directly support the gold answer? "
            "First line: YES, NO, or UNSURE. Second line: one-sentence rationale."
        )
        return "\n".join(lines)


class _AnthropicBridge:
    """Adapts `anthropic.Anthropic` to the `_AnthropicLike` protocol.

    Exists solely to keep the Protocol abstract at the call site so tests
    don't need to fake a full SDK client — injecting any object with a
    matching `messages_create` is enough.
    """

    def __init__(self, client: Any) -> None:
        self._c = client

    def messages_create(
        self,
        *,
        model: str,
        max_tokens: int,
        system: str,
        messages: list[dict[str, Any]],
    ) -> Any:
        return self._c.messages.create(
            model=model,
            max_tokens=max_tokens,
            system=system,
            messages=messages,
        )


def _parse_verdict(response: Any) -> tuple[str, str]:
    """Extract the first-line verdict token and rationale from an Anthropic
    `messages.create` response, or a fake shaped the same way.

    Returns `("UNSURE", "...")` on any parse failure and logs — we do NOT
    want a badly formatted judge response to look like a miss/hit by
    accident.
    """
    text = _response_to_text(response)
    if not text:
        return "UNSURE", "empty response"
    stripped = text.strip().splitlines()
    if not stripped:
        return "UNSURE", "blank response"
    head = stripped[0].strip().upper()
    # Accept bullet/quote prefixes the model sometimes adds even when
    # told not to.
    head = head.lstrip("-* >").strip()
    if head.startswith("YES"):
        token = "YES"
    elif head.startswith("NO"):
        token = "NO"
    else:
        token = "UNSURE"
        if not head.startswith("UNSURE"):
            logger.debug("LlmJudge: unparseable first line %r, scoring UNSURE", stripped[0])
    rationale = " ".join(s.strip() for s in stripped[1:]).strip() or stripped[0]
    return token, rationale


def _response_to_text(response: Any) -> str:
    """Pull text content out of an `anthropic.Message`-shaped object.

    Deliberately permissive: tests inject plain objects with `.content`
    lists where each element has a `.text` or is a string.
    """
    content = getattr(response, "content", None)
    if content is None and isinstance(response, dict):
        content = response.get("content")
    if content is None:
        return ""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts: list[str] = []
        for block in content:
            text = getattr(block, "text", None)
            if text is None and isinstance(block, dict):
                text = block.get("text")
            if isinstance(text, str):
                parts.append(text)
        return "\n".join(parts)
    return str(content)
