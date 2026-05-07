"""Tests for the annotation file format, AnnotationStore, and sampling."""

from __future__ import annotations

from pathlib import Path

import pytest

from lib.annotations import (
    Annotation,
    AnnotationReview,
    AnnotationStore,
    make_annotation_id,
    should_sample,
)


def _seed(**overrides) -> Annotation:
    base = dict(
        annotation_id="r1-tex-i0-gg-l2",
        run_id=1,
        task_id="ex",
        trial_index=0,
        grader_id="g-l2",
        polarity="positive",
        tags=["capability"],
        axes={"complexity": "low"},
        task_input={"feature_brief": "Add /healthz"},
        task_expected={"contract": "full-sfdipot"},
        recipe_output="## Structure\n## Function",
        rubric_path="rubrics/sme_quality.md",
        created_at="2026-05-08T12:00:00+00:00",
        status="pending",
        review=None,
    )
    base.update(overrides)
    return Annotation(**base)


# ---------- make_annotation_id ----------


def test_make_annotation_id_is_deterministic() -> None:
    a = make_annotation_id(run_id=42, task_id="rate-limiting", trial_index=2, grader_id="g-x")
    b = make_annotation_id(run_id=42, task_id="rate-limiting", trial_index=2, grader_id="g-x")
    assert a == b


def test_make_annotation_id_filesystem_safe() -> None:
    """Slashes, spaces, and other awkward characters in task ids must not
    leak into the filename."""
    aid = make_annotation_id(
        run_id=1, task_id="weird/name with space", trial_index=0, grader_id="g/l2",
    )
    assert "/" not in aid
    assert " " not in aid


# ---------- should_sample ----------


def test_sample_rate_zero_never_samples() -> None:
    for trial in range(50):
        assert not should_sample(
            run_id=1, task_id="t", trial_index=trial, grader_id="g", sample_rate=0.0,
        )


def test_sample_rate_one_always_samples() -> None:
    for trial in range(50):
        assert should_sample(
            run_id=1, task_id="t", trial_index=trial, grader_id="g", sample_rate=1.0,
        )


def test_sampling_is_deterministic() -> None:
    """Identical inputs must produce identical decisions across calls."""
    args = dict(run_id=7, task_id="t-1", trial_index=3, grader_id="g-1", sample_rate=0.5)
    first = [should_sample(**args) for _ in range(10)]
    assert all(d == first[0] for d in first)


def test_sample_rate_distribution_roughly_matches() -> None:
    """Across many distinct keys the empirical sample rate should be close
    to the configured rate. Not a tight statistical test — just enough to
    catch a major skew (e.g., always-true or always-false bug)."""
    samples = [
        should_sample(run_id=1, task_id=f"t-{i}", trial_index=0, grader_id="g", sample_rate=0.20)
        for i in range(500)
    ]
    rate = sum(samples) / len(samples)
    assert 0.10 < rate < 0.30, f"empirical rate {rate} far from configured 0.20"


def test_sample_rate_axes_are_independent() -> None:
    """Changing the grader_id alone changes the decision (same task / trial)."""
    decisions = {
        gid: should_sample(run_id=1, task_id="t", trial_index=0, grader_id=gid, sample_rate=0.5)
        for gid in ("g-a", "g-b", "g-c", "g-d", "g-e", "g-f")
    }
    # Across 6 grader ids at sample_rate=0.5 we expect both True and False.
    assert True in decisions.values()
    assert False in decisions.values()


# ---------- Annotation round-trip ----------


def test_annotation_to_dict_round_trip() -> None:
    a = _seed()
    parsed = Annotation.from_dict(a.to_dict())
    assert parsed.annotation_id == a.annotation_id
    assert parsed.task_input == a.task_input
    assert parsed.review is None


def test_annotation_with_review_round_trip() -> None:
    a = _seed(
        status="completed",
        review=AnnotationReview(
            verdict="pass",
            reviewer="N",
            reviewed_at="2026-05-08T13:00:00+00:00",
            notes="ok",
            scores={"honesty": 2},
        ),
    )
    parsed = Annotation.from_dict(a.to_dict())
    assert parsed.review is not None
    assert parsed.review.verdict == "pass"
    assert parsed.review.scores == {"honesty": 2}


def test_annotation_rejects_bad_status() -> None:
    a = _seed()
    bad = a.to_dict()
    bad["status"] = "approved"
    with pytest.raises(ValueError, match="status must be one of"):
        Annotation.from_dict(bad)


def test_annotation_rejects_bad_verdict_in_review() -> None:
    bad = _seed().to_dict()
    bad["review"] = {"verdict": "yes", "reviewer": "N", "reviewed_at": "x"}
    with pytest.raises(ValueError, match="verdict must be one of"):
        Annotation.from_dict(bad)


def test_annotation_rejects_unknown_version() -> None:
    bad = _seed().to_dict()
    bad["version"] = 99
    with pytest.raises(ValueError, match="unsupported annotation version"):
        Annotation.from_dict(bad)


# ---------- AnnotationStore ----------


def test_store_round_trip(tmp_path: Path) -> None:
    store = AnnotationStore(tmp_path / "queue")
    a = _seed()
    store.write(a)
    assert store.read(a.annotation_id).annotation_id == a.annotation_id


def test_store_lists_pending_and_completed(tmp_path: Path) -> None:
    store = AnnotationStore(tmp_path / "queue")
    store.write(_seed(annotation_id="a-1"))
    store.write(_seed(annotation_id="a-2"))
    store.write(_seed(annotation_id="a-3"))
    assert {a.annotation_id for a in store.list_pending()} == {"a-1", "a-2", "a-3"}
    store.complete("a-2", verdict="pass", reviewer="N", notes="looks ok")
    assert {a.annotation_id for a in store.list_pending()} == {"a-1", "a-3"}
    assert {a.annotation_id for a in store.list_completed()} == {"a-2"}


def test_complete_persists_review(tmp_path: Path) -> None:
    store = AnnotationStore(tmp_path / "queue")
    store.write(_seed(annotation_id="a-1"))
    store.complete(
        "a-1", verdict="fail", reviewer="N", notes="fabricated 429", scores={"honesty": 0},
    )
    a = store.read("a-1")
    assert a.status == "completed"
    assert a.review is not None
    assert a.review.verdict == "fail"
    assert a.review.notes == "fabricated 429"
    assert a.review.scores == {"honesty": 0}


def test_complete_rejects_unknown_verdict(tmp_path: Path) -> None:
    store = AnnotationStore(tmp_path / "queue")
    store.write(_seed(annotation_id="a-1"))
    with pytest.raises(ValueError, match="verdict must be one of"):
        store.complete("a-1", verdict="maybe", reviewer="N")


def test_complete_rejects_already_completed(tmp_path: Path) -> None:
    store = AnnotationStore(tmp_path / "queue")
    store.write(_seed(annotation_id="a-1"))
    store.complete("a-1", verdict="pass", reviewer="N")
    with pytest.raises(ValueError, match="already completed"):
        store.complete("a-1", verdict="fail", reviewer="N")


def test_discard_only_works_on_pending(tmp_path: Path) -> None:
    store = AnnotationStore(tmp_path / "queue")
    store.write(_seed(annotation_id="a-1"))
    store.complete("a-1", verdict="pass", reviewer="N")
    with pytest.raises(ValueError, match="can only discard pending"):
        store.discard("a-1", reason="changed my mind")


def test_discard_marks_status_and_records_reason(tmp_path: Path) -> None:
    store = AnnotationStore(tmp_path / "queue")
    store.write(_seed(annotation_id="a-1"))
    store.discard("a-1", reason="recipe output was malformed")
    a = store.read("a-1")
    assert a.status == "discarded"
    assert a.review is not None
    assert "malformed" in a.review.notes


def test_store_creates_directory_if_missing(tmp_path: Path) -> None:
    nested = tmp_path / "x" / "y" / "queue"
    AnnotationStore(nested)
    assert nested.is_dir()
