#!/usr/bin/env python3
"""run_once.py — single-shot ad-hoc recipe execution.

For when you want to apply a recipe to *one* feature brief / target /
observation right now, without editing tasks.jsonl. Reads parameters from
the command line (or a JSON file), invokes the recipe via the configured
runner, optionally runs L1 graders against the output, and prints the
recipe's output to stdout (so it pipes cleanly into a file or another tool).

Usage:
    # Charter from a brief
    python eval-bench/run_once.py recipes/test/charter-sfdipot \\
        --input feature_brief="Add a /healthz endpoint that returns 200..."

    # Long brief from a file
    python eval-bench/run_once.py recipes/test/charter-sfdipot \\
        --input feature_brief=@docs/spec.md

    # All inputs at once as JSON
    python eval-bench/run_once.py recipes/test/oracles-fewhiccupps \\
        --inputs '{"target_description": "...", "target_kind": "endpoint"}'

    # Smoke without goose / Anthropic
    python eval-bench/run_once.py recipes/test/charter-sfdipot \\
        --input feature_brief="x" --runner stub --judge stub

By default ad-hoc runs do NOT write to the SQLite results store
(experiments are not measurements). Pass --store <path> to opt in.

Exit codes:
    0  the recipe ran AND the L1 graders all passed (or were skipped via --no-graders)
    1  the recipe ran but at least one L1 grader failed (composition layer's verdict)
    2  setup error (recipe missing, required parameter missing, runner failed)
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

import yaml

_THIS_DIR = Path(__file__).resolve().parent
if str(_THIS_DIR) not in sys.path:
    sys.path.insert(0, str(_THIS_DIR))

from lib import (  # noqa: E402
    AnthropicJudge,
    GooseSubprocessRunner,
    GraderOutcome,
    Judge,
    L1Grader,
    L3Grader,
    RecipeRunner,
    ResultsStore,
    RunResult,
    StubJudge,
    StubRunner,
    Task,
    compose_trial_pass,
    grade_one,
    load_graders,
)
from lib.graders import is_l3_calibrated  # noqa: E402

REPO_ROOT = _THIS_DIR.parent


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("recipe", type=Path, help="Path to a recipe directory.")
    parser.add_argument(
        "--input",
        action="append",
        default=[],
        metavar="KEY=VALUE",
        help=(
            "Recipe parameter, repeat for multiple. Use KEY=@path/to/file to "
            "read the value from a file. Mutually exclusive with --inputs."
        ),
    )
    parser.add_argument(
        "--inputs",
        default=None,
        help="All recipe parameters as a single JSON object. Mutually exclusive with --input.",
    )
    parser.add_argument(
        "--runner",
        choices=["goose", "stub"],
        default="goose",
        help="Recipe runner. Default: goose.",
    )
    parser.add_argument(
        "--judge",
        choices=["anthropic", "stub", "off"],
        default="off",
        help="L3 judge. Default: off (ad-hoc runs don't auto-charge LLM costs).",
    )
    parser.add_argument(
        "--no-graders",
        action="store_true",
        help="Skip L1 grader dispatch entirely. Just print the recipe output and exit 0.",
    )
    parser.add_argument(
        "--output-only",
        action="store_true",
        help="Print only the recipe output to stdout (no header / no grading summary).",
    )
    parser.add_argument(
        "--store",
        type=Path,
        default=None,
        help="If set, persist this run to a SQLite results store. Off by default for ad-hoc runs.",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=REPO_ROOT,
        help=f"Repo root used to resolve grader runner paths. Default: {REPO_ROOT}",
    )
    args = parser.parse_args(argv)

    recipe_dir: Path = args.recipe
    evals_dir = recipe_dir / "evals"
    if not (recipe_dir / "recipe.yaml").is_file():
        print(f"error: {recipe_dir} has no recipe.yaml", file=sys.stderr)
        return 2

    if args.input and args.inputs is not None:
        print("error: --input and --inputs are mutually exclusive", file=sys.stderr)
        return 2

    try:
        params = _collect_params(args.input, args.inputs)
        params = _validate_against_recipe(params, recipe_dir / "recipe.yaml")
    except ValueError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2

    runner = _build_runner(args.runner)
    judge = _build_judge(args.judge)

    if not args.output_only:
        print(f"# recipe: {recipe_dir}", file=sys.stderr)
        print(f"# runner: {args.runner}   judge: {args.judge}", file=sys.stderr)
        print(f"# inputs: {list(params.keys())}", file=sys.stderr)

    run_result = runner.run(recipe_dir, params)
    if run_result.error:
        print(f"\nerror: recipe runner failed: {run_result.error}", file=sys.stderr)
        return 2

    print(run_result.output)

    if args.no_graders:
        if args.store:
            _persist_unscored(args.store, recipe_dir, params, run_result)
        return 0

    if not (evals_dir / "graders.yaml").is_file():
        if not args.output_only:
            print("\n# (no graders.yaml in this recipe; skipping grading)", file=sys.stderr)
        return 0

    outcomes = _grade_output(
        evals_dir=evals_dir,
        run_result=run_result,
        params=params,
        repo_root=args.repo_root,
        judge=judge,
    )

    graders = load_graders(evals_dir / "graders.yaml")
    graders_by_id = {g.id: g for g in graders.graders}
    task = _adhoc_task(params)
    passed, evidence = compose_trial_pass(outcomes, graders_by_id, task)

    if args.store:
        _persist_scored(args.store, recipe_dir, params, run_result, outcomes, passed)

    if not args.output_only:
        _print_grading_summary(outcomes, evidence, passed, file=sys.stderr)

    return 0 if passed else 1


def _collect_params(input_args: list[str], inputs_json: str | None) -> dict[str, Any]:
    if inputs_json is not None:
        try:
            obj = json.loads(inputs_json)
        except json.JSONDecodeError as e:
            raise ValueError(f"--inputs must be valid JSON: {e}") from e
        if not isinstance(obj, dict):
            raise ValueError("--inputs must be a JSON object")
        return obj

    out: dict[str, Any] = {}
    for entry in input_args:
        if "=" not in entry:
            raise ValueError(f"--input expects KEY=VALUE, got {entry!r}")
        key, raw = entry.split("=", 1)
        key = key.strip()
        if not key:
            raise ValueError(f"--input has empty key in {entry!r}")
        if raw.startswith("@"):
            path = Path(raw[1:])
            if not path.is_file():
                raise ValueError(f"--input {key}=@{path} but file not found")
            out[key] = path.read_text(encoding="utf-8")
        else:
            out[key] = raw
    return out


def _validate_against_recipe(params: dict[str, Any], recipe_yaml: Path) -> dict[str, Any]:
    """Read recipe.yaml's `parameters` block and verify every required one is set.

    Pass through the params dict unchanged on success. Raises ValueError with
    a clear message naming the missing required keys.
    """
    with recipe_yaml.open("r", encoding="utf-8") as f:
        recipe = yaml.safe_load(f) or {}
    declared = recipe.get("parameters") or []
    required = {p.get("name") for p in declared if p.get("required")}
    declared_names = {p.get("name") for p in declared}
    missing = sorted(required - params.keys())
    if missing:
        raise ValueError(
            f"recipe {recipe_yaml.parent} requires {missing} but they were not provided. "
            f"Use --input <name>=<value> or --input <name>=@<file>."
        )
    unknown = sorted(params.keys() - declared_names) if declared_names else []
    if unknown:
        # Don't reject — the recipe author may use Jinja2 vars not declared as
        # parameters. Just note it on stderr unless --output-only.
        print(
            f"warning: parameters not declared in recipe.yaml: {unknown}",
            file=sys.stderr,
        )
    return params


def _build_runner(name: str) -> RecipeRunner:
    if name == "stub":
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
        return StubJudge.always("pass", evidence="stub judge")
    if name == "anthropic":
        return AnthropicJudge()
    raise ValueError(f"unknown judge {name!r}")


def _adhoc_task(params: dict[str, Any]) -> Task:
    """Build a synthetic Task representing this single ad-hoc invocation."""
    return Task(
        id="adhoc",
        description="ad-hoc run via run_once.py",
        input=params,
        polarity="positive",
        tags=["capability"],
        axes={},
    )


def _grade_output(
    *,
    evals_dir: Path,
    run_result: RunResult,
    params: dict[str, Any],
    repo_root: Path,
    judge: Judge | None,
) -> list[GraderOutcome]:
    graders = load_graders(evals_dir / "graders.yaml")
    task = _adhoc_task(params)
    outcomes: list[GraderOutcome] = []

    for g in graders.by_level("L1"):
        assert isinstance(g, L1Grader)
        outcomes.append(grade_one(g, run_result.output, task, repo_root=repo_root))

    for g in graders.by_level("L2"):
        outcomes.append(
            GraderOutcome(
                grader_id=g.id, passed=False, skipped=True,
                skip_reason="L2 sampling not applicable to ad-hoc runs",
            )
        )

    calibration_log = evals_dir / "calibration.jsonl"
    for g in graders.by_level("L3"):
        assert isinstance(g, L3Grader)
        ok, reason = is_l3_calibrated(g, calibration_log)
        if not ok or judge is None:
            outcomes.append(
                GraderOutcome(
                    grader_id=g.id, passed=False, skipped=True,
                    skip_reason=reason if not ok else "L3 disabled (--judge off)",
                )
            )
            continue
        # Real L3 invocation reuses the same path run_kpass.py uses; for
        # parity we inline a small wrapper here rather than import a private.
        rubric_text = (evals_dir / g.rubric).read_text(encoding="utf-8")
        verdict = judge.judge(
            rubric_text,
            {
                "feature_brief": params.get("feature_brief"),
                "task_input": params,
                "task_expected": None,
                "task_polarity": "positive",
                "output": run_result.output,
            },
        )
        if verdict.error or verdict.verdict == "Unknown":
            outcomes.append(
                GraderOutcome(
                    grader_id=g.id, passed=False, skipped=True,
                    skip_reason=verdict.error or f"judge returned Unknown: {verdict.evidence}",
                )
            )
        else:
            outcomes.append(
                GraderOutcome(
                    grader_id=g.id,
                    passed=verdict.verdict == "pass",
                    score=1.0 if verdict.verdict == "pass" else 0.0,
                    details=verdict.evidence,
                )
            )

    return outcomes


def _persist_unscored(
    store_path: Path, recipe: Path, params: dict[str, Any], run_result: RunResult,
) -> None:
    store = ResultsStore(path=store_path)
    run_id = store.start_run(recipe=str(recipe), k=1, notes="run_once: --no-graders")
    store.record_trial(
        run_id=run_id, task_id="adhoc", trial_index=0, passed=True,
        polarity="positive", tags=["capability"], axes={},
        grader_scores={"adhoc_inputs": list(params.keys())},
        duration_ms=run_result.duration_ms or None,
        trace_id=run_result.trace_id,
    )
    store.finish_run(run_id)


def _persist_scored(
    store_path: Path,
    recipe: Path,
    params: dict[str, Any],
    run_result: RunResult,
    outcomes: list[GraderOutcome],
    passed: bool,
) -> None:
    store = ResultsStore(path=store_path)
    run_id = store.start_run(recipe=str(recipe), k=1, notes="run_once")
    store.record_trial(
        run_id=run_id, task_id="adhoc", trial_index=0, passed=passed,
        polarity="positive", tags=["capability"], axes={},
        grader_scores={
            "adhoc_inputs": list(params.keys()),
            "outcomes": [
                {"id": o.grader_id, "passed": o.passed, "skipped": o.skipped, "score": o.score}
                for o in outcomes
            ],
        },
        duration_ms=run_result.duration_ms or None,
        trace_id=run_result.trace_id,
    )
    store.finish_run(run_id)


def _print_grading_summary(
    outcomes: list[GraderOutcome],
    evidence: dict[str, str],
    passed: bool,
    file,
) -> None:
    print(file=file)
    print("# grading", file=file)
    for o in outcomes:
        marker = "skip" if o.skipped else ("pass" if o.passed else "fail")
        line = f"#   [{marker}] {o.grader_id}"
        if o.skipped:
            line += f" — {o.skip_reason}"
        elif o.details:
            line += f" — {o.details[:120]}"
        print(line, file=file)
    print(f"# overall: {'PASS' if passed else 'FAIL'}", file=file)


if __name__ == "__main__":
    sys.exit(main())
