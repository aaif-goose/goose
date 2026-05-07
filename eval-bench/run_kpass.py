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

Judges (L3 LLM-as-judge):
    --judge anthropic  POST to the Anthropic Messages API; auth via
                       ANTHROPIC_API_KEY (default).
    --judge stub       always returns 'pass' (development smoke).
    --judge off        disable L3 judging entirely; L3 graders skip with
                       reason 'L3 judging disabled'.
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
    AnthropicJudge,
    GooseSubprocessRunner,
    GraderOutcome,
    Judge,
    JudgeVerdict,
    L1Grader,
    L3Grader,
    RecipeRunner,
    ResultsStore,
    RunResult,
    StubJudge,
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
        "--judge",
        choices=["anthropic", "stub", "off"],
        default="anthropic",
        help="L3 judge backend. 'anthropic' uses the Messages API "
             "(needs ANTHROPIC_API_KEY); 'stub' always returns pass "
             "(development smoke); 'off' disables L3 judging entirely.",
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
    print(f"judge:      {args.judge}")
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
    judge = _build_judge(args.judge)
    store = ResultsStore(path=args.store)
    run_id = store.start_run(
        recipe=str(recipe_dir),
        k=args.k,
        notes=f"runner={args.runner}; judge={args.judge}; tag={args.tag}; "
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
                judge=judge,
                evals_dir=evals_dir,
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


def _build_judge(name: str) -> Judge | None:
    if name == "off":
        return None
    if name == "stub":
        # Default stub: always pass. Useful for harness smoke without
        # touching the Anthropic API. Does NOT measure recipe quality.
        return StubJudge.always("pass", evidence="stub judge")
    if name == "anthropic":
        return AnthropicJudge()
    raise ValueError(f"unknown judge {name!r}")


def _outcomes_for_trial(
    *,
    task: Task,
    graders,
    l3_status: dict[str, tuple[bool, str]],
    run_result: RunResult,
    repo_root: Path,
    judge: Judge | None,
    evals_dir: Path,
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
        assert isinstance(g, L3Grader)
        ok, reason = l3_status.get(g.id, (False, "no calibration check performed"))
        if not ok:
            outcomes.append(
                GraderOutcome(grader_id=g.id, passed=False, skipped=True, skip_reason=reason)
            )
            continue
        if judge is None:
            outcomes.append(
                GraderOutcome(
                    grader_id=g.id,
                    passed=False,
                    skipped=True,
                    skip_reason="L3 judging disabled (--judge off)",
                )
            )
            continue
        outcomes.append(_invoke_l3_judge(g, judge, run_result, task, evals_dir))

    return outcomes


def _invoke_l3_judge(
    grader: L3Grader,
    judge: Judge,
    run_result: RunResult,
    task: Task,
    evals_dir: Path,
) -> GraderOutcome:
    """Run a calibrated L3 judge for one trial.

    Loads the judge's rubric (path is relative to evals_dir) and invokes the
    judge with the rubric + a payload (feature inputs from the task, recipe
    output). Translates the verdict into a GraderOutcome:

      pass    -> passed=True
      fail    -> passed=False
      Unknown -> skipped=True (we do not guess; humans review)
      error   -> skipped=True with the judge's error preserved
    """
    rubric_path = evals_dir / grader.rubric
    try:
        rubric_text = rubric_path.read_text(encoding="utf-8")
    except OSError as e:
        return GraderOutcome(
            grader_id=grader.id,
            passed=False,
            skipped=True,
            skip_reason=f"could not read rubric {rubric_path}: {e}",
        )

    payload = {
        "feature_brief": task.input.get("feature_brief"),
        "task_input": task.input,
        "task_expected": task.expected,
        "task_polarity": task.polarity,
        "output": run_result.output,
    }
    verdict = judge.judge(rubric_text, payload)

    if verdict.error:
        return GraderOutcome(
            grader_id=grader.id,
            passed=False,
            skipped=True,
            skip_reason=f"judge error: {verdict.error}",
        )
    if verdict.verdict == "Unknown":
        return GraderOutcome(
            grader_id=grader.id,
            passed=False,
            skipped=True,
            skip_reason=f"judge returned Unknown: {verdict.evidence or 'no evidence'}",
        )
    return GraderOutcome(
        grader_id=grader.id,
        passed=verdict.verdict == "pass",
        score=1.0 if verdict.verdict == "pass" else 0.0,
        details=verdict.evidence,
    )


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
