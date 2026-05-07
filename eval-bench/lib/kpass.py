"""pass@k and pass^k computation, plus slicing by axis.

These are the two determinism metrics every Skein LLM-using recipe records.

- pass@k:  P(>=1 of k trials passes).         Useful when one working solution suffices.
- pass^k:  P(all k trials pass).              The CI / customer-facing metric.

Both are derived from a per-trial pass rate p:
    pass@k = 1 - (1 - p)^k
    pass^k = p^k

We compute these over a sequence of boolean trial outcomes for a single task,
then aggregate across tasks and slice axes.
"""

from __future__ import annotations

from collections import defaultdict
from collections.abc import Iterable, Sequence
from dataclasses import dataclass


@dataclass(frozen=True)
class TrialResult:
    """One trial outcome for one task."""

    task_id: str
    trial_index: int
    passed: bool
    axes: dict[str, str | int | float | bool]


def compute_passk(trials: Sequence[bool], k: int | None = None) -> tuple[float, float]:
    """Return (pass_at_k, pass_pow_k) for a sequence of trial outcomes.

    If k is None, k = len(trials). If k > len(trials), the call raises ValueError —
    we never extrapolate beyond observed trials, since that hides distribution shape.
    """
    if not trials:
        raise ValueError("compute_passk requires at least one trial")
    n = len(trials)
    k = n if k is None else k
    if k > n:
        raise ValueError(f"k={k} exceeds observed trial count {n}; cannot extrapolate")
    p = sum(1 for t in trials if t) / n
    pass_at_k = 1.0 - (1.0 - p) ** k
    pass_pow_k = p ** k
    return pass_at_k, pass_pow_k


def aggregate_by_task(results: Iterable[TrialResult]) -> dict[str, list[bool]]:
    """Group trial outcomes by task id, preserving trial order."""
    by_task: dict[str, list[tuple[int, bool]]] = defaultdict(list)
    for r in results:
        by_task[r.task_id].append((r.trial_index, r.passed))
    return {
        tid: [passed for _, passed in sorted(pairs)]
        for tid, pairs in by_task.items()
    }


def slice_results(
    results: Iterable[TrialResult],
    axis: str,
) -> dict[str, list[TrialResult]]:
    """Group trial results by the value of one axis.

    Trials missing the axis go into an explicit "<unset>" bucket — never silently
    dropped, since that would hide a slicing gap.
    """
    out: dict[str, list[TrialResult]] = defaultdict(list)
    for r in results:
        key = str(r.axes.get(axis, "<unset>"))
        out[key].append(r)
    return dict(out)


def passk_by_slice(
    results: Iterable[TrialResult],
    axis: str,
    k: int | None = None,
) -> dict[str, tuple[float, float]]:
    """For each value of `axis`, compute (pass@k, pass^k) across all trials in that slice.

    Returns {axis_value: (pass_at_k, pass_pow_k)}. The pass rates here are computed
    across all trials in the slice without re-grouping by task; this surfaces the
    coarse "how does this slice do overall" view. For per-task views, use
    `aggregate_by_task` and call `compute_passk` on each.
    """
    out: dict[str, tuple[float, float]] = {}
    for value, trials in slice_results(results, axis).items():
        outcomes = [t.passed for t in trials]
        if outcomes:
            out[value] = compute_passk(outcomes, k=k)
    return out
