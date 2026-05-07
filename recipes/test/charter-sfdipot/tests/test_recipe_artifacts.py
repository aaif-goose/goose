"""Verify charter-sfdipot's eval artifacts load cleanly through eval-bench.

These are not LLM-output tests — those require running the recipe and a
calibrated judge. These guard the *artifacts* themselves: schema-shape,
two-sided coverage, taxonomy hygiene, grader composition.
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


# ---------- recipe.yaml exists & has the expected shape ----------


def test_recipe_yaml_exists() -> None:
    assert (RECIPE_DIR / "recipe.yaml").is_file()


def test_recipe_yaml_declares_feature_brief_parameter() -> None:
    text = (RECIPE_DIR / "recipe.yaml").read_text(encoding="utf-8")
    assert "feature_brief" in text
    # Recipe instructions must explicitly say not to fabricate.
    assert "fabric" in text.lower() or "N/A" in text
    # Recipe must instruct refusal for vague briefs.
    assert "refuse" in text.lower() or "refusal" in text.lower()


# ---------- tasks.jsonl ----------


def test_tasks_load_cleanly() -> None:
    tasks = load_tasks(EVALS / "tasks.jsonl")
    assert len(tasks) >= 10, "seed set should contain at least 10 tasks"


def test_tasks_are_two_sided() -> None:
    tasks = load_tasks(EVALS / "tasks.jsonl")
    polarities = {t.polarity for t in tasks}
    assert polarities == {"positive", "negative"}, (
        "charter-sfdipot must ship both positive and negative-polarity tasks; "
        "negative ones cover the refusal contract"
    )


def test_negative_tasks_target_overhelpful_failure_mode() -> None:
    """Every negative task must be tagged with the overhelpful failure mode —
    otherwise the harness can't tell which mode that task is meant to detect."""
    tasks = load_tasks(EVALS / "tasks.jsonl")
    for t in tasks:
        if t.polarity == "negative":
            assert "fm-overhelpful-on-vague-brief" in t.failure_modes_targeted, (
                f"negative task {t.id!r} must target fm-overhelpful-on-vague-brief"
            )


def test_negative_tasks_declare_refusal_contract() -> None:
    tasks = load_tasks(EVALS / "tasks.jsonl")
    for t in tasks:
        if t.polarity == "negative":
            assert isinstance(t.expected, dict)
            assert t.expected.get("contract") == "refusal-with-clarifying-questions", (
                f"negative task {t.id!r} expected.contract must be 'refusal-with-clarifying-questions'"
            )


def test_regression_tasks_have_must_mention_hints() -> None:
    """Regression tasks lock the recipe to specific brief details. Without
    must_mention_any hints we have no way to know whether the regression
    output actually addressed the brief."""
    tasks = load_tasks(EVALS / "tasks.jsonl")
    regression = [t for t in tasks if "regression" in t.tags]
    assert regression, "no regression tasks yet — capability suite hasn't graduated anything"
    for t in regression:
        assert isinstance(t.expected, dict)
        assert t.expected.get("must_mention_any"), f"regression task {t.id!r} needs must_mention_any hints"


def test_axes_recorded_for_slice_explorer() -> None:
    """Tasks must record at least `complexity`, `domain`, and `language` axes
    so the Slice Explorer can break results down."""
    tasks = load_tasks(EVALS / "tasks.jsonl")
    required_axes = {"complexity", "domain", "language"}
    for t in tasks:
        missing = required_axes - t.axes.keys()
        assert not missing, f"task {t.id!r} missing axes: {missing}"


# ---------- failure-modes.yaml ----------


def test_failure_modes_load() -> None:
    fm = load_failure_modes(EVALS / "failure-modes.yaml")
    ids = {m.id for m in fm.modes}
    expected_seed = {
        "fm-fabrication",
        "fm-skips-dimensions-silently",
        "fm-script-output",
        "fm-no-oracles-named",
        "fm-fakes-tactics",
        "fm-overhelpful-on-vague-brief",
        "fm-overstuffs-N/A-sections",
    }
    assert expected_seed.issubset(ids), f"missing seed modes: {expected_seed - ids}"


def test_every_failure_mode_referenced_by_at_least_one_task() -> None:
    """A failure mode that no task targets is decoration. Every active mode
    should appear in at least one task's failure_modes_targeted list."""
    fm = load_failure_modes(EVALS / "failure-modes.yaml")
    tasks = load_tasks(EVALS / "tasks.jsonl")
    referenced = set()
    for t in tasks:
        referenced.update(t.failure_modes_targeted)
    active_ids = {m.id for m in fm.active()}
    orphaned = active_ids - referenced
    assert not orphaned, f"active failure modes with no task targeting them: {orphaned}"


# ---------- graders.yaml ----------


def test_graders_load_with_l1_floor_and_l3_judge() -> None:
    g = load_graders(EVALS / "graders.yaml")
    assert g.min_passk_target == 0.80
    levels = {grader.level for grader in g.graders}
    assert {"L1", "L2", "L3"}.issubset(levels), (
        "charter-sfdipot must ship a full L1+L2+L3 composition"
    )
    l1_runners = [grader.runner for grader in g.by_level("L1")]
    assert any("output_shape.py" in r for r in l1_runners), "must include output_shape L1"
    assert any("markdown_sections.py" in r for r in l1_runners), "must include markdown_sections L1"


# ---------- end-to-end harness smoke ----------


def test_dry_run_against_recipe_loads_cleanly() -> None:
    """The complete artifact set must load through run_kpass.py --dry-run."""
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
    # The L3 judge must be reported skipped (no calibration log yet).
    assert "L3 g-charter-judge: skipped" in out


# ---------- L1 graders accept the right shapes ----------


GRADER_RUNNERS = REPO_ROOT / "eval-bench" / "grader_runners"


def _run_markdown_sections(output: str) -> subprocess.CompletedProcess:
    import json
    return subprocess.run(
        [
            sys.executable,
            str(GRADER_RUNNERS / "markdown_sections.py"),
            "--required",
            "Structure,Function,Data,Interfaces,Platform,Operations,Time",
        ],
        input=json.dumps({"output": output, "task": {}}),
        capture_output=True,
        text=True,
        check=False,
    )


def test_full_sfdipot_output_passes_l1_sections_grader() -> None:
    full = "\n".join(f"## {s}\n**Mission:** stub\n" for s in [
        "Structure", "Function", "Data", "Interfaces", "Platform", "Operations", "Time"
    ])
    res = _run_markdown_sections(full)
    assert res.returncode == 0


def test_charter_missing_time_section_fails_l1_sections_grader() -> None:
    incomplete = "\n".join(f"## {s}" for s in [
        "Structure", "Function", "Data", "Interfaces", "Platform", "Operations"
    ])  # Time deliberately missing
    res = _run_markdown_sections(incomplete)
    assert res.returncode == 1


def test_refusal_output_raw_fails_l1_sections_grader() -> None:
    """A refusal contains no SFDIPOT sections, so the runner's raw verdict is
    fail (exit 1). The composition layer (lib.composition) inverts this to
    `effective passed` for negative-polarity tasks via the
    `negate_on_polarity_negative: true` flag on g-charter-sections."""
    refusal = "I can't charter this brief without more detail. Could you tell me the target endpoint, the expected user-facing behaviour, and the SLO?"
    res = _run_markdown_sections(refusal)
    assert res.returncode == 1


def test_g_charter_sections_marked_negate_on_polarity_negative() -> None:
    """The shape grader must opt into polarity inversion so refusals on
    negative tasks compose to a pass. Without this flag the recipe's
    negative tasks would always fail; the runbook would be wrong."""
    g = load_graders(EVALS / "graders.yaml")
    sections_grader = next(grader for grader in g.graders if grader.id == "g-charter-sections")
    assert sections_grader.negate_on_polarity_negative is True


def test_g_output_shape_does_not_negate_on_polarity() -> None:
    """The non-empty-output grader must NOT invert: a refusal still has to
    have non-empty text, just no SFDIPOT sections."""
    g = load_graders(EVALS / "graders.yaml")
    shape_grader = next(grader for grader in g.graders if grader.id == "g-output-shape")
    assert shape_grader.negate_on_polarity_negative is False


def test_refusal_composes_to_pass_on_negative_task() -> None:
    """End-to-end: a refusal output, run through the L1 graders and the
    composition layer with charter-sfdipot's actual graders.yaml against a
    real negative task from tasks.jsonl, must compose to passed=True."""
    from lib.composition import GraderOutcome, compose_trial_pass

    g = load_graders(EVALS / "graders.yaml")
    tasks = load_tasks(EVALS / "tasks.jsonl")
    negative = next(t for t in tasks if t.polarity == "negative")

    # Simulate the L1 outcomes for a correct refusal: shape passes
    # (non-empty), sections fails (no SFDIPOT). L2 / L3 are skipped.
    outcomes = [
        GraderOutcome(grader_id="g-output-shape", passed=True),
        GraderOutcome(grader_id="g-charter-sections", passed=False),
        GraderOutcome(
            grader_id="g-charter-sme-review",
            passed=False,
            skipped=True,
            skip_reason="not sampled",
        ),
        GraderOutcome(
            grader_id="g-charter-judge",
            passed=False,
            skipped=True,
            skip_reason="no calibration",
        ),
    ]
    graders_by_id = {grader.id: grader for grader in g.graders}
    # Threshold lowered to (g-shape weight + g-sections weight) /
    # (g-shape weight + g-sections weight) since L2/L3 are skipped, both
    # remaining graders must effectively pass — default threshold of 1.0.
    passed, evidence = compose_trial_pass(outcomes, graders_by_id, negative)
    assert passed is True, f"refusal should compose to pass; evidence={evidence}"
    assert "polarity-inverted" in evidence["g-charter-sections"]


def test_charter_output_for_negative_task_composes_to_fail() -> None:
    """The mirror case: if the recipe charterised a vague brief instead of
    refusing, both L1 graders' raw verdicts pass — but with the inversion
    g-charter-sections becomes effective fail, and the trial fails."""
    from lib.composition import GraderOutcome, compose_trial_pass

    g = load_graders(EVALS / "graders.yaml")
    tasks = load_tasks(EVALS / "tasks.jsonl")
    negative = next(t for t in tasks if t.polarity == "negative")

    outcomes = [
        GraderOutcome(grader_id="g-output-shape", passed=True),
        GraderOutcome(grader_id="g-charter-sections", passed=True),
        GraderOutcome(
            grader_id="g-charter-sme-review",
            passed=False,
            skipped=True,
            skip_reason="not sampled",
        ),
        GraderOutcome(
            grader_id="g-charter-judge",
            passed=False,
            skipped=True,
            skip_reason="no calibration",
        ),
    ]
    graders_by_id = {grader.id: grader for grader in g.graders}
    passed, evidence = compose_trial_pass(outcomes, graders_by_id, negative)
    assert passed is False, f"fabricated charter on vague brief must fail; evidence={evidence}"
    assert "polarity-inverted" in evidence["g-charter-sections"]
    assert "raw=pass" in evidence["g-charter-sections"]
