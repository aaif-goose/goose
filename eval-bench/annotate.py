#!/usr/bin/env python3
"""annotate.py — work the L2 annotation queue for a Skein recipe.

Subcommands:

    list <recipe>
        Show pending and completed annotation counts plus a one-line
        summary per pending annotation.

    show <recipe> --id <annotation_id>
        Print one annotation's full content (task input, recipe output,
        rubric path).

    review <recipe> --id <annotation_id> --verdict pass|fail|unknown \\
                    --reviewer <name> [--note <text>] \\
                    [--score <dim>=<int> ...]
        Mark an annotation as completed with the SME's verdict. Suitable
        for both interactive use and scripted batch annotation.

    discard <recipe> --id <annotation_id> --reason <text>
        Remove a pending annotation from the active queue (e.g., the
        recipe output was malformed and the SME does not want it as a
        calibration data point).

The annotations live under <recipe>/evals/annotations/ as one JSON file
per annotation. The rubric the SME applies is at <recipe>/evals/<rubric>.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

_THIS_DIR = Path(__file__).resolve().parent
if str(_THIS_DIR) not in sys.path:
    sys.path.insert(0, str(_THIS_DIR))

from lib.annotations import AnnotationStore  # noqa: E402


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_list = sub.add_parser("list", help="list queue contents")
    p_list.add_argument("recipe", type=Path)

    p_show = sub.add_parser("show", help="print one annotation in full")
    p_show.add_argument("recipe", type=Path)
    p_show.add_argument("--id", required=True, dest="annotation_id")

    p_review = sub.add_parser("review", help="mark an annotation completed")
    p_review.add_argument("recipe", type=Path)
    p_review.add_argument("--id", required=True, dest="annotation_id")
    p_review.add_argument("--verdict", required=True, choices=["pass", "fail", "unknown"])
    p_review.add_argument("--reviewer", required=True, help="who is signing off on this verdict")
    p_review.add_argument("--note", default="", help="optional free-form note")
    p_review.add_argument(
        "--score",
        action="append",
        default=[],
        metavar="DIM=N",
        help="per-dimension integer score; repeat for multiple dimensions",
    )

    p_discard = sub.add_parser("discard", help="remove a pending annotation from the queue")
    p_discard.add_argument("recipe", type=Path)
    p_discard.add_argument("--id", required=True, dest="annotation_id")
    p_discard.add_argument("--reason", required=True)

    args = parser.parse_args(argv)
    store = _store_for_recipe(args.recipe)

    if args.cmd == "list":
        return _cmd_list(store)
    if args.cmd == "show":
        return _cmd_show(store, args.annotation_id)
    if args.cmd == "review":
        return _cmd_review(
            store,
            args.annotation_id,
            verdict=args.verdict,
            reviewer=args.reviewer,
            note=args.note,
            score_args=args.score,
        )
    if args.cmd == "discard":
        return _cmd_discard(store, args.annotation_id, reason=args.reason)
    parser.error(f"unknown subcommand {args.cmd!r}")
    return 2


def _store_for_recipe(recipe: Path) -> AnnotationStore:
    annotations_dir = recipe / "evals" / "annotations"
    if not (recipe / "evals").is_dir():
        print(f"error: {recipe} has no evals/ directory", file=sys.stderr)
        sys.exit(2)
    return AnnotationStore(annotations_dir)


def _cmd_list(store: AnnotationStore) -> int:
    pending = store.list_pending()
    completed = store.list_completed()
    print(f"queue: {store.directory}")
    print(f"pending:   {len(pending)}")
    print(f"completed: {len(completed)}")
    if not pending:
        print("\n(no pending annotations)")
        return 0
    print("\npending:")
    for a in pending:
        first_line_of_input = _first_line(a.task_input.get("feature_brief"))
        print(f"  {a.annotation_id}")
        print(f"    task={a.task_id}  polarity={a.polarity}  trial={a.trial_index}")
        if first_line_of_input:
            print(f"    input: {first_line_of_input}")
    return 0


def _cmd_show(store: AnnotationStore, annotation_id: str) -> int:
    try:
        a = store.read(annotation_id)
    except FileNotFoundError:
        print(f"error: annotation {annotation_id!r} not found", file=sys.stderr)
        return 2
    print(f"annotation_id: {a.annotation_id}")
    print(f"status:        {a.status}")
    print(f"task:          {a.task_id} (polarity={a.polarity}, trial={a.trial_index})")
    print(f"axes:          {a.axes}")
    print(f"rubric:        {a.rubric_path}")
    print()
    print("=== task input ===")
    for k, v in a.task_input.items():
        print(f"{k}: {v}")
    print()
    print("=== recipe output ===")
    print(a.recipe_output)
    if a.review:
        print()
        print("=== review ===")
        print(f"verdict:  {a.review.verdict}")
        print(f"reviewer: {a.review.reviewer}")
        print(f"at:       {a.review.reviewed_at}")
        if a.review.scores:
            print(f"scores:   {a.review.scores}")
        if a.review.notes:
            print(f"notes:    {a.review.notes}")
    return 0


def _cmd_review(
    store: AnnotationStore,
    annotation_id: str,
    *,
    verdict: str,
    reviewer: str,
    note: str,
    score_args: list[str],
) -> int:
    scores = _parse_scores(score_args)
    try:
        a = store.complete(
            annotation_id,
            verdict=verdict,
            reviewer=reviewer,
            notes=note,
            scores=scores,
        )
    except FileNotFoundError:
        print(f"error: annotation {annotation_id!r} not found", file=sys.stderr)
        return 2
    except ValueError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    print(f"completed {a.annotation_id}: {verdict} by {reviewer}")
    return 0


def _cmd_discard(store: AnnotationStore, annotation_id: str, *, reason: str) -> int:
    try:
        store.discard(annotation_id, reason=reason)
    except FileNotFoundError:
        print(f"error: annotation {annotation_id!r} not found", file=sys.stderr)
        return 2
    except ValueError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    print(f"discarded {annotation_id}")
    return 0


def _parse_scores(score_args: list[str]) -> dict[str, int]:
    scores: dict[str, int] = {}
    for s in score_args:
        if "=" not in s:
            print(f"error: --score expects DIM=N, got {s!r}", file=sys.stderr)
            sys.exit(2)
        dim, raw = s.split("=", 1)
        try:
            scores[dim.strip()] = int(raw.strip())
        except ValueError:
            print(f"error: --score value must be int, got {raw!r}", file=sys.stderr)
            sys.exit(2)
    return scores


def _first_line(value: object) -> str:
    if not isinstance(value, str):
        return ""
    line = value.strip().splitlines()[0] if value.strip() else ""
    return (line[:80] + "…") if len(line) > 80 else line


if __name__ == "__main__":
    sys.exit(main())
