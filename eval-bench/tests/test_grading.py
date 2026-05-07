"""Tests for grading dispatch — invokes the real grader runners via subprocess."""

from __future__ import annotations

from pathlib import Path

import pytest

from lib.grading import grade_one
from lib.graders import L1Grader
from lib.tasks import Task

REPO_ROOT = Path(__file__).resolve().parents[2]


def _markdown_sections_grader(required: str, *, timeout_s: int = 10) -> L1Grader:
    return L1Grader(
        id="g-sections",
        level="L1",
        type="code",
        weight=1.0,
        dimension=None,
        runner=f"python eval-bench/grader_runners/markdown_sections.py --required {required}",
        timeout_s=timeout_s,
    )


def _output_shape_grader(*, timeout_s: int = 10) -> L1Grader:
    return L1Grader(
        id="g-shape",
        level="L1",
        type="code",
        weight=1.0,
        dimension=None,
        runner="python eval-bench/grader_runners/output_shape.py",
        timeout_s=timeout_s,
    )


def _positive_task() -> Task:
    return Task(
        id="t-pos",
        description="positive",
        input={"feature_brief": "x"},
        polarity="positive",
        tags=["capability"],
    )


# ---------- happy path ----------


def test_grade_one_passes_full_charter() -> None:
    grader = _markdown_sections_grader("Structure,Function,Data,Interfaces,Platform,Operations,Time")
    full = "\n".join(f"## {s}" for s in [
        "Structure", "Function", "Data", "Interfaces", "Platform", "Operations", "Time"
    ])
    outcome = grade_one(grader, full, _positive_task(), repo_root=REPO_ROOT)
    assert outcome.passed is True
    assert outcome.skipped is False
    assert outcome.score == 1.0
    assert "all 7 required" in outcome.details


def test_grade_one_fails_missing_section() -> None:
    grader = _markdown_sections_grader("Structure,Function,Data,Interfaces,Platform,Operations,Time")
    incomplete = "\n".join(f"## {s}" for s in [
        "Structure", "Function", "Data", "Interfaces", "Platform", "Operations"
    ])  # Time missing
    outcome = grade_one(grader, incomplete, _positive_task(), repo_root=REPO_ROOT)
    assert outcome.passed is False
    assert "Time" in outcome.details


def test_grade_one_passes_non_empty_output_for_shape() -> None:
    grader = _output_shape_grader()
    outcome = grade_one(grader, "any non-empty text", _positive_task(), repo_root=REPO_ROOT)
    assert outcome.passed is True


def test_grade_one_fails_empty_output_for_shape() -> None:
    grader = _output_shape_grader()
    outcome = grade_one(grader, "", _positive_task(), repo_root=REPO_ROOT)
    assert outcome.passed is False


# ---------- input/programming errors in the runner ----------


def test_grade_one_unparseable_runner_command() -> None:
    grader = L1Grader(
        id="g-bad",
        level="L1",
        type="code",
        weight=1.0,
        dimension=None,
        runner='python -c "open quote',  # unbalanced quote
        timeout_s=5,
    )
    outcome = grade_one(grader, "anything", _positive_task(), repo_root=REPO_ROOT)
    assert outcome.passed is False
    assert "could not parse runner command" in outcome.details


def test_grade_one_runner_not_found() -> None:
    grader = L1Grader(
        id="g-missing",
        level="L1",
        type="code",
        weight=1.0,
        dimension=None,
        runner="this-binary-does-not-exist-xyz",
        timeout_s=5,
    )
    outcome = grade_one(grader, "anything", _positive_task(), repo_root=REPO_ROOT)
    assert outcome.passed is False
    assert "runner not found" in outcome.details or "No such file" in outcome.details


def test_grade_one_runner_input_error_returned_as_fail_with_note() -> None:
    """markdown_sections needs --required; without it the runner exits 2.
    grade_one converts that into a fail outcome with a clear note rather
    than letting it propagate as a Python exception."""
    grader = L1Grader(
        id="g-no-required",
        level="L1",
        type="code",
        weight=1.0,
        dimension=None,
        runner="python eval-bench/grader_runners/markdown_sections.py",
        timeout_s=5,
    )
    outcome = grade_one(grader, "## A", _positive_task(), repo_root=REPO_ROOT)
    assert outcome.passed is False
    assert "input error" in outcome.details


# ---------- timeout ----------


def test_grade_one_timeout(tmp_path: Path) -> None:
    """A grader that hangs longer than its timeout becomes a fail outcome."""
    fake = tmp_path / "slow_grader.py"
    fake.write_text("import time\ntime.sleep(5)\n")
    grader = L1Grader(
        id="g-slow",
        level="L1",
        type="code",
        weight=1.0,
        dimension=None,
        runner=f"python {fake}",
        timeout_s=1,
    )
    outcome = grade_one(grader, "anything", _positive_task(), repo_root=REPO_ROOT)
    assert outcome.passed is False
    assert "timed out" in outcome.details


# ---------- task payload reaches the runner ----------


def test_runner_receives_task_payload(tmp_path: Path) -> None:
    """Verify the stdin payload includes both `output` and `task` so runners
    that need task context (axes, polarity, etc.) can use it."""
    echo_grader = tmp_path / "echo_grader.py"
    echo_grader.write_text(
        'import json, sys\n'
        'data = json.loads(sys.stdin.read())\n'
        'task = data.get("task", {})\n'
        'has_task = "polarity" in task and "id" in task\n'
        'has_output = "output" in data\n'
        'ok = has_task and has_output\n'
        'details = "ok" if ok else "missing keys"\n'
        'sys.stdout.write(json.dumps({"passed": ok, "score": 1.0 if ok else 0.0, "details": details}))\n'
        'sys.exit(0 if ok else 1)\n'
    )
    grader = L1Grader(
        id="g-echo",
        level="L1",
        type="code",
        weight=1.0,
        dimension=None,
        runner=f"python {echo_grader}",
        timeout_s=5,
    )
    outcome = grade_one(grader, "hello", _positive_task(), repo_root=REPO_ROOT)
    assert outcome.passed is True
