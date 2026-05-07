"""Tests for the per-trial composition layer and polarity-aware grader inversion."""

from __future__ import annotations

import pytest

from lib.composition import GraderOutcome, compose_trial_pass, effective_passed
from lib.graders import L1Grader
from lib.tasks import Task


def _make_l1(
    grader_id: str,
    *,
    weight: float = 1.0,
    negate_on_polarity_negative: bool = False,
) -> L1Grader:
    return L1Grader(
        id=grader_id,
        level="L1",
        type="code",
        weight=weight,
        dimension=None,
        negate_on_polarity_negative=negate_on_polarity_negative,
        runner="echo ok",
        timeout_s=10,
    )


def _positive_task() -> Task:
    return Task(
        id="pos",
        description="positive",
        input={},
        polarity="positive",
        tags=["capability"],
    )


def _negative_task() -> Task:
    return Task(
        id="neg",
        description="negative",
        input={},
        polarity="negative",
        tags=["capability"],
    )


# ---------- effective_passed ----------


def test_effective_passed_no_inversion() -> None:
    g = _make_l1("g-x", negate_on_polarity_negative=False)
    o = GraderOutcome(grader_id="g-x", passed=True)
    assert effective_passed(o, g, _negative_task()) is True


def test_effective_passed_inverts_only_when_polarity_negative_and_flag_set() -> None:
    g_invertible = _make_l1("g-x", negate_on_polarity_negative=True)
    g_normal = _make_l1("g-y", negate_on_polarity_negative=False)
    pos = _positive_task()
    neg = _negative_task()
    raw_pass = GraderOutcome(grader_id="g-x", passed=True)
    raw_fail = GraderOutcome(grader_id="g-y", passed=False)

    # Invertible grader, positive task: no inversion.
    assert effective_passed(raw_pass, g_invertible, pos) is True
    # Invertible grader, negative task: inverted.
    assert effective_passed(raw_pass, g_invertible, neg) is False
    # Non-invertible grader, negative task: never inverted.
    assert effective_passed(raw_fail, g_normal, neg) is False


def test_effective_passed_skipped_outcome_raises() -> None:
    g = _make_l1("g-x")
    skipped = GraderOutcome(grader_id="g-x", passed=False, skipped=True, skip_reason="stale")
    with pytest.raises(ValueError, match="skipped grader"):
        effective_passed(skipped, g, _positive_task())


# ---------- compose_trial_pass: basic ----------


def test_all_pass_yields_trial_pass() -> None:
    g1 = _make_l1("g-1")
    g2 = _make_l1("g-2")
    outcomes = [
        GraderOutcome(grader_id="g-1", passed=True),
        GraderOutcome(grader_id="g-2", passed=True),
    ]
    passed, evidence = compose_trial_pass(outcomes, {"g-1": g1, "g-2": g2}, _positive_task())
    assert passed is True
    assert evidence["g-1"] == "passed"
    assert evidence["g-2"] == "passed"


def test_one_fail_yields_trial_fail_at_default_threshold() -> None:
    g1 = _make_l1("g-1")
    g2 = _make_l1("g-2")
    outcomes = [
        GraderOutcome(grader_id="g-1", passed=True),
        GraderOutcome(grader_id="g-2", passed=False),
    ]
    passed, evidence = compose_trial_pass(outcomes, {"g-1": g1, "g-2": g2}, _positive_task())
    assert passed is False
    assert evidence["g-2"] == "failed"


def test_threshold_lets_one_weak_signal_pass() -> None:
    # 0.6 threshold: total weight 1.0+0.5=1.5; one passing weight 1.0 = 0.667 ratio >= 0.6.
    g_strong = _make_l1("g-1", weight=1.0)
    g_weak = _make_l1("g-2", weight=0.5)
    outcomes = [
        GraderOutcome(grader_id="g-1", passed=True),
        GraderOutcome(grader_id="g-2", passed=False),
    ]
    passed, _ = compose_trial_pass(
        outcomes,
        {"g-1": g_strong, "g-2": g_weak},
        _positive_task(),
        threshold=0.6,
    )
    assert passed is True


# ---------- compose_trial_pass: polarity inversion ----------


def test_negative_task_passes_when_invertible_grader_raw_fails() -> None:
    """Recipe correctly refused: shape grader's raw verdict is `fail` (no
    SFDIPOT sections), but with negate_on_polarity_negative the effective
    verdict is `pass`."""
    g_sections = _make_l1("g-sections", negate_on_polarity_negative=True)
    outcome_raw_fail = GraderOutcome(grader_id="g-sections", passed=False, details="no sections")
    passed, evidence = compose_trial_pass(
        [outcome_raw_fail],
        {"g-sections": g_sections},
        _negative_task(),
    )
    assert passed is True
    assert "polarity-inverted" in evidence["g-sections"]
    assert "raw=fail" in evidence["g-sections"]


def test_negative_task_fails_when_invertible_grader_raw_passes() -> None:
    """Recipe failed to refuse: produced full SFDIPOT sections for a vague
    brief. Raw shape grader says `pass`; inverted to `fail` for the trial."""
    g_sections = _make_l1("g-sections", negate_on_polarity_negative=True)
    outcome_raw_pass = GraderOutcome(grader_id="g-sections", passed=True)
    passed, evidence = compose_trial_pass(
        [outcome_raw_pass],
        {"g-sections": g_sections},
        _negative_task(),
    )
    assert passed is False
    assert "polarity-inverted" in evidence["g-sections"]
    assert "raw=pass" in evidence["g-sections"]


def test_positive_task_unaffected_by_inversion_flag() -> None:
    g_sections = _make_l1("g-sections", negate_on_polarity_negative=True)
    raw_pass = GraderOutcome(grader_id="g-sections", passed=True)
    passed, evidence = compose_trial_pass(
        [raw_pass],
        {"g-sections": g_sections},
        _positive_task(),
    )
    assert passed is True
    assert "polarity-inverted" not in evidence["g-sections"]


def test_mixed_invertible_and_normal_graders_on_negative_task() -> None:
    """A refusal should: have non-empty output (g-shape pass) AND have no
    SFDIPOT sections (g-sections raw fail, inverted to pass). Both effective
    pass = trial pass."""
    g_shape = _make_l1("g-shape", negate_on_polarity_negative=False)
    g_sections = _make_l1("g-sections", negate_on_polarity_negative=True)
    outcomes = [
        GraderOutcome(grader_id="g-shape", passed=True),       # output non-empty
        GraderOutcome(grader_id="g-sections", passed=False),   # no sections — correct for refusal
    ]
    passed, _ = compose_trial_pass(
        outcomes,
        {"g-shape": g_shape, "g-sections": g_sections},
        _negative_task(),
    )
    assert passed is True


# ---------- compose_trial_pass: skipped graders ----------


def test_skipped_graders_excluded_from_composition() -> None:
    g1 = _make_l1("g-1")
    g_skipped = _make_l1("g-skipped")
    outcomes = [
        GraderOutcome(grader_id="g-1", passed=True),
        GraderOutcome(
            grader_id="g-skipped",
            passed=False,
            skipped=True,
            skip_reason="stale calibration",
        ),
    ]
    passed, evidence = compose_trial_pass(
        outcomes,
        {"g-1": g1, "g-skipped": g_skipped},
        _positive_task(),
    )
    # Only the running grader g-1 is composed; it passed, so the trial passes.
    assert passed is True
    assert evidence["g-skipped"].startswith("skipped: stale calibration")


def test_all_skipped_yields_un_graded_trial_as_fail() -> None:
    """Refuse to declare a pass when no grader actually ran. Surfacing the
    fact is more useful than silently passing."""
    g = _make_l1("g-x")
    outcomes = [
        GraderOutcome(grader_id="g-x", passed=False, skipped=True, skip_reason="stale"),
    ]
    passed, evidence = compose_trial_pass(outcomes, {"g-x": g}, _positive_task())
    assert passed is False
    assert evidence["_"] == "no graders ran"


# ---------- compose_trial_pass: error paths ----------


def test_unknown_grader_id_raises() -> None:
    g = _make_l1("g-known")
    outcomes = [GraderOutcome(grader_id="g-mystery", passed=True)]
    with pytest.raises(ValueError, match="unknown grader 'g-mystery'"):
        compose_trial_pass(outcomes, {"g-known": g}, _positive_task())


def test_zero_weight_running_graders_yield_un_graded() -> None:
    g_zero = _make_l1("g-zero", weight=0.0)
    outcomes = [GraderOutcome(grader_id="g-zero", passed=True)]
    passed, evidence = compose_trial_pass(outcomes, {"g-zero": g_zero}, _positive_task())
    assert passed is False
    assert "zero weight" in evidence["_"]
