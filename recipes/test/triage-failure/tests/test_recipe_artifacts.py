"""Verify triage-failure's eval artifacts load cleanly through eval-bench.

Same shape as charter-sfdipot / oracles-fewhiccupps. Adds triage-specific
invariants: the L2 sample rate is higher than the other recipes (stakes),
fabrication and fix-proposal failure modes must be high severity, the
L3 judge rubric must contain the two hard rules.
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


def test_recipe_yaml_declares_required_parameters() -> None:
    text = (RECIPE_DIR / "recipe.yaml").read_text(encoding="utf-8")
    assert "failure_summary" in text
    assert "stack_trace" in text


def test_recipe_yaml_states_no_fix_proposals() -> None:
    """The recipe's instructions must explicitly forbid proposing code
    changes — this is the recipe's defining constraint."""
    text = (RECIPE_DIR / "recipe.yaml").read_text(encoding="utf-8")
    text_lower = text.lower()
    # Look for any of the equivalent phrasings.
    assert any(
        phrase in text_lower
        for phrase in ["never propose a code change", "no fix", "do not propose", "investigation steps"]
    ), "recipe.yaml must explicitly forbid proposing fixes"


def test_recipe_yaml_lists_four_h2_sections() -> None:
    text = (RECIPE_DIR / "recipe.yaml").read_text(encoding="utf-8")
    for section in ["Failure summary", "Hypotheses", "Suggested next steps", "Out of scope"]:
        assert section in text, f"recipe.yaml must mention required section {section!r}"


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
            assert "fm-overhelpful-on-vague-input" in t.failure_modes_targeted, (
                f"negative task {t.id!r} must target fm-overhelpful-on-vague-input"
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
    assert regression, "no regression tasks yet"
    for t in regression:
        assert isinstance(t.expected, dict)
        assert t.expected.get("must_mention_any"), (
            f"regression task {t.id!r} needs must_mention_any hints"
        )


def test_failure_kind_axis_covers_diverse_kinds() -> None:
    """The recipe should be exercised against diverse failure kinds, not
    just one. Without this, Slice Explorer trends by failure_kind are
    uninterpretable."""
    tasks = load_tasks(EVALS / "tasks.jsonl")
    positive_kinds = {t.axes.get("failure_kind") for t in tasks if t.polarity == "positive"}
    # At least four distinct positive failure kinds — assertion / timeout /
    # race / regression / mocking / environment / input-validation are the
    # ones we ship.
    assert len(positive_kinds) >= 4, (
        f"positive tasks must cover at least 4 failure kinds; got {positive_kinds}"
    )


def test_required_axes_present_on_every_task() -> None:
    tasks = load_tasks(EVALS / "tasks.jsonl")
    required_axes = {"complexity", "language", "failure_kind"}
    for t in tasks:
        missing = required_axes - t.axes.keys()
        assert not missing, f"task {t.id!r} missing axes: {missing}"


# ---------- failure-modes.yaml ----------


def test_failure_modes_load() -> None:
    fm = load_failure_modes(EVALS / "failure-modes.yaml")
    ids = {m.id for m in fm.modes}
    expected_seed = {
        "fm-fabricates-stack-frame",
        "fm-jumps-to-fix",
        "fm-confident-without-evidence",
        "fm-ignores-recent-changes",
        "fm-overhelpful-on-vague-input",
        "fm-no-disconfirming-check",
        "fm-not-actionable-next-steps",
    }
    assert expected_seed.issubset(ids), f"missing seed modes: {expected_seed - ids}"


def test_high_severity_modes_set_correctly() -> None:
    """The four high-stakes failure modes must be marked severity=high.
    Engineers are going to act on this output; the rubric and runbook
    consume severity to prioritise reviewing."""
    fm = load_failure_modes(EVALS / "failure-modes.yaml")
    expected_high = {
        "fm-fabricates-stack-frame",
        "fm-jumps-to-fix",
        "fm-confident-without-evidence",
        "fm-overhelpful-on-vague-input",
    }
    for mode_id in expected_high:
        mode = fm.by_id(mode_id)
        assert mode is not None, f"missing failure mode {mode_id!r}"
        assert mode.severity == "high", (
            f"failure mode {mode_id!r} must be severity=high; got {mode.severity!r}"
        )


def test_active_failure_modes_referenced_or_acknowledged() -> None:
    """Active modes that no task targets and which aren't sub-patterns of
    a higher-level mode are decoration. Allow specific known sub-patterns
    to remain unreferenced (rubric-watched only) but require everything
    else to be linked from at least one task."""
    fm = load_failure_modes(EVALS / "failure-modes.yaml")
    tasks = load_tasks(EVALS / "tasks.jsonl")
    referenced = set()
    for t in tasks:
        referenced.update(t.failure_modes_targeted)
    active_ids = {m.id for m in fm.active()}
    orphaned = active_ids - referenced
    # fm-no-disconfirming-check and fm-not-actionable-next-steps are
    # rubric-watched (D3 / D4 in the SME rubric); they don't need a task
    # explicitly targeting them. Allow these.
    allowed_orphans = {"fm-no-disconfirming-check", "fm-not-actionable-next-steps"}
    assert orphaned <= allowed_orphans, (
        f"unexpected orphan failure modes: {orphaned - allowed_orphans}"
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


def test_g_triage_sections_marked_negate_on_polarity_negative() -> None:
    g = load_graders(EVALS / "graders.yaml")
    sections_grader = next(grader for grader in g.graders if grader.id == "g-triage-sections")
    assert sections_grader.negate_on_polarity_negative is True


def test_l2_sample_rate_higher_than_other_recipes() -> None:
    """Triage stakes are larger than charter / oracles, so the L2 sample
    rate is set higher (0.15 vs 0.10 / 0.05). This is a deliberate
    decision documented in the runbook; pin it so it doesn't silently
    drift back to the recipe-template default."""
    g = load_graders(EVALS / "graders.yaml")
    l2 = next(grader for grader in g.graders if grader.level == "L2")
    assert l2.sample_rate >= 0.15, (
        f"triage L2 sample_rate must be >= 0.15 (higher stakes); got {l2.sample_rate}"
    )


def test_l3_judge_targets_correct_rubric() -> None:
    g = load_graders(EVALS / "graders.yaml")
    judge = next(grader for grader in g.graders if grader.id == "g-triage-judge")
    assert judge.rubric == "rubrics/judge_triage_quality.md"
    assert (EVALS / judge.rubric).is_file()
    rubric_text = (EVALS / judge.rubric).read_text(encoding="utf-8")
    # The triage rubric has TWO hard rules — fabricated frames AND fix
    # proposals. Pin them both.
    assert "fabricated frame" in rubric_text.lower()
    assert "code change" in rubric_text.lower() or "fix" in rubric_text.lower()


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
    assert "L3 g-triage-judge: skipped" in out


# ---------- L1 graders accept the right shapes for this recipe ----------


GRADER_RUNNERS = REPO_ROOT / "eval-bench" / "grader_runners"


def _run_triage_sections_grader(output: str) -> subprocess.CompletedProcess:
    import json
    return subprocess.run(
        [
            sys.executable,
            str(GRADER_RUNNERS / "markdown_sections.py"),
            "--required",
            "Failure summary,Hypotheses,Suggested next steps,Out of scope",
        ],
        input=json.dumps({"output": output, "task": {}}),
        capture_output=True,
        text=True,
        check=False,
    )


def test_full_triage_passes_l1_sections_grader() -> None:
    full = (
        "## Failure summary\nstub\n"
        "## Hypotheses\nstub\n"
        "## Suggested next steps\nstub\n"
        "## Out of scope\nstub\n"
    )
    res = _run_triage_sections_grader(full)
    assert res.returncode == 0


def test_triage_missing_out_of_scope_fails_l1_sections_grader() -> None:
    incomplete = (
        "## Failure summary\nstub\n"
        "## Hypotheses\nstub\n"
        "## Suggested next steps\nstub\n"
    )  # Out of scope deliberately missing
    res = _run_triage_sections_grader(incomplete)
    assert res.returncode == 1
    assert "Out of scope" in res.stdout


def test_refusal_composes_to_pass_on_negative_task() -> None:
    """End-to-end: a refusal output, run through L1 graders + composition
    on a real negative task from tasks.jsonl, must compose to passed=True."""
    from lib.composition import GraderOutcome, compose_trial_pass

    g = load_graders(EVALS / "graders.yaml")
    tasks = load_tasks(EVALS / "tasks.jsonl")
    negative = next(t for t in tasks if t.polarity == "negative")

    outcomes = [
        GraderOutcome(grader_id="g-output-shape", passed=True),
        GraderOutcome(grader_id="g-triage-sections", passed=False),
        GraderOutcome(
            grader_id="g-triage-sme-review",
            passed=False, skipped=True, skip_reason="not sampled",
        ),
        GraderOutcome(
            grader_id="g-triage-judge",
            passed=False, skipped=True, skip_reason="no calibration",
        ),
    ]
    graders_by_id = {grader.id: grader for grader in g.graders}
    passed, evidence = compose_trial_pass(outcomes, graders_by_id, negative)
    assert passed is True, f"refusal should compose to pass; evidence={evidence}"
    assert "polarity-inverted" in evidence["g-triage-sections"]


def test_triage_output_for_negative_task_composes_to_fail() -> None:
    """If the recipe triaged inadequate inputs instead of refusing, both
    L1 graders' raw verdicts pass — but with the inversion, g-triage-sections
    becomes effective fail, and the trial fails."""
    from lib.composition import GraderOutcome, compose_trial_pass

    g = load_graders(EVALS / "graders.yaml")
    tasks = load_tasks(EVALS / "tasks.jsonl")
    negative = next(t for t in tasks if t.polarity == "negative")

    outcomes = [
        GraderOutcome(grader_id="g-output-shape", passed=True),
        GraderOutcome(grader_id="g-triage-sections", passed=True),
        GraderOutcome(
            grader_id="g-triage-sme-review",
            passed=False, skipped=True, skip_reason="not sampled",
        ),
        GraderOutcome(
            grader_id="g-triage-judge",
            passed=False, skipped=True, skip_reason="no calibration",
        ),
    ]
    graders_by_id = {grader.id: grader for grader in g.graders}
    passed, evidence = compose_trial_pass(outcomes, graders_by_id, negative)
    assert passed is False
    assert "polarity-inverted" in evidence["g-triage-sections"]
    assert "raw=pass" in evidence["g-triage-sections"]
