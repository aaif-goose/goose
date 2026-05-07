#!/usr/bin/env python3
"""L1 grader: output is markdown with the required H2 sections present.

For recipes whose contract is "produce a structured markdown document with
these named sections." Catches the "skipped a section silently" and "renamed a
section" failure modes. Sections may be in any order. Matching is
case-insensitive on the heading text, but the level (## by default) is exact
so we don't pass H1 or H3 in place of H2.

Usage:
    python eval-bench/grader_runners/markdown_sections.py \
        --required Structure,Function,Data,Interfaces,Platform,Operations,Time \
        [--level 2]
    # reads JSON {"output": str, "task": {...}} from stdin
"""

from __future__ import annotations

import argparse
import re
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


def extract_headings(markdown: str, level: int) -> list[str]:
    """Return the trimmed text of every heading at the requested level.

    Skips fenced code blocks so a `## inside a code fence` does not count.
    """
    prefix = "#" * level + " "
    lines = markdown.splitlines()
    in_fence = False
    headings: list[str] = []
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        if line.startswith(prefix):
            text = line[len(prefix):].strip()
            # Strip trailing closing hashes like `## Foo ##`.
            text = re.sub(r"\s+#+\s*$", "", text)
            if text:
                headings.append(text)
    return headings


def grade(output: str, *, required: list[str], level: int) -> GraderResult:
    found = {h.lower() for h in extract_headings(output, level)}
    missing = [r for r in required if r.lower() not in found]
    if missing:
        return GraderResult(
            False,
            (len(required) - len(missing)) / len(required) if required else 0.0,
            f"missing required H{level} sections: {missing}",
        )
    return GraderResult(True, 1.0, f"all {len(required)} required H{level} sections present")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    add_input_argument(parser)
    parser.add_argument(
        "--required",
        required=True,
        help="Comma-separated list of required heading texts (e.g. Structure,Function,Data).",
    )
    parser.add_argument("--level", type=int, default=2, help="Heading level. Default 2 (H2).")
    args = parser.parse_args()

    required = [s.strip() for s in args.required.split(",") if s.strip()]
    if not required:
        fail_on_input_error("--required must contain at least one section name")
        return
    if args.level < 1 or args.level > 6:
        fail_on_input_error(f"--level must be 1..6 (got {args.level})")
        return

    try:
        trial = read_trial(args.input)
    except (ValueError, OSError) as e:
        fail_on_input_error(str(e))
        return

    emit_and_exit(grade(trial.output, required=required, level=args.level))


if __name__ == "__main__":
    main()
