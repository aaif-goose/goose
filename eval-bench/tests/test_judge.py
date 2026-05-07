"""Tests for the L3 judge abstraction, verdict parser, StubJudge, and AnthropicJudge."""

from __future__ import annotations

import json
from unittest.mock import MagicMock, patch

import pytest

from lib.judge import (
    ANTHROPIC_API_URL,
    AnthropicJudge,
    Judge,
    JudgeVerdict,
    StubJudge,
    parse_verdict,
)


# ---------- Protocol shape ----------


def test_protocol_recognises_concrete_judges() -> None:
    assert isinstance(StubJudge.always("pass"), Judge)
    assert isinstance(AnthropicJudge(api_key="dummy"), Judge)


# ---------- parse_verdict ----------


def test_parse_verdict_clean_json() -> None:
    raw = '{"verdict": "pass", "evidence": "all sections present"}'
    v = parse_verdict(raw)
    assert v.verdict == "pass"
    assert v.evidence == "all sections present"
    assert v.error is None


def test_parse_verdict_in_json_code_fence() -> None:
    raw = (
        "Here is my verdict:\n\n"
        "```json\n"
        '{"verdict": "fail", "evidence": "fabricated 429 threshold"}\n'
        "```\n"
        "Hope this helps."
    )
    v = parse_verdict(raw)
    assert v.verdict == "fail"
    assert "fabricated" in v.evidence


def test_parse_verdict_in_unlabeled_code_fence() -> None:
    raw = "```\n" '{"verdict": "Unknown", "evidence": "cannot tell"}\n' "```"
    v = parse_verdict(raw)
    assert v.verdict == "Unknown"


def test_parse_verdict_bare_object_amid_prose() -> None:
    raw = (
        'I considered the inputs. My answer: {"verdict": "pass", "evidence": "ok"} — done.'
    )
    v = parse_verdict(raw)
    assert v.verdict == "pass"


def test_parse_verdict_invalid_verdict_value() -> None:
    raw = '{"verdict": "yes", "evidence": "ok"}'
    v = parse_verdict(raw)
    assert v.verdict == "Unknown"
    assert v.error is not None
    assert "not one of" in v.error


def test_parse_verdict_missing_verdict_field() -> None:
    raw = '{"evidence": "without a verdict field"}'
    v = parse_verdict(raw)
    assert v.verdict == "Unknown"
    assert v.error is not None


def test_parse_verdict_no_json_at_all() -> None:
    raw = "I don't think I should answer this."
    v = parse_verdict(raw)
    assert v.verdict == "Unknown"
    assert v.error is not None


def test_parse_verdict_malformed_json() -> None:
    raw = '{"verdict": "pass", "evidence": "missing close'
    v = parse_verdict(raw)
    assert v.verdict == "Unknown"
    assert v.error is not None


def test_parse_verdict_empty_response() -> None:
    v = parse_verdict("")
    assert v.verdict == "Unknown"
    assert "empty" in (v.error or "")


def test_parse_verdict_evidence_optional() -> None:
    raw = '{"verdict": "pass"}'
    v = parse_verdict(raw)
    assert v.verdict == "pass"
    assert v.evidence == ""


# ---------- StubJudge ----------


def test_stub_judge_always_pass() -> None:
    j = StubJudge.always("pass", evidence="stub note")
    v = j.judge("rubric", {"feature_brief": "x"})
    assert v.verdict == "pass"
    assert v.evidence == "stub note"


def test_stub_judge_always_fail() -> None:
    j = StubJudge.always("fail")
    assert j.judge("r", {}).verdict == "fail"


def test_stub_judge_always_rejects_invalid_verdict() -> None:
    with pytest.raises(ValueError, match="verdict must be one of"):
        StubJudge.always("definitely-pass")


def test_stub_judge_callable_receives_rubric_and_payload() -> None:
    captured: dict[str, object] = {}

    def fn(rubric: str, payload: dict) -> JudgeVerdict:
        captured["rubric"] = rubric
        captured["payload"] = payload
        return JudgeVerdict(verdict="pass", evidence="ok")

    j = StubJudge(fn)
    j.judge("# Rubric", {"output": "hi"})
    assert captured["rubric"] == "# Rubric"
    assert captured["payload"] == {"output": "hi"}


# ---------- AnthropicJudge: construction and API key handling ----------


def test_anthropic_judge_no_api_key_returns_unknown_with_error(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
    j = AnthropicJudge()
    v = j.judge("rubric", {"output": "hi"})
    assert v.verdict == "Unknown"
    assert v.error is not None
    assert "ANTHROPIC_API_KEY" in v.error


def test_anthropic_judge_picks_up_env_api_key(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("ANTHROPIC_API_KEY", "test-key")
    j = AnthropicJudge()
    assert j.api_key == "test-key"


def test_anthropic_judge_explicit_api_key_overrides_env(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("ANTHROPIC_API_KEY", "env-key")
    j = AnthropicJudge(api_key="explicit-key")
    assert j.api_key == "explicit-key"


# ---------- AnthropicJudge: request body shape (pinned) ----------


def test_anthropic_request_body_includes_required_fields() -> None:
    j = AnthropicJudge(api_key="dummy", model="claude-opus-4-7")
    body = j._build_request_body(
        rubric_text="# Rubric — Honesty dimension",
        payload={"feature_brief": "Add /healthz", "output": "## Structure ..."},
    )
    assert body["model"] == "claude-opus-4-7"
    assert body["max_tokens"] == 1024
    assert body["temperature"] == 0.0
    assert body["system"] == "# Rubric — Honesty dimension"
    assert isinstance(body["messages"], list)
    assert body["messages"][0]["role"] == "user"
    user_content = body["messages"][0]["content"]
    assert "Add /healthz" in user_content
    assert "## Structure" in user_content
    # The user message must remind the model of the JSON-output contract.
    assert "JSON" in user_content


def test_anthropic_request_body_pretty_prints_payload() -> None:
    j = AnthropicJudge(api_key="dummy")
    body = j._build_request_body(
        rubric_text="r",
        payload={"a": 1, "b": "two", "c": [1, 2]},
    )
    user_content = body["messages"][0]["content"]
    # Pretty-printed JSON has 2-space indent and quoted keys.
    assert '"a"' in user_content
    assert '"two"' in user_content


# ---------- AnthropicJudge: HTTP path with mocked transport ----------


def _api_response(text: str) -> bytes:
    return json.dumps({"content": [{"type": "text", "text": text}]}).encode("utf-8")


def test_anthropic_judge_full_path_with_mocked_http(monkeypatch: pytest.MonkeyPatch) -> None:
    """End-to-end: AnthropicJudge.judge() calls urlopen with the right
    URL/headers/body and parses the assistant's response."""
    monkeypatch.setenv("ANTHROPIC_API_KEY", "test-key-xyz")

    captured: dict[str, object] = {}

    class FakeResp:
        def __init__(self, body: bytes) -> None:
            self._body = body

        def __enter__(self) -> "FakeResp":
            return self

        def __exit__(self, *args: object) -> None:
            return None

        def read(self) -> bytes:
            return self._body

    def fake_urlopen(req, timeout: int):  # noqa: ANN001 — urllib.request.Request
        captured["url"] = req.full_url
        captured["headers"] = dict(req.header_items())
        captured["body"] = json.loads(req.data.decode("utf-8"))
        captured["timeout"] = timeout
        return FakeResp(_api_response('{"verdict": "pass", "evidence": "ok"}'))

    with patch("lib.judge.urllib.request.urlopen", fake_urlopen):
        j = AnthropicJudge(model="claude-opus-4-7")
        v = j.judge("# Rubric", {"feature_brief": "x", "output": "y"})

    assert v.verdict == "pass"
    assert v.evidence == "ok"
    assert v.error is None

    # Pin the HTTP shape.
    assert captured["url"] == ANTHROPIC_API_URL
    headers = {k.lower(): v for k, v in captured["headers"].items()}
    assert headers["x-api-key"] == "test-key-xyz"
    assert headers["anthropic-version"] == "2023-06-01"
    assert headers["content-type"] == "application/json"
    body = captured["body"]
    assert body["model"] == "claude-opus-4-7"
    assert body["system"] == "# Rubric"


def test_anthropic_judge_http_error_returns_unknown(monkeypatch: pytest.MonkeyPatch) -> None:
    import urllib.error
    monkeypatch.setenv("ANTHROPIC_API_KEY", "k")

    err = urllib.error.HTTPError(
        url=ANTHROPIC_API_URL, code=429, msg="Too Many Requests",
        hdrs={}, fp=MagicMock(read=lambda: b"rate limited"),
    )

    def raising_urlopen(*_args, **_kwargs):
        raise err

    with patch("lib.judge.urllib.request.urlopen", raising_urlopen):
        v = AnthropicJudge().judge("r", {})
    assert v.verdict == "Unknown"
    assert v.error is not None
    assert "HTTP 429" in v.error


def test_anthropic_judge_url_error_returns_unknown(monkeypatch: pytest.MonkeyPatch) -> None:
    import urllib.error
    monkeypatch.setenv("ANTHROPIC_API_KEY", "k")

    def raising_urlopen(*_args, **_kwargs):
        raise urllib.error.URLError("connection refused")

    with patch("lib.judge.urllib.request.urlopen", raising_urlopen):
        v = AnthropicJudge().judge("r", {})
    assert v.verdict == "Unknown"
    assert v.error is not None
    assert "URL error" in v.error


def test_anthropic_judge_handles_unparseable_assistant_response(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("ANTHROPIC_API_KEY", "k")

    class FakeResp:
        def __enter__(self) -> "FakeResp":
            return self

        def __exit__(self, *args: object) -> None:
            return None

        def read(self) -> bytes:
            # Assistant returned prose with no JSON.
            return _api_response("I prefer not to answer this.")

    with patch("lib.judge.urllib.request.urlopen", lambda *_a, **_k: FakeResp()):
        v = AnthropicJudge().judge("r", {})
    assert v.verdict == "Unknown"
    assert v.error is not None
