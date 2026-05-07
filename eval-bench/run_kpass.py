#!/usr/bin/env python3
"""run_kpass.py — eval-bench harness CLI.

Loads a recipe's eval suite (tasks + failure-modes + graders), runs each task
k times, composes grader scores into a per-trial pass/fail, and reports
pass@k and pass^k aggregated across the recipe and sliced by every recorded axis.

Usage:
    python eval-bench/run_kpass.py --recipe recipes/test/charter-sfdipot --k 5
    python eval-bench/run_kpass.py --recipe ... --k 5 --tag regression
    python eval-bench/run_kpass.py --recipe ... --k 1 --dry-run

This is the Phase 0 scaffold. The recipe-execution path delegates to a
runner-stub that prints what it *would* execute and returns a placeholder
result; the real wiring into Goose's recipe runner lands with the first
Phase 1 recipe (`charter-sfdipot`).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

# Allow `python eval-bench/run_kpass.py` from the repo root without install.
_THIS_DIR = Path(__file__).resolve().parent
if str(_THIS_DIR) not in sys.path:
    sys.path.insert(0, str(_THIS_DIR))

from lib import (  # noqa: E402  (import after sys.path tweak)
    L3Grader,
    ResultsStore,
    Task,
    compute_passk,
    load_failure_modes,
    load_graders,
    load_tasks,
    passk_by_slice,
)
from lib.graders import is_l3_calibrated  # noqa: E402
from lib.kpass import TrialResult  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description="Skein eval-bench harness")
    parser.add_argument(
        "--recipe",
        required=True,
        type=Path,
        help="Path to a recipe directory (containing recipe.yaml and evals/).",
    )
    parser.add_argument("--k", type=int, default=5, help="Trials per task. Default 5.")
    parser.add_argument(
        "--tag",
        choices=["regression", "capability", "all"],
        default="all",
        help="Restrict to tasks with this tag. Default: all.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Validate recipe artifacts and print the run plan; do not execute the recipe.",
    )
    parser.add_argument(
        "--store",
        type=Path,
        default=None,
        help="Override SQLite results store path (default ~/.skein/eval-bench.sqlite).",
    )
    args = parser.parse_args()

    recipe_dir: Path = args.recipe
    evals_dir = recipe_dir / "evals"
    if not evals_dir.is_dir():
        print(f"error: {recipe_dir} has no evals/ directory", file=sys.stderr)
        return 2

    tasks = load_tasks(evals_dir / "tasks.jsonl")
    failure_modes = load_failure_modes(evals_dir / "failure-modes.yaml")
    graders = load_graders(evals_dir / "graders.yaml")
    calibration_log = evals_dir / "calibration.jsonl"

    if args.tag != "all":
        tasks = [t for t in tasks if args.tag in t.tags]
    if not tasks:
        print(f"error: no tasks match --tag {args.tag}", file=sys.stderr)
        return 2

    # Decide which L3 graders are calibrated. Skipped graders annotate the run rather than fail it.
    l3_status: dict[str, tuple[bool, str]] = {}
    for g in graders.by_level("L3"):
        assert isinstance(g, L3Grader)
        l3_status[g.id] = is_l3_calibrated(g, calibration_log)

    print(f"recipe:     {recipe_dir}")
    print(f"tasks:      {len(tasks)} (after --tag {args.tag} filter)")
    print(f"k:          {args.k}")
    print(f"failure modes: {len(failure_modes.modes)} ({len(failure_modes.active())} active)")
    print(f"graders:    L1={len(graders.by_level('L1'))} L2={len(graders.by_level('L2'))} L3={len(graders.by_level('L3'))}")
    for gid, (ok, reason) in l3_status.items():
        marker = "ok" if ok else "skipped"
        print(f"  L3 {gid}: {marker} ({reason})")

    if args.dry_run:
        print("\n--dry-run set; not executing the recipe.")
        return 0

    store = ResultsStore(path=args.store)
    run_id = store.start_run(
        recipe=str(recipe_dir),
        k=args.k,
        notes=f"tag={args.tag}; l3_skipped={[gid for gid, (ok, _) in l3_status.items() if not ok]}",
    )

    all_trials: list[TrialResult] = []
    for task in tasks:
        for trial_index in range(args.k):
            passed, grader_scores = _execute_one_trial(task, graders, l3_status)
            store.record_trial(
                run_id=run_id,
                task_id=task.id,
                trial_index=trial_index,
                passed=passed,
                polarity=task.polarity,
                tags=task.tags,
                axes=task.axes,
                grader_scores=grader_scores,
            )
            all_trials.append(
                TrialResult(
                    task_id=task.id,
                    trial_index=trial_index,
                    passed=passed,
                    axes=task.axes,
                )
            )
    store.finish_run(run_id)

    # Aggregate.
    by_task: dict[str, list[bool]] = {}
    for tr in all_trials:
        by_task.setdefault(tr.task_id, []).append(tr.passed)

    overall_outcomes = [t for outcomes in by_task.values() for t in outcomes]
    overall_at_k, overall_pow_k = compute_passk(overall_outcomes, k=args.k)

    print("\nresults")
    print("-------")
    print(f"overall: pass@{args.k} = {overall_at_k:.3f}   pass^{args.k} = {overall_pow_k:.3f}")

    target = graders.min_passk_target
    if target is not None:
        marker = "OK" if overall_pow_k >= target else "FAIL"
        print(f"target:  pass^{args.k} >= {target:.3f}   [{marker}]")

    # Slice by every axis seen.
    axis_keys: set[str] = set()
    for t in tasks:
        axis_keys.update(t.axes.keys())
    for axis in sorted(axis_keys):
        slices = passk_by_slice(all_trials, axis, k=args.k)
        if not slices:
            continue
        print(f"\nslice by {axis}:")
        for value, (at_k, pow_k) in sorted(slices.items()):
            print(f"  {value}: pass@{args.k} = {at_k:.3f}   pass^{args.k} = {pow_k:.3f}")

    if target is not None and overall_pow_k < target:
        return 1
    return 0


def _execute_one_trial(
    task: Task,
    graders: Any,
    l3_status: dict[str, tuple[bool, str]],
) -> tuple[bool, dict[str, Any]]:
    """Stub trial executor.

    Phase 0 scaffold: marks every trial as passed and returns empty grader scores.
    The real implementation, landing with the first Phase 1 recipe, will:
      1. Invoke the goose recipe runner with `task.input`.
      2. Capture the output and the trace id (Langfuse).
      3. Run each L1 grader (subprocess).
      4. Run each calibrated L3 grader (model call against the rubric).
      5. Optionally enqueue an L2 sample.
      6. Compose weighted scores into pass/fail.
    """
    _ = (task, graders, l3_status)  # placeholder to silence linters
    return True, {"_stub": True}


if __name__ == "__main__":
    sys.exit(main())
