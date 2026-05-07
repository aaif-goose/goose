"""Verify oracles-fewhiccupps's eval artifacts load cleanly through eval-bench.

Same shape as charter-sfdipot's recipe-artifact tests — guards the *artifacts*
themselves (schema, two-sided coverage, taxonomy hygiene, grader composition,
oracle-specific Statutes invariant). Does not run the LLM.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

from lib.failure_modes import load_failure_modes
from lib.graders import load_graders
from lib.tasks import load_tasks

RECIPE_DIR = Path(__file__).resolve().parents[1]
EVALS = RECIPE_DIR / "evals"
REPO_ROOT = RECIPE_DIR.parents[2]


# ---------- recipe.yaml ----------


def test_recipe_yaml_exists() -> None:
    assert (RECIPE_DIR / "recipe.yaml").is_file()


def test_recipe_yaml_declares_target_description_parameter() -> None:
    text = (RECIPE_DIR / "recipe.yaml").read_text(encoding="utf-8")
    assert "target_description" in text
    # Recipe instructions must explicitly say not to fabricate.
    assert "fabric" in text.lower() or "N/A" in text
    # Recipe must instruct refusal for vague targets.
    assert "refuse" in text.lower() or "refusal" in text.lower()
    # Recipe must list all 11 oracle names somewhere in instructions.
    for oracle in [
        "Familiar problems", "Explainability", "World", "History", "Image",
        "Comparable products", "Claims", "User expectations", "Product",
        "Purpose", "Statutes",
    ]:
        assert oracle in text, f"recipe.yaml must mention oracle {oracle!r}"


# ---------- tasks.jsonl ----------


def test_tasks_load_cleanly() -> None:
    tasks = load_tasks(EVALS / "tasks.jsonl")
    assert len(tasks) >= 10


def test_tasks_are_two_sided() -> None:
    tasks = load_tasks(EVALS / "tasks.jsonl")
    polarities = {t.polarity for t in tasks}
    assert polarities == {"positive", "negative"}


def test_negative_tasks_target_overhelpful_failure_mode() -> None:
    tasks = load_tasks(EVALS / "tasks.jsonl")
    for t in tasks:
        if t.polarity == "negative":
            assert "fm-overhelpful-on-vague-target" in t.failure_modes_targeted, (
                f"negative task {t.id!r} must target fm-overhelpful-on-vague-target"
            )


def test_negative_tasks_declare_refusal_contract() -> None:
    tasks = load_tasks(EVALS / "tasks.jsonl")
    for t in tasks:
        if t.polarity == "negative":
            assert isinstance(t.expected, dict)
            assert t.expected.get("contract") == "refusal-with-clarifying-questions"


def test_regression_tasks_have_must_mention_hints() -> None:
    tasks = load_tasks(EVALS / "tasks.jsonl")
    regression = [t for t in tasks if "regression" in t.tags]
    assert regression, "no regression tasks yet — capability suite hasn't graduated anything"
    for t in regression:
        assert isinstance(t.expected, dict)
        assert t.expected.get("must_mention_any"), (
            f"regression task {t.id!r} needs must_mention_any hints"
        )


def test_axes_recorded_for_slice_explorer() -> None:
    """Every task must record at least `complexity`, `language`, and
    `target_kind` axes so the Slice Explorer can break results down."""
    tasks = load_tasks(EVALS / "tasks.jsonl")
    required_axes = {"complexity", "language", "target_kind"}
    for t in tasks:
        missing = required_axes - t.axes.keys()
        assert not missing, f"task {t.id!r} missing axes: {missing}"


def test_target_kinds_cover_function_endpoint_screen() -> None:
    """The recipe should be exercised against all three target kinds it
    supports, not just one. Without this, Slice Explorer trends by target
    kind are uninterpretable."""
    tasks = load_tasks(EVALS / "tasks.jsonl")
    positive_kinds = {t.axes.get("target_kind") for t in tasks if t.polarity == "positive"}
    assert {"function", "endpoint", "screen"}.issubset(positive_kinds), (
        f"positive tasks must cover function/endpoint/screen; got {positive_kinds}"
    )


# ---------- failure-modes.yaml ----------


def test_failure_modes_load() -> None:
    fm = load_failure_modes(EVALS / "failure-modes.yaml")
    ids = {m.id for m in fm.modes}
    expected_seed = {
        "fm-fabrication",
        "fm-skips-oracles-silently",
        "fm-mislabels-oracle",
        "fm-not-actionable",
        "fm-overhelpful-on-vague-target",
        "fm-overstuffs-N/A-sections",
        "fm-statutes-hallucination",
    }
    assert expected_seed.issubset(ids), f"missing seed modes: {expected_seed - ids}"


def test_statutes_hallucination_is_high_severity() -> None:
    """Cited statutes / RFCs that don't apply or don't exist are dangerous;
    the failure mode must be marked high severity."""
    fm = load_failure_modes(EVALS / "failure-modes.yaml")
    statutes_mode = fm.by_id("fm-statutes-hallucination")
    assert statutes_mode is not None
    assert statutes_mode.severity == "high"


def test_every_failure_mode_referenced_by_at_least_one_task() -> None:
    """Active modes that no task targets are decoration. Every active mode
    should appear in at least one task's failure_modes_targeted list."""
    fm = load_failure_modes(EVALS / "failure-modes.yaml")
    tasks = load_tasks(EVALS / "tasks.jsonl")
    referenced = set()
    for t in tasks:
        referenced.update(t.failure_modes_targeted)
    active_ids = {m.id for m in fm.active()}
    orphaned = active_ids - referenced
    # fm-statutes-hallucination is currently not directly tagged on a task —
    # it's a sub-pattern of fm-fabrication that the L3 judge rubric watches
    # for. Allow it; require everything else to be linked.
    assert orphaned <= {"fm-statutes-hallucination"}, (
        f"unexpected orphan failure modes: {orphaned}"
    )


# ---------- graders.yaml ----------


def test_graders_load_with_l1_floor_and_l3_judge() -> None:
    g = load_graders(EVALS / "graders.yaml")
    assert g.min_passk_target == 0.80
    levels = {grader.level for grader in g.graders}
    assert {"L1", "L2", "L3"}.issubset(levels)
    l1_runners = [grader.runner for grader in g.by_level("L1")]
    assert any("output_shape.py" in r for r in l1_runners)
    assert any("markdown_sections.py" in r for r in l1_runners)


def test_g_oracle_sections_marked_negate_on_polarity_negative() -> None:
    g = load_graders(EVALS / "graders.yaml")
    sections_grader = next(grader for grader in g.graders if grader.id == "g-oracle-sections")
    assert sections_grader.negate_on_polarity_negative is True


def test_g_output_shape_does_not_negate_on_polarity() -> None:
    g = load_graders(EVALS / "graders.yaml")
    shape_grader = next(grader for grader in g.graders if grader.id == "g-output-shape")
    assert shape_grader.negate_on_polarity_negative is False


def test_l3_judge_targets_correct_rubric() -> None:
    """The L3 judge rubric must be the oracle-specific one, not a copy-paste
    of the charter rubric."""
    g = load_graders(EVALS / "graders.yaml")
    judge = next(grader for grader in g.graders if grader.id == "g-oracle-judge")
    assert judge.rubric == "rubrics/judge_oracle_quality.md"
    # The rubric must exist on disk.
    assert (EVALS / judge.rubric).is_file()
    rubric_text = (EVALS / judge.rubric).read_text(encoding="utf-8")
    # The oracle rubric has a Statutes-specific hard rule; confirm it's there.
    assert "Statutes" in rubric_text


# ---------- end-to-end harness smoke ----------


def test_dry_run_against_recipe_loads_cleanly() -> None:
    harness = REPO_ROOT / "eval-bench" / "run_kpass.py"
    result = subprocess.run(
        [sys.executable, str(harness), "--recipe", str(RECIPE_DIR), "--dry-run"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, f"dry-run failed:\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"
    out = result.stdout
    assert "tasks:" in out
    assert "L3 g-oracle-judge: skipped" in out


# ---------- L1 graders accept the right shapes for this recipe ----------


GRADER_RUNNERS = REPO_ROOT / "eval-bench" / "grader_runners"


def _run_oracle_sections_grader(output: str) -> subprocess.CompletedProcess:
    import json
    return subprocess.run(
        [
            sys.executable,
            str(GRADER_RUNNERS / "markdown_sections.py"),
            "--required",
            "Familiar problems,Explainability,World,History,Image,Comparable products,Claims,User expectations,Product,Purpose,Statutes",
        ],
        input=json.dumps({"output": output, "task": {}}),
        capture_output=True,
        text=True,
        check=False,
    )


def test_full_oracle_suite_passes_l1_sections_grader() -> None:
    full = "\n".join(f"## {s}\n**Oracle:** stub\n" for s in [
        "Familiar problems", "Explainability", "World", "History", "Image",
        "Comparable products", "Claims", "User expectations", "Product",
        "Purpose", "Statutes",
    ])
    res = _run_oracle_sections_grader(full)
    assert res.returncode == 0


def test_oracle_suite_missing_statutes_fails_l1_sections_grader() -> None:
    incomplete = "\n".join(f"## {s}" for s in [
        "Familiar problems", "Explainability", "World", "History", "Image",
        "Comparable products", "Claims", "User expectations", "Product",
        "Purpose",
    ])  # Statutes deliberately missing
    res = _run_oracle_sections_grader(incomplete)
    assert res.returncode == 1
    assert "Statutes" in res.stdout


def test_refusal_output_correctly_fails_l1_sections_grader_raw() -> None:
    """Same as charter-sfdipot: the runner's raw verdict on a refusal is
    fail (no oracle sections), and the composition layer inverts it to
    effective pass on negative-polarity tasks via negate_on_polarity_negative."""
    refusal = "I can't compose oracles for this target without more detail. What kind of system is it, what's the intended behaviour, and what are the user-facing claims?"
    res = _run_oracle_sections_grader(refusal)
    assert res.returncode == 1


def test_refusal_composes_to_pass_on_negative_task() -> None:
    """End-to-end: a refusal output, run through the L1 graders and the
    composition layer with this recipe's actual graders.yaml against a
    real negative task from tasks.jsonl, must compose to passed=True."""
    from lib.composition import GraderOutcome, compose_trial_pass

    g = load_graders(EVALS / "graders.yaml")
    tasks = load_tasks(EVALS / "tasks.jsonl")
    negative = next(t for t in tasks if t.polarity == "negative")

    # Simulate: shape passes (non-empty refusal), sections fails (no oracles),
    # L2 / L3 skipped.
    outcomes = [
        GraderOutcome(grader_id="g-output-shape", passed=True),
        GraderOutcome(grader_id="g-oracle-sections", passed=False),
        GraderOutcome(
            grader_id="g-oracle-sme-review",
            passed=False, skipped=True, skip_reason="not sampled",
        ),
        GraderOutcome(
            grader_id="g-oracle-judge",
            passed=False, skipped=True, skip_reason="no calibration",
        ),
    ]
    graders_by_id = {grader.id: grader for grader in g.graders}
    passed, evidence = compose_trial_pass(outcomes, graders_by_id, negative)
    assert passed is True, f"refusal should compose to pass; evidence={evidence}"
    assert "polarity-inverted" in evidence["g-oracle-sections"]


def test_oracle_output_for_negative_task_composes_to_fail() -> None:
    """If the recipe composed oracles for a vague target instead of refusing,
    both L1 graders' raw verdicts pass — but with the inversion
    g-oracle-sections becomes effective fail, and the trial fails."""
    from lib.composition import GraderOutcome, compose_trial_pass

    g = load_graders(EVALS / "graders.yaml")
    tasks = load_tasks(EVALS / "tasks.jsonl")
    negative = next(t for t in tasks if t.polarity == "negative")

    outcomes = [
        GraderOutcome(grader_id="g-output-shape", passed=True),
        GraderOutcome(grader_id="g-oracle-sections", passed=True),
        GraderOutcome(
            grader_id="g-oracle-sme-review",
            passed=False, skipped=True, skip_reason="not sampled",
        ),
        GraderOutcome(
            grader_id="g-oracle-judge",
            passed=False, skipped=True, skip_reason="no calibration",
        ),
    ]
    graders_by_id = {grader.id: grader for grader in g.graders}
    passed, evidence = compose_trial_pass(outcomes, graders_by_id, negative)
    assert passed is False
    assert "polarity-inverted" in evidence["g-oracle-sections"]
    assert "raw=pass" in evidence["g-oracle-sections"]
