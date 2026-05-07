"""Tests for the calibrate.py CLI."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

from lib.annotations import Annotation, AnnotationReview, AnnotationStore

REPO_ROOT = Path(__file__).resolve().parents[2]
CALIBRATE_CLI = REPO_ROOT / "eval-bench" / "calibrate.py"
CHARTER = REPO_ROOT / "recipes" / "test" / "charter-sfdipot"


def _seed_completed_annotations(
    recipe: Path,
    *,
    n_pass: int,
    n_fail: int,
) -> None:
    """Drop completed annotations into the recipe's queue. SME verdict
    matches the n_pass / n_fail split."""
    store = AnnotationStore(recipe / "evals" / "annotations")
    for stale in store.directory.glob("r*.json"):
        stale.unlink()
    counter = 0
    for verdict, count in [("pass", n_pass), ("fail", n_fail)]:
        for _ in range(count):
            counter += 1
            a = Annotation(
                annotation_id=f"r1-tex-{counter}",
                run_id=1,
                task_id="ex",
                trial_index=counter,
                grader_id="g-charter-sme-review",
                polarity="positive",
                tags=["capability"],
                axes={},
                task_input={"feature_brief": "Add /healthz"},
                task_expected={"contract": "full-sfdipot"},
                recipe_output="## Structure\n## Function\n## Data\n## Interfaces\n## Platform\n## Operations\n## Time",
                rubric_path="rubrics/judge_charter_quality.md",
                created_at="2026-05-08T12:00:00+00:00",
                status="completed",
                review=AnnotationReview(
                    verdict=verdict,
                    reviewer="N",
                    reviewed_at="2026-05-08T13:00:00+00:00",
                ),
            )
            store.write(a)


def _copy_charter_recipe(tmp_path: Path) -> Path:
    """Copy charter-sfdipot to tmp so we don't mutate the real recipe."""
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present in this checkout")
    dst = tmp_path / "charter-copy"
    shutil.copytree(CHARTER, dst)
    return dst


def _run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(CALIBRATE_CLI), *args],
        capture_output=True,
        text=True,
        check=False,
    )


# ---------- happy path ----------


def test_stub_judge_full_agreement_writes_deployed_record(tmp_path: Path) -> None:
    recipe = _copy_charter_recipe(tmp_path)
    _seed_completed_annotations(recipe, n_pass=25, n_fail=0)

    res = _run(
        str(recipe),
        "--grader-id", "g-charter-judge",
        "--judge", "stub",
        "--note", "first calibration",
    )
    assert res.returncode == 0, f"stderr:\n{res.stderr}\nstdout:\n{res.stdout}"
    assert "deployed:      True" in res.stdout

    log = recipe / "evals" / "calibration.jsonl"
    assert log.exists()
    [record] = [json.loads(line) for line in log.read_text(encoding="utf-8").strip().splitlines()]
    assert record["judge_id"] == "g-charter-judge"
    assert record["agreement"] == 1.0
    assert record["deployed"] is True
    assert "first calibration" in (record["notes"] or "")


def test_stub_judge_disagreement_writes_not_deployed_record(tmp_path: Path) -> None:
    recipe = _copy_charter_recipe(tmp_path)
    # Stub always returns pass; 5 fail SME verdicts → 20/25 = 0.80 → divergence 0.20 > 0.15.
    _seed_completed_annotations(recipe, n_pass=20, n_fail=5)

    res = _run(str(recipe), "--grader-id", "g-charter-judge", "--judge", "stub")
    assert res.returncode == 1  # not deployable → exit 1
    assert "deployed:      False" in res.stdout
    assert "disagreements" in res.stdout
    assert "fail_vs_pass" in res.stdout


def test_dry_run_does_not_write_record(tmp_path: Path) -> None:
    recipe = _copy_charter_recipe(tmp_path)
    _seed_completed_annotations(recipe, n_pass=25, n_fail=0)

    res = _run(
        str(recipe),
        "--grader-id", "g-charter-judge",
        "--judge", "stub",
        "--dry-run",
    )
    assert res.returncode == 0
    assert "record NOT appended" in res.stdout
    assert not (recipe / "evals" / "calibration.jsonl").exists()


def test_record_appends_rather_than_overwrites(tmp_path: Path) -> None:
    recipe = _copy_charter_recipe(tmp_path)
    _seed_completed_annotations(recipe, n_pass=25, n_fail=0)

    _run(str(recipe), "--grader-id", "g-charter-judge", "--judge", "stub")
    _run(str(recipe), "--grader-id", "g-charter-judge", "--judge", "stub")

    lines = (recipe / "evals" / "calibration.jsonl").read_text(encoding="utf-8").strip().splitlines()
    assert len(lines) == 2


# ---------- argument validation / error paths ----------


def test_missing_recipe_dir_errors(tmp_path: Path) -> None:
    res = _run(
        str(tmp_path / "does-not-exist"),
        "--grader-id", "g",
        "--judge", "stub",
    )
    assert res.returncode == 2
    assert "no evals/" in res.stderr


def test_unknown_grader_id_errors(tmp_path: Path) -> None:
    recipe = _copy_charter_recipe(tmp_path)
    _seed_completed_annotations(recipe, n_pass=25, n_fail=0)

    res = _run(str(recipe), "--grader-id", "g-not-real", "--judge", "stub")
    assert res.returncode == 2
    assert "no L3 grader" in res.stderr


def test_no_completed_annotations_errors(tmp_path: Path) -> None:
    recipe = _copy_charter_recipe(tmp_path)
    # Don't seed any.
    res = _run(str(recipe), "--grader-id", "g-charter-judge", "--judge", "stub")
    assert res.returncode == 2
    assert "no completed annotations" in res.stderr


def test_below_min_sample_size_exits_one_but_writes_record(tmp_path: Path) -> None:
    recipe = _copy_charter_recipe(tmp_path)
    _seed_completed_annotations(recipe, n_pass=5, n_fail=0)

    res = _run(
        str(recipe),
        "--grader-id", "g-charter-judge",
        "--judge", "stub",
        "--min-sample-size", "20",
    )
    # Sample below threshold → not deployable → exit 1.
    assert res.returncode == 1
    log = recipe / "evals" / "calibration.jsonl"
    # Record is still written so the team can see why a calibration attempt
    # was blocked.
    assert log.exists()


def test_min_sample_size_override_lets_small_sample_deploy(tmp_path: Path) -> None:
    recipe = _copy_charter_recipe(tmp_path)
    _seed_completed_annotations(recipe, n_pass=5, n_fail=0)

    res = _run(
        str(recipe),
        "--grader-id", "g-charter-judge",
        "--judge", "stub",
        "--min-sample-size", "5",
    )
    assert res.returncode == 0
    assert "deployed:      True" in res.stdout


def test_max_divergence_override_widens(tmp_path: Path) -> None:
    recipe = _copy_charter_recipe(tmp_path)
    _seed_completed_annotations(recipe, n_pass=15, n_fail=5)  # divergence 0.25

    # Default 0.15 → not deployable.
    res_default = _run(str(recipe), "--grader-id", "g-charter-judge", "--judge", "stub")
    assert res_default.returncode == 1

    # Reset the log so the next run is clean.
    (recipe / "evals" / "calibration.jsonl").unlink()

    res_loose = _run(
        str(recipe),
        "--grader-id", "g-charter-judge",
        "--judge", "stub",
        "--max-divergence", "0.30",
    )
    assert res_loose.returncode == 0


# ---------- output formatting ----------


def test_summary_lists_skipped_annotations(tmp_path: Path) -> None:
    recipe = _copy_charter_recipe(tmp_path)
    # Mix of usable and unknown-SME annotations.
    _seed_completed_annotations(recipe, n_pass=22, n_fail=0)
    # Add three with verdict=unknown that should be skipped from agreement.
    store = AnnotationStore(recipe / "evals" / "annotations")
    for i in range(3):
        a = Annotation(
            annotation_id=f"r1-tex-unk-{i}",
            run_id=1,
            task_id="ex",
            trial_index=99 + i,
            grader_id="g-charter-sme-review",
            polarity="positive",
            tags=["capability"],
            axes={},
            task_input={"feature_brief": "x"},
            task_expected=None,
            recipe_output="## Structure",
            rubric_path="rubrics/judge_charter_quality.md",
            created_at="2026-05-08T12:00:00+00:00",
            status="completed",
            review=AnnotationReview(
                verdict="unknown",
                reviewer="N",
                reviewed_at="2026-05-08T13:00:00+00:00",
            ),
        )
        store.write(a)

    res = _run(str(recipe), "--grader-id", "g-charter-judge", "--judge", "stub")
    assert res.returncode == 0
    assert "skipped 3" in res.stdout
    assert "SME verdict is unknown" in res.stdout
