#!/usr/bin/env python3
"""run_kpass.py — eval-bench harness CLI.

Loads a recipe's eval suite (tasks + failure-modes + graders), runs each task
k times via the configured RecipeRunner, dispatches the recipe output to each
L1 grader, composes the per-grader outcomes into a per-trial pass/fail using
polarity-aware composition, and reports pass@k and pass^k aggregated across
the recipe and sliced by every recorded axis.

Usage:
    python eval-bench/run_kpass.py --recipe recipes/test/charter-sfdipot --k 5
    python eval-bench/run_kpass.py --recipe ... --k 1 --runner stub --dry-run
    python eval-bench/run_kpass.py --recipe ... --k 5 --tag regression

Runners:
    --runner goose  invoke 'goose run --recipe ... --params ...' (default)
    --runner stub   in-process stub that returns a fixed placeholder output
                    per trial. Useful for harness smoke without goose
                    installed; not for measuring recipe quality.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# Allow `python eval-bench/run_kpass.py` from the repo root without install.
_THIS_DIR = Path(__file__).resolve().parent
if str(_THIS_DIR) not in sys.path:
    sys.path.insert(0, str(_THIS_DIR))

from lib import (  # noqa: E402  (import after sys.path tweak)
    GooseSubprocessRunner,
    GraderOutcome,
    L1Grader,
    L3Grader,
    RecipeRunner,
    ResultsStore,
    RunResult,
    StubRunner,
    Task,
    compose_trial_pass,
    compute_passk,
    grade_one,
    load_failure_modes,
    load_graders,
    load_tasks,
    passk_by_slice,
)
from lib.graders import is_l3_calibrated  # noqa: E402
from lib.kpass import TrialResult  # noqa: E402

REPO_ROOT = _THIS_DIR.parent


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
        "--runner",
        choices=["goose", "stub"],
        default="goose",
        help="Recipe runner. 'goose' invokes the goose binary as a subprocess; "
             "'stub' returns a placeholder output per trial (development smoke only).",
    )
    parser.add_argument(
        "--store",
        type=Path,
        default=None,
        help="Override SQLite results store path (default ~/.skein/eval-bench.sqlite).",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=REPO_ROOT,
        help=f"Repo root used to resolve grader runner paths. Default: {REPO_ROOT}",
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

    l3_status: dict[str, tuple[bool, str]] = {}
    for g in graders.by_level("L3"):
        assert isinstance(g, L3Grader)
        l3_status[g.id] = is_l3_calibrated(g, calibration_log)

    print(f"recipe:     {recipe_dir}")
    print(f"runner:     {args.runner}")
    print(f"tasks:      {len(tasks)} (after --tag {args.tag} filter)")
    print(f"k:          {args.k}")
    print(f"failure modes: {len(failure_modes.modes)} ({len(failure_modes.active())} active)")
    print(
        f"graders:    L1={len(graders.by_level('L1'))} "
        f"L2={len(graders.by_level('L2'))} L3={len(graders.by_level('L3'))}"
    )
    for gid, (ok, reason) in l3_status.items():
        marker = "ok" if ok else "skipped"
        print(f"  L3 {gid}: {marker} ({reason})")

    if args.dry_run:
        print("\n--dry-run set; not executing the recipe.")
        return 0

    runner = _build_runner(args.runner)
    store = ResultsStore(path=args.store)
    run_id = store.start_run(
        recipe=str(recipe_dir),
        k=args.k,
        notes=f"runner={args.runner}; tag={args.tag}; "
              f"l3_skipped={[gid for gid, (ok, _) in l3_status.items() if not ok]}",
    )

    all_trials: list[TrialResult] = []
    graders_by_id = {g.id: g for g in graders.graders}

    for task in tasks:
        for trial_index in range(args.k):
            run_result = runner.run(recipe_dir, task.input)
            outcomes = _outcomes_for_trial(
                task=task,
                graders=graders,
                l3_status=l3_status,
                run_result=run_result,
                repo_root=args.repo_root,
            )
            passed, evidence = compose_trial_pass(outcomes, graders_by_id, task)

            store.record_trial(
                run_id=run_id,
                task_id=task.id,
                trial_index=trial_index,
                passed=passed,
                polarity=task.polarity,
                tags=task.tags,
                axes=task.axes,
                grader_scores={"evidence": evidence, "outcomes": [
                    {"id": o.grader_id, "passed": o.passed, "skipped": o.skipped, "score": o.score}
                    for o in outcomes
                ]},
                duration_ms=run_result.duration_ms or None,
                trace_id=run_result.trace_id,
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

    _print_summary(all_trials, tasks, args.k, graders.min_passk_target)

    target = graders.min_passk_target
    if target is not None:
        overall_outcomes = [t.passed for t in all_trials]
        _, overall_pow_k = compute_passk(overall_outcomes, k=args.k)
        if overall_pow_k < target:
            return 1
    return 0


def _build_runner(name: str) -> RecipeRunner:
    if name == "stub":
        # Phase 1 stub: produces a placeholder charter for any input. This
        # exercises the harness pipeline end-to-end without requiring goose
        # to be installed. It does NOT measure recipe quality.
        placeholder = "\n".join(f"## {s}\n**Mission:** stub" for s in [
            "Structure", "Function", "Data", "Interfaces", "Platform", "Operations", "Time"
        ])
        return StubRunner(lambda _recipe, _params: RunResult(output=placeholder))
    if name == "goose":
        return GooseSubprocessRunner()
    raise ValueError(f"unknown runner {name!r}")


def _outcomes_for_trial(
    *,
    task: Task,
    graders,
    l3_status: dict[str, tuple[bool, str]],
    run_result: RunResult,
    repo_root: Path,
) -> list[GraderOutcome]:
    """Build the GraderOutcome list for one trial.

    If the recipe runner failed, every grader is reported skipped with the
    runner error. Otherwise: L1 graders are dispatched and graded;
    L2 graders are skipped (sampled review is not yet automated by the
    harness); L3 graders are skipped per their calibration status (real
    judge invocation lands in a follow-up).
    """
    outcomes: list[GraderOutcome] = []

    if run_result.error:
        for g in graders.graders:
            outcomes.append(
                GraderOutcome(
                    grader_id=g.id,
                    passed=False,
                    skipped=True,
                    skip_reason=f"recipe runner failed: {run_result.error}",
                )
            )
        return outcomes

    for g in graders.by_level("L1"):
        assert isinstance(g, L1Grader)
        outcomes.append(grade_one(g, run_result.output, task, repo_root=repo_root))

    for g in graders.by_level("L2"):
        # L2 sampling is not yet wired up by the harness; surface the gap
        # cleanly rather than silently treating L2 as always-pass.
        outcomes.append(
            GraderOutcome(
                grader_id=g.id,
                passed=False,
                skipped=True,
                skip_reason="L2 sampling not yet automated by the harness",
            )
        )

    for g in graders.by_level("L3"):
        ok, reason = l3_status.get(g.id, (False, "no calibration check performed"))
        if ok:
            outcomes.append(
                GraderOutcome(
                    grader_id=g.id,
                    passed=False,
                    skipped=True,
                    skip_reason="L3 judge invocation not yet wired (calibration ok but runner pending)",
                )
            )
        else:
            outcomes.append(
                GraderOutcome(
                    grader_id=g.id,
                    passed=False,
                    skipped=True,
                    skip_reason=reason,
                )
            )

    return outcomes


def _print_summary(
    all_trials: list[TrialResult],
    tasks: list[Task],
    k: int,
    target: float | None,
) -> None:
    overall_outcomes = [t.passed for t in all_trials]
    overall_at_k, overall_pow_k = compute_passk(overall_outcomes, k=k)

    print("\nresults")
    print("-------")
    print(f"overall: pass@{k} = {overall_at_k:.3f}   pass^{k} = {overall_pow_k:.3f}")

    if target is not None:
        marker = "OK" if overall_pow_k >= target else "FAIL"
        print(f"target:  pass^{k} >= {target:.3f}   [{marker}]")

    axis_keys: set[str] = set()
    for t in tasks:
        axis_keys.update(t.axes.keys())
    for axis in sorted(axis_keys):
        slices = passk_by_slice(all_trials, axis, k=k)
        if not slices:
            continue
        print(f"\nslice by {axis}:")
        for value, (at_k, pow_k) in sorted(slices.items()):
            print(f"  {value}: pass@{k} = {at_k:.3f}   pass^{k} = {pow_k:.3f}")


if __name__ == "__main__":
    sys.exit(main())
