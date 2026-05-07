"""Verify bug-advocacy's eval artifacts load cleanly through eval-bench.

Same shape as the other Phase 1 recipe tests, plus bug-advocacy-specific
invariants: 7 H2 sections, four high-severity failure modes around
fabrication and prescriptiveness, L2 sample_rate matches triage-failure
(0.15 — higher stakes than charter / oracles), L3 rubric carries three
hard rules.
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


def test_recipe_yaml_declares_observation_parameter() -> None:
    text = (RECIPE_DIR / "recipe.yaml").read_text(encoding="utf-8")
    assert "observation" in text


def test_recipe_yaml_states_no_fix_proposals() -> None:
    """The recipe's instructions must explicitly forbid proposing fixes."""
    text_lower = (RECIPE_DIR / "recipe.yaml").read_text(encoding="utf-8").lower()
    assert any(
        phrase in text_lower
        for phrase in [
            "never propose a code change",
            "never propose",
            "do not propose",
            "describe the bug",
        ]
    )


def test_recipe_yaml_lists_seven_h2_sections() -> None:
    text = (RECIPE_DIR / "recipe.yaml").read_text(encoding="utf-8")
    for section in [
        "Summary",
        "Steps to reproduce",
        "Actual result",
        "Expected result",
        "Environment",
        "Severity rationale",
        "Variations and open questions",
    ]:
        assert section in text, f"recipe.yaml must mention required section {section!r}"


def test_recipe_yaml_requires_falsifier_in_open_questions() -> None:
    """The recipe's instructions must require an explicit 'What would prove
    this is NOT a bug?' falsifier — it's the only thing that makes the
    report actually closeable."""
    text = (RECIPE_DIR / "recipe.yaml").read_text(encoding="utf-8")
    assert "NOT a bug" in text or "not a bug" in text, (
        "recipe.yaml must require an explicit falsifier line"
    )


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


def test_domain_axis_covers_diverse_domains() -> None:
    """The recipe should be exercised against diverse domains, not just one.
    Without diversity, Slice Explorer trends by domain are uninterpretable
    and we don't know if the recipe handles e.g. accessibility differently
    from billing."""
    tasks = load_tasks(EVALS / "tasks.jsonl")
    positive_domains = {t.axes.get("domain") for t in tasks if t.polarity == "positive"}
    assert len(positive_domains) >= 4, (
        f"positive tasks must cover at least 4 domains; got {positive_domains}"
    )


def test_required_axes_present_on_every_task() -> None:
    tasks = load_tasks(EVALS / "tasks.jsonl")
    required_axes = {"complexity", "language", "domain"}
    for t in tasks:
        missing = required_axes - t.axes.keys()
        assert not missing, f"task {t.id!r} missing axes: {missing}"


def test_severity_calibration_tasks_present() -> None:
    """At least one task must explicitly target severity calibration —
    typically the trivial-cosmetic case (fm-overstates-severity) and the
    high-impact case (data loss). Without these the recipe could
    over- or under-state severity systematically without our noticing."""
    tasks = load_tasks(EVALS / "tasks.jsonl")
    targets = set()
    for t in tasks:
        targets.update(t.failure_modes_targeted)
    assert "fm-overstates-severity" in targets, (
        "at least one task must target fm-overstates-severity to keep severity calibration honest"
    )


# ---------- failure-modes.yaml ----------


def test_failure_modes_load() -> None:
    fm = load_failure_modes(EVALS / "failure-modes.yaml")
    ids = {m.id for m in fm.modes}
    expected_seed = {
        "fm-fabricates-repro-steps",
        "fm-overstates-severity",
        "fm-prescriptive",
        "fm-overhelpful-on-vague-input",
        "fm-no-disconfirming-falsifier",
        "fm-fabricated-environment",
        "fm-conflates-symptom-with-bug",
    }
    assert expected_seed.issubset(ids), f"missing seed modes: {expected_seed - ids}"


def test_high_severity_modes_set_correctly() -> None:
    """The four high-stakes failure modes must be marked severity=high.
    Engineers act on these reports; severity drives the runbook
    prioritisation."""
    fm = load_failure_modes(EVALS / "failure-modes.yaml")
    expected_high = {
        "fm-fabricates-repro-steps",
        "fm-overstates-severity",
        "fm-prescriptive",
        "fm-overhelpful-on-vague-input",
    }
    for mode_id in expected_high:
        mode = fm.by_id(mode_id)
        assert mode is not None, f"missing failure mode {mode_id!r}"
        assert mode.severity == "high", (
            f"failure mode {mode_id!r} must be severity=high; got {mode.severity!r}"
        )


def test_active_failure_modes_referenced_or_acknowledged() -> None:
    """Active modes that no task targets should be rubric-watched only —
    don't let decoration accumulate. Allow a small set of rubric-watched
    modes; require everything else to be linked from at least one task."""
    fm = load_failure_modes(EVALS / "failure-modes.yaml")
    tasks = load_tasks(EVALS / "tasks.jsonl")
    referenced = set()
    for t in tasks:
        referenced.update(t.failure_modes_targeted)
    active_ids = {m.id for m in fm.active()}
    orphaned = active_ids - referenced
    # fm-no-disconfirming-falsifier and fm-conflates-symptom-with-bug are
    # rubric-watched (D4 in the SME rubric, and the recipe's symptom-vs-bug
    # guidance respectively); they don't need a task explicitly targeting them.
    allowed_orphans = {"fm-no-disconfirming-falsifier", "fm-conflates-symptom-with-bug"}
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


def test_g_bug_sections_marked_negate_on_polarity_negative() -> None:
    g = load_graders(EVALS / "graders.yaml")
    sections_grader = next(grader for grader in g.graders if grader.id == "g-bug-sections")
    assert sections_grader.negate_on_polarity_negative is True


def test_l2_sample_rate_matches_triage_stakes() -> None:
    """Both bug-advocacy and triage-failure feed engineer prioritisation
    downstream, so they share the higher 0.15 sample rate. Pin it so it
    doesn't silently drift back to the recipe-template default."""
    g = load_graders(EVALS / "graders.yaml")
    l2 = next(grader for grader in g.graders if grader.level == "L2")
    assert l2.sample_rate >= 0.15, (
        f"bug-advocacy L2 sample_rate must be >= 0.15 (higher stakes); got {l2.sample_rate}"
    )


def test_l3_judge_targets_correct_rubric() -> None:
    g = load_graders(EVALS / "graders.yaml")
    judge = next(grader for grader in g.graders if grader.id == "g-bug-judge")
    assert judge.rubric == "rubrics/judge_bug_quality.md"
    assert (EVALS / judge.rubric).is_file()
    rubric_text = (EVALS / judge.rubric).read_text(encoding="utf-8").lower()
    # The bug-advocacy rubric has THREE hard rules — fabricated repro,
    # fix proposals, fabricated environment values. Pin all three.
    assert "fabricated repro" in rubric_text or "fabricated step" in rubric_text or "repro step" in rubric_text
    assert "fix" in rubric_text and "code change" in rubric_text
    assert "environment" in rubric_text


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
    assert "L3 g-bug-judge: skipped" in out


# ---------- L1 graders accept the right shapes for this recipe ----------


GRADER_RUNNERS = REPO_ROOT / "eval-bench" / "grader_runners"


def _run_bug_sections_grader(output: str) -> subprocess.CompletedProcess:
    import json
    return subprocess.run(
        [
            sys.executable,
            str(GRADER_RUNNERS / "markdown_sections.py"),
            "--required",
            "Summary,Steps to reproduce,Actual result,Expected result,Environment,Severity rationale,Variations and open questions",
        ],
        input=json.dumps({"output": output, "task": {}}),
        capture_output=True,
        text=True,
        check=False,
    )


def test_full_bug_report_passes_l1_sections_grader() -> None:
    full = "\n".join(f"## {s}\nstub\n" for s in [
        "Summary",
        "Steps to reproduce",
        "Actual result",
        "Expected result",
        "Environment",
        "Severity rationale",
        "Variations and open questions",
    ])
    res = _run_bug_sections_grader(full)
    assert res.returncode == 0


def test_bug_report_missing_severity_rationale_fails_l1_sections_grader() -> None:
    incomplete = "\n".join(f"## {s}\nstub\n" for s in [
        "Summary",
        "Steps to reproduce",
        "Actual result",
        "Expected result",
        "Environment",
        "Variations and open questions",
    ])  # Severity rationale deliberately missing
    res = _run_bug_sections_grader(incomplete)
    assert res.returncode == 1
    assert "Severity rationale" in res.stdout


def test_refusal_composes_to_pass_on_negative_task() -> None:
    """End-to-end: a refusal output must compose to passed=True on a real
    negative task from tasks.jsonl via polarity inversion."""
    from lib.composition import GraderOutcome, compose_trial_pass

    g = load_graders(EVALS / "graders.yaml")
    tasks = load_tasks(EVALS / "tasks.jsonl")
    negative = next(t for t in tasks if t.polarity == "negative")

    outcomes = [
        GraderOutcome(grader_id="g-output-shape", passed=True),
        GraderOutcome(grader_id="g-bug-sections", passed=False),
        GraderOutcome(
            grader_id="g-bug-sme-review",
            passed=False, skipped=True, skip_reason="not sampled",
        ),
        GraderOutcome(
            grader_id="g-bug-judge",
            passed=False, skipped=True, skip_reason="no calibration",
        ),
    ]
    graders_by_id = {grader.id: grader for grader in g.graders}
    passed, evidence = compose_trial_pass(outcomes, graders_by_id, negative)
    assert passed is True, f"refusal should compose to pass; evidence={evidence}"
    assert "polarity-inverted" in evidence["g-bug-sections"]


def test_bug_report_for_negative_task_composes_to_fail() -> None:
    """If the recipe drafted a report on inadequate inputs instead of
    refusing, both L1 graders' raw verdicts pass — but with the inversion,
    g-bug-sections becomes effective fail and the trial fails."""
    from lib.composition import GraderOutcome, compose_trial_pass

    g = load_graders(EVALS / "graders.yaml")
    tasks = load_tasks(EVALS / "tasks.jsonl")
    negative = next(t for t in tasks if t.polarity == "negative")

    outcomes = [
        GraderOutcome(grader_id="g-output-shape", passed=True),
        GraderOutcome(grader_id="g-bug-sections", passed=True),
        GraderOutcome(
            grader_id="g-bug-sme-review",
            passed=False, skipped=True, skip_reason="not sampled",
        ),
        GraderOutcome(
            grader_id="g-bug-judge",
            passed=False, skipped=True, skip_reason="no calibration",
        ),
    ]
    graders_by_id = {grader.id: grader for grader in g.graders}
    passed, evidence = compose_trial_pass(outcomes, graders_by_id, negative)
    assert passed is False
    assert "polarity-inverted" in evidence["g-bug-sections"]
    assert "raw=pass" in evidence["g-bug-sections"]
