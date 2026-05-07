#!/usr/bin/env python3
"""L1 grader: output is a non-empty string under a configurable max length.

The floor of correctness for any text-producing recipe. Catches the
"silently returned an empty string" failure mode that any further grader
would hide behind its own logic.

Usage:
    python eval-bench/grader_runners/output_shape.py [--min-len 1] [--max-len 50000]
    # reads JSON {"output": str, "task": {...}} from stdin
"""

from __future__ import annotations

import argparse

# Allow running this file directly: enable `from _common import ...`.
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from _common import (  # noqa: E402
    GraderResult,
    add_input_argument,
    emit_and_exit,
    fail_on_input_error,
    read_trial,
)


def grade(output: str, *, min_len: int, max_len: int) -> GraderResult:
    n = len(output)
    if n < min_len:
        return GraderResult(False, 0.0, f"output length {n} < min_len {min_len}")
    if n > max_len:
        return GraderResult(False, 0.0, f"output length {n} > max_len {max_len}")
    return GraderResult(True, 1.0, f"output length {n} within [{min_len}, {max_len}]")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    add_input_argument(parser)
    parser.add_argument("--min-len", type=int, default=1)
    parser.add_argument("--max-len", type=int, default=50000)
    args = parser.parse_args()

    try:
        trial = read_trial(args.input)
    except (ValueError, OSError) as e:
        fail_on_input_error(str(e))
        return

    emit_and_exit(grade(trial.output, min_len=args.min_len, max_len=args.max_len))


if __name__ == "__main__":
    main()
