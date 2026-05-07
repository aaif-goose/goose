"""Compose per-grader outcomes into a per-trial pass/fail.

The composition layer is what the harness uses to turn a list of grader
verdicts into a single pass/fail for one trial. It applies two rules:

    1. Polarity inversion. If a grader has `negate_on_polarity_negative=True`
       and the task has `polarity=negative`, the grader's raw `passed` value
       is flipped. This lets shape-checking graders (e.g., "all 7 SFDIPOT
       sections present") express the right thing for both positive tasks
       (sections must be present) and refusal tasks (sections must NOT be
       present).

    2. Skipped graders. Graders that did not run (e.g., L3 judges that
       auto-skipped because their calibration is stale) are excluded from the
       composition entirely. They neither pass nor fail the trial; they are
       absent. The trial's pass status is decided by the graders that did
       run, and the run record annotates the skipped ones for visibility.

Composition policy: a trial passes when the weighted sum of effective-passed
graders is at or above a configurable threshold (default 1.0 — every running
grader must pass). A simpler "AND of all running graders" is the default; the
weighted threshold is available for recipes that want to allow one weak
signal among several strong ones.
"""

from __future__ import annotations

from dataclasses import dataclass

from .graders import Grader
from .tasks import Task


@dataclass(frozen=True)
class GraderOutcome:
    """One grader's raw verdict on one trial.

    `passed` is the grader's own verdict, before any polarity inversion.
    `skipped` is True when the grader did not run for this trial (e.g., L3
    judges with stale calibration; L2 graders not sampled this trial).
    """

    grader_id: str
    passed: bool
    score: float = 0.0
    details: str = ""
    skipped: bool = False
    skip_reason: str = ""


def effective_passed(outcome: GraderOutcome, grader: Grader, task: Task) -> bool:
    """The grader's verdict after polarity inversion is applied (if declared).

    Skipped graders raise ValueError — the caller must filter them out before
    composing, since skipped graders contribute nothing to the trial verdict.
    """
    if outcome.skipped:
        raise ValueError(
            f"effective_passed called on skipped grader {grader.id!r}; "
            "filter skipped outcomes before composing"
        )
    if grader.negate_on_polarity_negative and task.polarity == "negative":
        return not outcome.passed
    return outcome.passed


def compose_trial_pass(
    outcomes: list[GraderOutcome],
    graders_by_id: dict[str, Grader],
    task: Task,
    *,
    threshold: float = 1.0,
) -> tuple[bool, dict[str, str]]:
    """Decide whether a trial passed, and explain why.

    Args:
        outcomes: one entry per grader that ran (or was eligible to run; skipped
            graders MAY be included and will be filtered).
        graders_by_id: every grader by id, for looking up weight / inversion.
        task: the task this trial is grading.
        threshold: weighted-pass threshold. The default 1.0 = every running
            grader must effectively pass. Lower threshold (e.g., 0.7) allows
            one weak signal among several stronger ones.

    Returns:
        (passed, evidence) where `evidence` maps grader_id -> short reason
        (effective verdict + skip reason if skipped + inversion note).

    Raises:
        ValueError if outcomes reference an unknown grader id.
    """
    evidence: dict[str, str] = {}
    running: list[GraderOutcome] = []

    for o in outcomes:
        if o.grader_id not in graders_by_id:
            raise ValueError(f"unknown grader {o.grader_id!r} in outcomes")
        if o.skipped:
            evidence[o.grader_id] = f"skipped: {o.skip_reason or 'no reason given'}"
            continue
        running.append(o)

    if not running:
        # No graders ran — refuse to declare a pass; the trial is effectively
        # un-graded. Caller should treat this as a yellow signal in the
        # results store; here we report False and explain.
        return False, {**evidence, "_": "no graders ran"}

    total_weight = 0.0
    weighted_pass = 0.0
    for o in running:
        g = graders_by_id[o.grader_id]
        eff = effective_passed(o, g, task)
        total_weight += g.weight
        if eff:
            weighted_pass += g.weight
        note = "passed" if eff else "failed"
        if g.negate_on_polarity_negative and task.polarity == "negative":
            note += f" (polarity-inverted; raw={'pass' if o.passed else 'fail'})"
        evidence[o.grader_id] = note

    if total_weight == 0:
        # Edge case: all running graders had zero weight. Treat as un-graded.
        return False, {**evidence, "_": "all running graders have zero weight"}

    ratio = weighted_pass / total_weight
    return ratio >= threshold, evidence
