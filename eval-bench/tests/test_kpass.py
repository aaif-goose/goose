"""Tests for the pass@k / pass^k math and slicing helpers."""

from __future__ import annotations

import math

import pytest

from lib.kpass import (
    TrialResult,
    aggregate_by_task,
    compute_passk,
    passk_by_slice,
    slice_results,
)


# ---------- compute_passk ----------


def test_passk_all_pass() -> None:
    at_k, pow_k = compute_passk([True, True, True, True], k=4)
    assert at_k == pytest.approx(1.0)
    assert pow_k == pytest.approx(1.0)


def test_passk_all_fail() -> None:
    at_k, pow_k = compute_passk([False, False, False], k=3)
    assert at_k == pytest.approx(0.0)
    assert pow_k == pytest.approx(0.0)


def test_passk_50_50_three_trials() -> None:
    # Per-trial p = 0.5; pass@3 = 1 - 0.5^3 = 0.875; pass^3 = 0.5^3 = 0.125
    at_k, pow_k = compute_passk([True, True, False, True, False, False], k=3)
    assert at_k == pytest.approx(0.875)
    assert pow_k == pytest.approx(0.125)


def test_passk_default_k_is_n() -> None:
    at_k, pow_k = compute_passk([True, False, True])
    # p = 2/3, k = 3
    assert at_k == pytest.approx(1 - (1 / 3) ** 3)
    assert pow_k == pytest.approx((2 / 3) ** 3)


def test_passk_rejects_extrapolation() -> None:
    # Refuses to compute pass@k for k > observed trials — would hide distribution shape.
    with pytest.raises(ValueError, match="exceeds observed trial count"):
        compute_passk([True, False], k=10)


def test_passk_rejects_empty() -> None:
    with pytest.raises(ValueError, match="at least one trial"):
        compute_passk([])


def test_passk_at_k_dominates_pow_k() -> None:
    # Universal property: for any 0 < p < 1 and k > 1, pass@k > pass^k.
    for p_passes, total in [(1, 5), (3, 5), (4, 5)]:
        trials = [True] * p_passes + [False] * (total - p_passes)
        at_k, pow_k = compute_passk(trials, k=total)
        assert at_k > pow_k, f"pass@k should dominate pass^k for p={p_passes}/{total}"


# ---------- aggregate_by_task ----------


def test_aggregate_by_task_preserves_trial_order() -> None:
    results = [
        TrialResult(task_id="t1", trial_index=2, passed=False, axes={}),
        TrialResult(task_id="t1", trial_index=0, passed=True, axes={}),
        TrialResult(task_id="t1", trial_index=1, passed=True, axes={}),
        TrialResult(task_id="t2", trial_index=0, passed=False, axes={}),
    ]
    grouped = aggregate_by_task(results)
    assert grouped["t1"] == [True, True, False]  # ordered by trial_index
    assert grouped["t2"] == [False]


# ---------- slice_results ----------


def test_slice_results_groups_by_axis_value() -> None:
    results = [
        TrialResult(task_id="t1", trial_index=0, passed=True, axes={"model": "opus"}),
        TrialResult(task_id="t2", trial_index=0, passed=False, axes={"model": "opus"}),
        TrialResult(task_id="t3", trial_index=0, passed=True, axes={"model": "haiku"}),
    ]
    sliced = slice_results(results, "model")
    assert set(sliced.keys()) == {"opus", "haiku"}
    assert len(sliced["opus"]) == 2
    assert len(sliced["haiku"]) == 1


def test_slice_results_unset_bucket_for_missing_axis() -> None:
    # Missing axis must not silently disappear — that would hide a slicing gap.
    results = [
        TrialResult(task_id="t1", trial_index=0, passed=True, axes={"model": "opus"}),
        TrialResult(task_id="t2", trial_index=0, passed=False, axes={}),
    ]
    sliced = slice_results(results, "model")
    assert "<unset>" in sliced
    assert len(sliced["<unset>"]) == 1


# ---------- passk_by_slice ----------


def test_passk_by_slice_per_axis_value() -> None:
    results = [
        TrialResult(task_id="t1", trial_index=0, passed=True, axes={"complexity": "low"}),
        TrialResult(task_id="t2", trial_index=0, passed=True, axes={"complexity": "low"}),
        TrialResult(task_id="t3", trial_index=0, passed=False, axes={"complexity": "high"}),
        TrialResult(task_id="t4", trial_index=0, passed=False, axes={"complexity": "high"}),
    ]
    by_slice = passk_by_slice(results, "complexity", k=2)
    # low: p=1, pass@2=1, pass^2=1
    assert by_slice["low"] == (pytest.approx(1.0), pytest.approx(1.0))
    # high: p=0, pass@2=0, pass^2=0
    assert by_slice["high"] == (pytest.approx(0.0), pytest.approx(0.0))


def test_passk_by_slice_excludes_empty_slices() -> None:
    by_slice = passk_by_slice([], "model", k=1)
    assert by_slice == {}


# ---------- numerical sanity ----------


def test_passk_matches_anthropic_example() -> None:
    # Anthropic article example: per-trial 75%, pass@10 ≈ 98%, pass^10 ≈ 6%.
    trials = [True] * 75 + [False] * 25  # p = 0.75
    at_k, pow_k = compute_passk(trials, k=10)
    assert math.isclose(at_k, 1 - 0.25 ** 10, rel_tol=1e-9)
    assert math.isclose(pow_k, 0.75 ** 10, rel_tol=1e-9)
    assert at_k > 0.98
    assert pow_k < 0.07
