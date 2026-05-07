"""Tests for run_calibration — the L3-vs-L2 calibration computation."""

from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path

import pytest

from lib.annotations import Annotation, AnnotationReview, AnnotationStore
from lib.calibration import _cohen_kappa, run_calibration
from lib.graders import L3Grader
from lib.judge import JudgeVerdict, StubJudge


def _make_grader(**overrides) -> L3Grader:
    base = dict(
        id="g-judge",
        level="L3",
        type="llm_judge",
        weight=1.0,
        dimension=None,
        judge_model="anthropic:claude-opus-4-7",
        rubric="rubrics/judge.md",
        requires_calibration_within_days=30,
        max_divergence_from_l2=0.15,
    )
    base.update(overrides)
    return L3Grader(**base)


def _annotation(
    *,
    annotation_id: str,
    sme_verdict: str,
    output: str = "## Structure",
    feature_brief: str = "Add /healthz",
) -> Annotation:
    review = (
        AnnotationReview(
            verdict=sme_verdict,
            reviewer="N",
            reviewed_at="2026-05-08T12:00:00+00:00",
        )
        if sme_verdict in {"pass", "fail", "unknown"}
        else None
    )
    return Annotation(
        annotation_id=annotation_id,
        run_id=1,
        task_id="t",
        trial_index=0,
        grader_id="g-l2",
        polarity="positive",
        tags=["capability"],
        axes={},
        task_input={"feature_brief": feature_brief},
        task_expected=None,
        recipe_output=output,
        rubric_path="rubrics/judge.md",
        created_at="2026-05-08T12:00:00+00:00",
        status="completed" if review else "pending",
        review=review,
    )


def _seed_annotations(tmp_path: Path, annotations: list[Annotation]) -> AnnotationStore:
    store = AnnotationStore(tmp_path / "annotations")
    for a in annotations:
        store.write(a)
    return store


def _fake_rubric(tmp_path: Path) -> Path:
    rubric = tmp_path / "judge.md"
    rubric.write_text("# Test rubric\nGrade Honesty.\n", encoding="utf-8")
    return rubric


# ---------- happy path ----------


def test_full_agreement_yields_high_score(tmp_path: Path) -> None:
    """Stub judge always returns pass; if SME also says pass for every
    annotation, agreement should be 1.0."""
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(25)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
    )
    assert outcome.record.agreement == pytest.approx(1.0)
    assert outcome.record.deployed is True
    assert len(outcome.pairs) == 25


def test_zero_agreement_yields_not_deployable(tmp_path: Path) -> None:
    """Stub says pass; SME says fail. Agreement should be 0; not deployable."""
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="fail") for i in range(25)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
    )
    assert outcome.record.agreement == pytest.approx(0.0)
    assert outcome.record.deployed is False


def test_partial_agreement_just_above_threshold_is_deployed(tmp_path: Path) -> None:
    """20 pass-pass + 1 fail-pass disagreement → 20/21 ≈ 0.952; divergence
    0.048 < default 0.15, sample 21 ≥ 20 → deployed."""
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(20)]
    annotations.append(_annotation(annotation_id="a-fail", sme_verdict="fail"))
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
    )
    assert outcome.record.agreement == pytest.approx(20 / 21)
    assert outcome.record.deployed is True


def test_partial_agreement_just_below_threshold_not_deployed(tmp_path: Path) -> None:
    """20 pass-pass + 5 fail-pass → 20/25 = 0.8; divergence 0.20 > 0.15 → NOT deployed."""
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(20)]
    annotations += [_annotation(annotation_id=f"a-fail-{i}", sme_verdict="fail") for i in range(5)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
    )
    assert outcome.record.agreement == pytest.approx(0.8)
    assert outcome.record.deployed is False


# ---------- skip behaviour ----------


def test_unknown_sme_verdicts_are_skipped(tmp_path: Path) -> None:
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(20)]
    annotations += [_annotation(annotation_id=f"a-unk-{i}", sme_verdict="unknown") for i in range(5)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
    )
    assert len(outcome.pairs) == 20
    assert len(outcome.skipped) == 5
    for _, reason in outcome.skipped:
        assert "SME verdict is unknown" in reason


def test_judge_unknown_verdicts_are_skipped(tmp_path: Path) -> None:
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(20)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    judge = StubJudge(
        lambda _r, _p: JudgeVerdict(verdict="Unknown", evidence="not enough info")
    )
    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=judge,
        rubric_path=rubric,
    )
    assert len(outcome.pairs) == 0
    assert len(outcome.skipped) == 20
    assert all("judge returned Unknown" in r for _, r in outcome.skipped)


def test_judge_errors_are_skipped(tmp_path: Path) -> None:
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(20)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    judge = StubJudge(
        lambda _r, _p: JudgeVerdict(verdict="Unknown", error="HTTP 500 from API")
    )
    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=judge,
        rubric_path=rubric,
    )
    assert len(outcome.pairs) == 0
    assert all("judge error" in r for _, r in outcome.skipped)


def test_pending_annotations_are_excluded(tmp_path: Path) -> None:
    """Only completed annotations contribute. Pending ones — never reviewed —
    must not become silent calibration data."""
    completed = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(15)]
    pending = _annotation(annotation_id="a-pending", sme_verdict="pending-not-completed")  # not actually a verdict
    pending.status = "pending"
    pending.review = None
    store = AnnotationStore(tmp_path / "annotations")
    for a in completed:
        store.write(a)
    store.write(pending)
    rubric = _fake_rubric(tmp_path)

    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
    )
    assert len(outcome.pairs) == 15  # only the completed ones


# ---------- min_sample_size ----------


def test_below_min_sample_size_not_deployable(tmp_path: Path) -> None:
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(5)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
        min_sample_size=20,
    )
    assert outcome.record.agreement == pytest.approx(1.0)  # perfect, but…
    assert outcome.record.deployed is False  # not enough samples


def test_min_sample_size_override_via_arg(tmp_path: Path) -> None:
    """Lowering min_sample_size lets a small but unanimous sample deploy."""
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(5)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
        min_sample_size=5,
    )
    assert outcome.record.deployed is True


# ---------- max_divergence override ----------


def test_max_divergence_override_widens_acceptance(tmp_path: Path) -> None:
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(15)]
    annotations += [_annotation(annotation_id=f"a-fail-{i}", sme_verdict="fail") for i in range(5)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    # Default 0.15 would reject (divergence = 5/20 = 0.25). Loosen to 0.30.
    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
        max_divergence_from_l2=0.30,
    )
    assert outcome.record.deployed is True


# ---------- divergence breakdown ----------


def test_divergence_breakdown_records_disagreement_pattern(tmp_path: Path) -> None:
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(10)]
    annotations += [_annotation(annotation_id=f"a-fail-{i}", sme_verdict="fail") for i in range(5)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
    )
    assert "fail_vs_pass" in outcome.record.divergence_breakdown
    assert outcome.record.divergence_breakdown["fail_vs_pass"] == pytest.approx(5 / 15)


# ---------- Cohen's kappa ----------


def test_kappa_perfect_agreement_is_one() -> None:
    pairs = [("pass", "pass")] * 10 + [("fail", "fail")] * 10
    assert _cohen_kappa(pairs) == pytest.approx(1.0)


def test_kappa_zero_for_chance_level_agreement() -> None:
    """When the joint distribution exactly matches the marginal product,
    kappa should be ~0."""
    # 50% pass / 50% fail per rater, independent: only half align by chance.
    pairs = [("pass", "pass")] * 25 + [("pass", "fail")] * 25
    pairs += [("fail", "pass")] * 25 + [("fail", "fail")] * 25
    assert _cohen_kappa(pairs) == pytest.approx(0.0)


def test_kappa_negative_when_systematic_disagreement() -> None:
    """When raters disagree more than chance, kappa goes negative."""
    pairs = [("pass", "fail")] * 10 + [("fail", "pass")] * 10
    assert _cohen_kappa(pairs) < 0


def test_kappa_returns_one_for_single_category() -> None:
    """If both raters use only one category and agree on every item, kappa
    is undefined; we treat it as 1.0 (perfect agreement)."""
    pairs = [("pass", "pass")] * 10
    assert _cohen_kappa(pairs) == pytest.approx(1.0)


def test_record_carries_cohen_kappa(tmp_path: Path) -> None:
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(10)]
    annotations += [_annotation(annotation_id=f"a-fail-{i}", sme_verdict="fail") for i in range(10)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
    )
    assert outcome.record.cohen_kappa is not None


# ---------- record fields ----------


def test_record_uses_provided_now(tmp_path: Path) -> None:
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(20)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    fixed_now = datetime(2026, 5, 8, 12, 0, 0, tzinfo=timezone.utc)
    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
        now=fixed_now,
    )
    assert outcome.record.timestamp == fixed_now


def test_record_includes_judge_id_and_model(tmp_path: Path) -> None:
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(20)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    grader = _make_grader(id="g-charter-judge", judge_model="anthropic:claude-opus-4-7")
    outcome = run_calibration(
        grader=grader,
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
    )
    assert outcome.record.judge_id == "g-charter-judge"
    assert outcome.record.judge_model == "anthropic:claude-opus-4-7"


def test_notes_includes_sample_size_and_threshold(tmp_path: Path) -> None:
    annotations = [_annotation(annotation_id=f"a-{i}", sme_verdict="pass") for i in range(5)]
    store = _seed_annotations(tmp_path, annotations)
    rubric = _fake_rubric(tmp_path)

    outcome = run_calibration(
        grader=_make_grader(),
        annotations_store=store,
        judge=StubJudge.always("pass"),
        rubric_path=rubric,
        notes="quarterly recal",
    )
    assert "quarterly recal" in (outcome.record.notes or "")
    assert "sample_size=5" in (outcome.record.notes or "")
