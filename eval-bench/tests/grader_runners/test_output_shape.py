"""Unit + integration tests for the output_shape grader runner."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

# Unit-level: import the module's grade() function.
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "grader_runners"))
import output_shape  # noqa: E402


# ---------- unit ----------


def test_passes_within_bounds() -> None:
    r = output_shape.grade("hello", min_len=1, max_len=100)
    assert r.passed is True
    assert r.score == 1.0


def test_fails_when_empty_below_min_len() -> None:
    r = output_shape.grade("", min_len=1, max_len=100)
    assert r.passed is False
    assert r.score == 0.0
    assert "min_len" in r.details


def test_fails_when_too_long() -> None:
    r = output_shape.grade("x" * 200, min_len=1, max_len=100)
    assert r.passed is False
    assert "max_len" in r.details


# ---------- integration: run via subprocess ----------


RUNNER = Path(__file__).resolve().parents[2] / "grader_runners" / "output_shape.py"


def _invoke(stdin_payload: str, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(RUNNER), *args],
        input=stdin_payload,
        capture_output=True,
        text=True,
        check=False,
    )


def test_subprocess_passes_with_exit_zero() -> None:
    payload = json.dumps({"output": "hello world", "task": {"id": "t1"}})
    res = _invoke(payload)
    assert res.returncode == 0
    body = json.loads(res.stdout.strip())
    assert body["passed"] is True


def test_subprocess_fails_with_exit_one_for_empty_output() -> None:
    payload = json.dumps({"output": "", "task": {}})
    res = _invoke(payload)
    assert res.returncode == 1
    body = json.loads(res.stdout.strip())
    assert body["passed"] is False


def test_subprocess_exit_two_on_malformed_input() -> None:
    """Malformed input must exit 2 — distinguishes 'recipe failed' from 'grader broken'."""
    res = _invoke("this is not json")
    assert res.returncode == 2
    body = json.loads(res.stdout.strip())
    assert "input error" in body["details"]


def test_subprocess_exit_two_on_missing_output_field() -> None:
    res = _invoke(json.dumps({"task": {}}))
    assert res.returncode == 2


def test_subprocess_reads_input_file(tmp_path: Path) -> None:
    p = tmp_path / "trial.json"
    p.write_text(json.dumps({"output": "hi", "task": {}}))
    res = _invoke("", "--input", str(p))
    assert res.returncode == 0


def test_subprocess_max_len_flag_is_respected() -> None:
    res = _invoke(json.dumps({"output": "x" * 50, "task": {}}), "--max-len", "10")
    assert res.returncode == 1
