"""Tests for the annotate.py CLI."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

from lib.annotations import Annotation, AnnotationStore

REPO_ROOT = Path(__file__).resolve().parents[2]
ANNOTATE_CLI = REPO_ROOT / "eval-bench" / "annotate.py"


def _make_recipe_with_annotation(tmp_path: Path, annotation_id: str = "r1-tex-i0-gg-l2") -> Path:
    """Create a fake recipe directory with one pending annotation."""
    recipe = tmp_path / "fake-recipe"
    (recipe / "evals" / "annotations").mkdir(parents=True)
    store = AnnotationStore(recipe / "evals" / "annotations")
    a = Annotation(
        annotation_id=annotation_id,
        run_id=1,
        task_id="ex",
        trial_index=0,
        grader_id="g-l2",
        polarity="positive",
        tags=["capability"],
        axes={"complexity": "low", "domain": "api"},
        task_input={"feature_brief": "Add /healthz returning 200"},
        task_expected={"contract": "full-sfdipot"},
        recipe_output="## Structure\n## Function\n## Data\n## Interfaces\n## Platform\n## Operations\n## Time",
        rubric_path="rubrics/sme_quality.md",
        created_at="2026-05-08T12:00:00+00:00",
    )
    store.write(a)
    return recipe


def _run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(ANNOTATE_CLI), *args],
        capture_output=True,
        text=True,
        check=False,
    )


# ---------- list ----------


def test_list_shows_pending(tmp_path: Path) -> None:
    recipe = _make_recipe_with_annotation(tmp_path)
    res = _run("list", str(recipe))
    assert res.returncode == 0
    assert "pending:   1" in res.stdout
    assert "r1-tex-i0-gg-l2" in res.stdout
    assert "Add /healthz" in res.stdout


def test_list_handles_empty_queue(tmp_path: Path) -> None:
    recipe = tmp_path / "empty"
    (recipe / "evals" / "annotations").mkdir(parents=True)
    res = _run("list", str(recipe))
    assert res.returncode == 0
    assert "no pending annotations" in res.stdout


def test_list_errors_on_recipe_without_evals(tmp_path: Path) -> None:
    bare = tmp_path / "bare"
    bare.mkdir()
    res = _run("list", str(bare))
    assert res.returncode != 0
    assert "no evals/" in res.stderr


# ---------- show ----------


def test_show_prints_full_annotation(tmp_path: Path) -> None:
    recipe = _make_recipe_with_annotation(tmp_path)
    res = _run("show", str(recipe), "--id", "r1-tex-i0-gg-l2")
    assert res.returncode == 0
    assert "annotation_id: r1-tex-i0-gg-l2" in res.stdout
    assert "status:        pending" in res.stdout
    assert "Add /healthz returning 200" in res.stdout
    assert "## Structure" in res.stdout
    assert "rubrics/sme_quality.md" in res.stdout


def test_show_missing_id(tmp_path: Path) -> None:
    recipe = _make_recipe_with_annotation(tmp_path)
    res = _run("show", str(recipe), "--id", "nope")
    assert res.returncode != 0
    assert "not found" in res.stderr


# ---------- review ----------


def test_review_marks_completed_with_verdict_and_scores(tmp_path: Path) -> None:
    recipe = _make_recipe_with_annotation(tmp_path)
    res = _run(
        "review", str(recipe),
        "--id", "r1-tex-i0-gg-l2",
        "--verdict", "pass",
        "--reviewer", "Norbi",
        "--note", "looks honest",
        "--score", "honesty=2",
        "--score", "tactics=1",
    )
    assert res.returncode == 0
    assert "completed r1-tex-i0-gg-l2: pass by Norbi" in res.stdout

    persisted = AnnotationStore(recipe / "evals" / "annotations").read("r1-tex-i0-gg-l2")
    assert persisted.status == "completed"
    assert persisted.review is not None
    assert persisted.review.verdict == "pass"
    assert persisted.review.scores == {"honesty": 2, "tactics": 1}
    assert "looks honest" in persisted.review.notes


def test_review_rejects_invalid_verdict(tmp_path: Path) -> None:
    recipe = _make_recipe_with_annotation(tmp_path)
    res = _run(
        "review", str(recipe),
        "--id", "r1-tex-i0-gg-l2",
        "--verdict", "maybe",
        "--reviewer", "N",
    )
    assert res.returncode != 0


def test_review_rejects_already_completed(tmp_path: Path) -> None:
    recipe = _make_recipe_with_annotation(tmp_path)
    _run("review", str(recipe), "--id", "r1-tex-i0-gg-l2", "--verdict", "pass", "--reviewer", "N")
    res = _run("review", str(recipe), "--id", "r1-tex-i0-gg-l2", "--verdict", "fail", "--reviewer", "N")
    assert res.returncode != 0
    assert "already completed" in res.stderr


def test_review_rejects_malformed_score(tmp_path: Path) -> None:
    recipe = _make_recipe_with_annotation(tmp_path)
    res = _run(
        "review", str(recipe),
        "--id", "r1-tex-i0-gg-l2",
        "--verdict", "pass",
        "--reviewer", "N",
        "--score", "no_equals_sign",
    )
    assert res.returncode != 0


def test_review_rejects_non_integer_score(tmp_path: Path) -> None:
    recipe = _make_recipe_with_annotation(tmp_path)
    res = _run(
        "review", str(recipe),
        "--id", "r1-tex-i0-gg-l2",
        "--verdict", "pass",
        "--reviewer", "N",
        "--score", "honesty=high",
    )
    assert res.returncode != 0


# ---------- discard ----------


def test_discard_marks_status_and_records_reason(tmp_path: Path) -> None:
    recipe = _make_recipe_with_annotation(tmp_path)
    res = _run("discard", str(recipe), "--id", "r1-tex-i0-gg-l2", "--reason", "output truncated")
    assert res.returncode == 0
    persisted = AnnotationStore(recipe / "evals" / "annotations").read("r1-tex-i0-gg-l2")
    assert persisted.status == "discarded"
    assert persisted.review is not None
    assert "output truncated" in persisted.review.notes


def test_discard_rejects_completed(tmp_path: Path) -> None:
    recipe = _make_recipe_with_annotation(tmp_path)
    _run("review", str(recipe), "--id", "r1-tex-i0-gg-l2", "--verdict", "pass", "--reviewer", "N")
    res = _run("discard", str(recipe), "--id", "r1-tex-i0-gg-l2", "--reason", "x")
    assert res.returncode != 0
