"""L3 judge abstraction.

A Judge takes a rubric (markdown) and a payload (the things the rubric tells
it to grade — usually feature_brief, task_expected, output) and returns a
JudgeVerdict in `{verdict: pass|fail|Unknown, evidence: <str>}` form.

Two implementations ship:

    StubJudge      — synchronous, deterministic; constructed with a callable
                     that maps (rubric_text, payload) to a JudgeVerdict.
                     Used by tests and by `--judge stub` for harness smoke.

    AnthropicJudge — POSTs to https://api.anthropic.com/v1/messages with
                     the rubric as the system prompt and the payload as the
                     user message; expects the assistant's response to
                     contain a JSON object matching the verdict schema.
                     Auth via the ANTHROPIC_API_KEY env var.

The Anthropic API request shape is captured in one place
(`AnthropicJudge._build_request_body`) so future API changes are a
deliberate, visible diff. We use stdlib urllib.request to keep deps minimal.
"""

from __future__ import annotations

import json
import os
import re
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Callable, Protocol, runtime_checkable

ANTHROPIC_API_URL = "https://api.anthropic.com/v1/messages"
ANTHROPIC_API_VERSION = "2023-06-01"


@dataclass
class JudgeVerdict:
    """One judge call's result.

    `verdict` is one of "pass", "fail", or "Unknown" (Anthropic's "give the
    judge an out" guidance — when the judge cannot decide from the evidence,
    Unknown is the honest answer and the harness surfaces it for human
    review rather than silently treating it as a fail).
    `evidence` is a short string the judge used to justify the verdict
    (typically a short quote from the output).
    `error` is set when the judge could not produce a verdict (network
    failure, API key missing, malformed response). When set, the harness
    treats the L3 grader as skipped.
    """

    verdict: str
    evidence: str = ""
    raw_response: str = ""
    error: str | None = None
    extra: dict[str, Any] = field(default_factory=dict)


@runtime_checkable
class Judge(Protocol):
    """Contract for judges. Synchronous; one trial at a time."""

    def judge(self, rubric_text: str, payload: dict[str, Any]) -> JudgeVerdict: ...


# ---------- Verdict parsing ----------

_VERDICT_FENCE_RE = re.compile(r"```(?:json)?\s*(\{.*?\})\s*```", re.DOTALL)
_BARE_OBJECT_RE = re.compile(r"\{[^{}]*\"verdict\"[^{}]*\}", re.DOTALL)
_VALID_VERDICTS = {"pass", "fail", "Unknown"}


def parse_verdict(text: str) -> JudgeVerdict:
    """Extract a JudgeVerdict from a raw model response.

    Tries, in order: fenced ```json``` block, fenced ``` block, then the first
    bare {...} object containing a "verdict" field. Validates the verdict
    value against the allowed set. Returns a JudgeVerdict with `error` set
    when nothing parseable is found.
    """
    if not text or not text.strip():
        return JudgeVerdict(verdict="Unknown", error="empty judge response", raw_response=text)

    candidate = _extract_json_object(text)
    if candidate is None:
        return JudgeVerdict(
            verdict="Unknown",
            error="no JSON object with 'verdict' found in response",
            raw_response=text,
        )

    try:
        parsed = json.loads(candidate)
    except json.JSONDecodeError as e:
        return JudgeVerdict(
            verdict="Unknown",
            error=f"could not parse verdict JSON: {e}",
            raw_response=text,
        )

    if not isinstance(parsed, dict) or "verdict" not in parsed:
        return JudgeVerdict(
            verdict="Unknown",
            error="parsed object missing 'verdict' field",
            raw_response=text,
        )

    verdict = str(parsed["verdict"]).strip()
    if verdict not in _VALID_VERDICTS:
        return JudgeVerdict(
            verdict="Unknown",
            error=f"verdict {verdict!r} not one of {sorted(_VALID_VERDICTS)}",
            raw_response=text,
        )

    return JudgeVerdict(
        verdict=verdict,
        evidence=str(parsed.get("evidence", "")).strip(),
        raw_response=text,
    )


def _extract_json_object(text: str) -> str | None:
    fence = _VERDICT_FENCE_RE.search(text)
    if fence:
        return fence.group(1)
    bare = _BARE_OBJECT_RE.search(text)
    if bare:
        return bare.group(0)
    return None


# ---------- Stub judge ----------


class StubJudge:
    """Test/smoke judge.

    Constructed with a callable that maps (rubric_text, payload) to a
    JudgeVerdict. The convenience factory `always` returns a fixed verdict
    regardless of input.
    """

    def __init__(self, fn: Callable[[str, dict[str, Any]], JudgeVerdict]) -> None:
        self._fn = fn

    def judge(self, rubric_text: str, payload: dict[str, Any]) -> JudgeVerdict:
        return self._fn(rubric_text, payload)

    @classmethod
    def always(cls, verdict: str, evidence: str = "stub") -> "StubJudge":
        if verdict not in _VALID_VERDICTS:
            raise ValueError(f"verdict must be one of {sorted(_VALID_VERDICTS)}; got {verdict!r}")
        return cls(lambda _r, _p: JudgeVerdict(verdict=verdict, evidence=evidence))


# ---------- Anthropic judge ----------


@dataclass
class AnthropicJudge:
    """L3 judge backed by the Anthropic Messages API.

    Defaults are conservative: a single 1024-token completion, low temperature,
    explicit response-format hint via the system prompt. The HTTP request
    shape is in `_build_request_body` so future API changes are a one-place
    edit.

    Auth: ANTHROPIC_API_KEY env var unless `api_key` is passed explicitly.
    Without an API key, every judge call returns a verdict with `error` set;
    the harness treats this as a skipped L3 grader rather than a failure.
    """

    model: str = "claude-opus-4-7"
    api_key: str | None = None
    max_tokens: int = 1024
    temperature: float = 0.0
    timeout_s: int = 60

    def __post_init__(self) -> None:
        if self.api_key is None:
            self.api_key = os.environ.get("ANTHROPIC_API_KEY")

    def judge(self, rubric_text: str, payload: dict[str, Any]) -> JudgeVerdict:
        if not self.api_key:
            return JudgeVerdict(
                verdict="Unknown",
                error="ANTHROPIC_API_KEY not set; pass --judge stub or configure the env var",
            )

        body = self._build_request_body(rubric_text, payload)
        try:
            response_text = self._post_messages(body)
        except urllib.error.HTTPError as e:
            return JudgeVerdict(
                verdict="Unknown",
                error=f"Anthropic API HTTP {e.code}: {self._read_error(e)}",
            )
        except urllib.error.URLError as e:
            return JudgeVerdict(verdict="Unknown", error=f"Anthropic API URL error: {e}")
        except (TimeoutError, OSError) as e:
            return JudgeVerdict(verdict="Unknown", error=f"Anthropic API timeout/IO: {e}")

        return parse_verdict(response_text)

    def _build_request_body(self, rubric_text: str, payload: dict[str, Any]) -> dict[str, Any]:
        user_content = (
            "Inputs:\n\n"
            f"{json.dumps(payload, indent=2, ensure_ascii=False)}\n\n"
            "Return your verdict in the exact JSON format specified by the rubric."
        )
        return {
            "model": self.model,
            "max_tokens": self.max_tokens,
            "temperature": self.temperature,
            "system": rubric_text,
            "messages": [{"role": "user", "content": user_content}],
        }

    def _post_messages(self, body: dict[str, Any]) -> str:
        req = urllib.request.Request(
            ANTHROPIC_API_URL,
            data=json.dumps(body).encode("utf-8"),
            headers={
                "x-api-key": self.api_key or "",
                "anthropic-version": ANTHROPIC_API_VERSION,
                "content-type": "application/json",
            },
            method="POST",
        )
        started = time.monotonic()
        with urllib.request.urlopen(req, timeout=self.timeout_s) as resp:
            raw = resp.read().decode("utf-8")
        _ = time.monotonic() - started  # reserved for future telemetry
        return _extract_assistant_text(raw)

    @staticmethod
    def _read_error(err: urllib.error.HTTPError) -> str:
        try:
            return err.read().decode("utf-8", errors="replace")[:500]
        except Exception:
            return str(err)


def _extract_assistant_text(api_response_json: str) -> str:
    """Pull the assistant's text out of the Anthropic Messages API envelope.

    The /v1/messages response shape (2023-06-01):
        { "content": [{"type": "text", "text": "..."}, ...], ... }
    We concatenate every text block.
    """
    obj = json.loads(api_response_json)
    blocks = obj.get("content") or []
    texts = [b.get("text", "") for b in blocks if isinstance(b, dict) and b.get("type") == "text"]
    return "".join(texts)
