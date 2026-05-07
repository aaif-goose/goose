"""Shared helpers for grader runners.

A grader runner reads a JSON trial description from stdin (or from --input PATH),
applies one invariant, and emits a one-line JSON result with a clear exit code.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass
class Trial:
    output: str
    task: dict[str, Any]


@dataclass
class GraderResult:
    passed: bool
    score: float
    details: str


def add_input_argument(parser: argparse.ArgumentParser) -> None:
    """Every runner accepts --input. If omitted, stdin is read."""
    parser.add_argument(
        "--input",
        type=Path,
        default=None,
        help="Path to JSON trial description. If omitted, read from stdin.",
    )


def read_trial(path: Path | None) -> Trial:
    raw = path.read_text(encoding="utf-8") if path else sys.stdin.read()
    if not raw.strip():
        raise ValueError("trial input is empty")
    obj = json.loads(raw)
    if not isinstance(obj, dict):
        raise ValueError("trial input must be a JSON object")
    if "output" not in obj or not isinstance(obj["output"], str):
        raise ValueError("trial input must contain a string 'output' field")
    return Trial(output=obj["output"], task=obj.get("task") or {})


def emit_and_exit(result: GraderResult) -> None:
    """Write the result and exit with the right code. Always called from main()."""
    print(json.dumps({"passed": result.passed, "score": result.score, "details": result.details}))
    sys.exit(0 if result.passed else 1)


def fail_on_input_error(message: str) -> None:
    """Exit 2 (not 1) for malformed input — distinguishes 'recipe failed' from 'grader broken'."""
    print(json.dumps({"passed": False, "score": 0.0, "details": f"input error: {message}"}))
    sys.exit(2)
