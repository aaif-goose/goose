#!/usr/bin/env python3
"""calibrate.py — calibrate an L3 judge against L2 annotations.

Reads completed annotations from a recipe's evals/annotations/ queue, runs
the configured L3 judge against the same outputs, computes agreement
between the SME's headline verdict (review.verdict) and the judge's
verdict, and appends a calibration record to evals/calibration.jsonl.

Usage:
    python eval-bench/calibrate.py <recipe> --grader-id g-charter-judge \\
        [--judge anthropic|stub] \\
        [--min-sample-size 20] \\
        [--max-divergence 0.15] \\
        [--note "..."]

The recipe's graders.yaml must declare an L3 grader with the given
--grader-id; that grader's `judge_model`, `rubric`, and
`max_divergence_from_l2` are read from there.

Pairs the runner cannot use as calibration data are listed at the end with
their reason: SME verdict was unknown, judge errored, judge returned
Unknown. These do NOT contribute to the agreement number; only `pass`/`fail`
pairs do, per Anthropic's calibration practice.

Exit codes:
    0  calibration computed; record appended; deployable.
    1  calibration computed; record appended; NOT deployable
       (insufficient sample, or divergence above threshold).
    2  could not compute (recipe paths missing, no L3 grader matching id,
       no usable annotations, etc.).
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

_THIS_DIR = Path(__file__).resolve().parent
if str(_THIS_DIR) not in sys.path:
    sys.path.insert(0, str(_THIS_DIR))

from lib import (  # noqa: E402
    AnnotationStore,
    AnthropicJudge,
    CalibrationOutcome,
    Judge,
    L3Grader,
    StubJudge,
    append_calibration_record,
    load_graders,
    run_calibration,
)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("recipe", type=Path, help="Path to a recipe directory.")
    parser.add_argument(
        "--grader-id",
        required=True,
        help="The L3 grader id in graders.yaml whose judge to calibrate.",
    )
    parser.add_argument(
        "--judge",
        choices=["anthropic", "stub"],
        default="anthropic",
        help="Judge backend. Default: anthropic (needs ANTHROPIC_API_KEY).",
    )
    parser.add_argument(
        "--min-sample-size",
        type=int,
        default=20,
        help="Minimum usable annotation count to mark deployable. Default 20.",
    )
    parser.add_argument(
        "--max-divergence",
        type=float,
        default=None,
        help="Override the grader's max_divergence_from_l2 for this run.",
    )
    parser.add_argument("--note", default=None, help="Free-form note recorded in the log.")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Compute the record and print it; do not append to calibration.jsonl.",
    )
    args = parser.parse_args(argv)

    evals_dir = args.recipe / "evals"
    if not evals_dir.is_dir():
        print(f"error: {args.recipe} has no evals/ directory", file=sys.stderr)
        return 2

    graders = load_graders(evals_dir / "graders.yaml")
    grader = next((g for g in graders.by_level("L3") if g.id == args.grader_id), None)
    if grader is None:
        print(
            f"error: no L3 grader with id {args.grader_id!r} in {evals_dir}/graders.yaml",
            file=sys.stderr,
        )
        return 2
    assert isinstance(grader, L3Grader)

    rubric_path = evals_dir / grader.rubric
    if not rubric_path.is_file():
        print(f"error: rubric not found at {rubric_path}", file=sys.stderr)
        return 2

    annotations = AnnotationStore(evals_dir / "annotations")
    completed = annotations.list_completed()
    if not completed:
        print(
            f"error: no completed annotations in {annotations.directory}; "
            "run `python eval-bench/annotate.py review ...` to grade samples first",
            file=sys.stderr,
        )
        return 2

    judge = _build_judge(args.judge)
    outcome = run_calibration(
        grader=grader,
        annotations_store=annotations,
        judge=judge,
        rubric_path=rubric_path,
        min_sample_size=args.min_sample_size,
        max_divergence_from_l2=args.max_divergence,
        notes=args.note,
    )

    _print_outcome(outcome, args.grader_id, args.min_sample_size)

    if args.dry_run:
        print("\n--dry-run set; record NOT appended to calibration.jsonl")
    else:
        append_calibration_record(evals_dir / "calibration.jsonl", outcome.record)
        print(f"\nrecord appended to {evals_dir / 'calibration.jsonl'}")

    return 0 if outcome.record.deployed else 1


def _build_judge(name: str) -> Judge:
    if name == "stub":
        return StubJudge.always("pass", evidence="stub calibration judge")
    if name == "anthropic":
        return AnthropicJudge()
    raise ValueError(f"unknown judge {name!r}")


def _print_outcome(outcome: CalibrationOutcome, grader_id: str, min_sample_size: int) -> None:
    record = outcome.record
    print(f"grader:        {grader_id}")
    print(f"judge_model:   {record.judge_model}")
    print(f"usable pairs:  {len(outcome.pairs)} (min {min_sample_size})")
    print(f"agreement:     {record.agreement:.3f}")
    if record.cohen_kappa is not None:
        print(f"cohen_kappa:   {record.cohen_kappa:.3f}")
    print(f"deployed:      {record.deployed}")
    if record.divergence_breakdown:
        print("\ndisagreements (sme_vs_judge → fraction of pairs):")
        for k, v in sorted(record.divergence_breakdown.items()):
            print(f"  {k}: {v:.3f}")
    if outcome.skipped:
        print(f"\nskipped {len(outcome.skipped)} annotation(s):")
        for aid, reason in outcome.skipped[:10]:
            print(f"  {aid}: {reason}")
        if len(outcome.skipped) > 10:
            print(f"  … and {len(outcome.skipped) - 10} more")


if __name__ == "__main__":
    sys.exit(main())
