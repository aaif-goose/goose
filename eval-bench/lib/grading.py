"""Dispatch one L1 grader runner against a recipe output.

Each L1 grader is invoked as a subprocess per the contract in
`eval-bench/grader_runners/README.md`. This module parses the result and
turns it into a GraderOutcome that the composition layer can consume.

A grader is invoked from the repo root (so paths in `runner` stay
repo-relative). Stdin carries `{"output": str, "task": {...}}`; stdout is
expected to be one line of JSON `{"passed": bool, "score": float, "details": str}`.

Exit codes:
  0 — passed
  1 — failed
  2 — input/programming error in the runner (treated as fail with a clear note)

Timeouts: each grader's `timeout_s` is honoured. A timeout produces a fail
outcome with `timed out after Ns` in the details.
"""

from __future__ import annotations

import json
import shlex
import subprocess
import sys
from pathlib import Path
from typing import Any

from .composition import GraderOutcome
from .graders import L1Grader
from .tasks import Task


def _normalise_python_interpreter(argv: list[str]) -> list[str]:
    """If the runner invokes `python` or `python3`, use the same interpreter
    that's running the harness. Avoids "python not on PATH" surprises on
    macOS (where only python3 is shipped) and pins the grader's Python
    version to the harness's, which is what every recipe author expects."""
    if argv and argv[0] in ("python", "python3"):
        return [sys.executable, *argv[1:]]
    return argv


def grade_one(
    grader: L1Grader,
    recipe_output: str,
    task: Task,
    *,
    repo_root: Path,
) -> GraderOutcome:
    """Invoke one L1 grader runner and return its outcome.

    Never raises for runner-side failures: a crashed runner becomes a fail
    outcome with the failure preserved in `details`. The caller's harness
    can then proceed with composition; one runner blowing up should not stop
    the whole run.
    """
    payload = json.dumps({"output": recipe_output, "task": _task_to_dict(task)})

    try:
        argv = shlex.split(grader.runner)
    except ValueError as e:
        return GraderOutcome(
            grader_id=grader.id,
            passed=False,
            details=f"could not parse runner command {grader.runner!r}: {e}",
        )
    if not argv:
        return GraderOutcome(
            grader_id=grader.id,
            passed=False,
            details=f"empty runner command for {grader.id!r}",
        )

    argv = _normalise_python_interpreter(argv)

    try:
        completed = subprocess.run(
            argv,
            input=payload,
            capture_output=True,
            text=True,
            check=False,
            cwd=repo_root,
            timeout=grader.timeout_s,
        )
    except FileNotFoundError as e:
        return GraderOutcome(
            grader_id=grader.id,
            passed=False,
            details=f"runner not found: {e}",
        )
    except subprocess.TimeoutExpired:
        return GraderOutcome(
            grader_id=grader.id,
            passed=False,
            details=f"timed out after {grader.timeout_s}s",
        )

    return _parse_runner_result(grader.id, completed)


def _parse_runner_result(grader_id: str, completed: subprocess.CompletedProcess) -> GraderOutcome:
    # Exit 2 = runner-side error; we treat as a fail with a clear note rather
    # than letting it propagate.
    if completed.returncode == 2:
        return GraderOutcome(
            grader_id=grader_id,
            passed=False,
            details=f"grader runner reported input error: {(completed.stderr or completed.stdout).strip()[:500]}",
        )

    raw = (completed.stdout or "").strip()
    last_line = raw.splitlines()[-1] if raw else ""
    try:
        payload = json.loads(last_line) if last_line else {}
    except json.JSONDecodeError:
        return GraderOutcome(
            grader_id=grader_id,
            passed=False,
            details=f"could not parse runner JSON output: {last_line!r}",
        )

    passed_from_payload = bool(payload.get("passed", False))
    # If the runner exited 0 but the JSON disagrees, trust the exit code:
    # exit code is the formal contract; the JSON is for evidence/score.
    passed = completed.returncode == 0 if completed.returncode in (0, 1) else passed_from_payload
    score = float(payload.get("score", 1.0 if passed else 0.0))
    details = str(payload.get("details", ""))
    return GraderOutcome(grader_id=grader_id, passed=passed, score=score, details=details)


def _task_to_dict(task: Task) -> dict[str, Any]:
    """Serialise a Task into the `task` subfield of the runner stdin."""
    return {
        "id": task.id,
        "description": task.description,
        "input": task.input,
        "expected": task.expected,
        "axes": task.axes,
        "polarity": task.polarity,
        "tags": task.tags,
        "failure_modes_targeted": task.failure_modes_targeted,
    }
