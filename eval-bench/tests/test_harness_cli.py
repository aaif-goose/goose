"""End-to-end smoke for run_kpass.py against the bundled recipe template
and the charter-sfdipot recipe."""

from __future__ import annotations

import sqlite3
import subprocess
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]
HARNESS = REPO_ROOT / "eval-bench" / "run_kpass.py"
TEMPLATE = REPO_ROOT / "recipes" / "_template"
CHARTER = REPO_ROOT / "recipes" / "test" / "charter-sfdipot"


def _run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(HARNESS), *args],
        capture_output=True,
        text=True,
        check=False,
    )


# ---------- basic CLI ----------


def test_help_runs() -> None:
    result = _run("--help")
    assert result.returncode == 0
    assert "Skein eval-bench harness" in result.stdout


def test_dry_run_against_template() -> None:
    if not TEMPLATE.exists():
        pytest.skip("recipes/_template not present in this checkout")
    result = _run("--recipe", str(TEMPLATE), "--dry-run")
    assert result.returncode == 0, f"stderr:\n{result.stderr}"
    out = result.stdout
    assert "tasks:" in out
    assert "graders:" in out
    assert "L3 g-judge-policy: skipped" in out
    assert "--dry-run set" in out


def test_dry_run_filters_by_tag() -> None:
    if not TEMPLATE.exists():
        pytest.skip("recipes/_template not present in this checkout")
    result = _run("--recipe", str(TEMPLATE), "--tag", "regression", "--dry-run")
    assert result.returncode != 0
    assert "no tasks match" in result.stderr


def test_missing_recipe_dir_errors_cleanly() -> None:
    result = _run("--recipe", "/nonexistent/path", "--dry-run")
    assert result.returncode != 0
    assert "no evals/" in result.stderr


# ---------- end-to-end: stub runner against charter-sfdipot ----------


def test_stub_runner_executes_pipeline_end_to_end(tmp_path: Path) -> None:
    """Run the harness with --runner stub against the real charter-sfdipot
    recipe. Verifies tasks load, runner returns output, L1 graders dispatch
    via subprocess, composition applies polarity inversion, and results
    persist to SQLite."""
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot recipe not present in this checkout")

    store = tmp_path / "results.sqlite"
    result = _run(
        "--recipe", str(CHARTER),
        "--k", "1",
        "--runner", "stub",
        "--store", str(store),
    )
    out = result.stdout

    # The stub produces a full SFDIPOT charter for every task. Negative tasks
    # (which expect refusal) fail; positive tasks pass. So pass^1 < target.
    # The harness exit code is 1 because pass^1 < min_passk_target=0.80.
    assert result.returncode == 1, f"unexpected:\nstdout={out}\nstderr={result.stderr}"
    assert "results" in out
    assert "pass@1" in out and "pass^1" in out
    assert "slice by domain:" in out
    # The vague / off-scope tasks must show as failing slices.
    assert "vague: pass@1 = 0.000" in out
    assert "off-scope: pass@1 = 0.000" in out
    # The positive-domain tasks must show as passing slices.
    assert "api: pass@1 = 1.000" in out

    # Results must have landed in SQLite.
    with sqlite3.connect(store) as conn:
        n_runs = conn.execute("SELECT COUNT(*) FROM run").fetchone()[0]
        n_trials = conn.execute("SELECT COUNT(*) FROM trial").fetchone()[0]
        finished = conn.execute("SELECT finished_at FROM run").fetchone()[0]
    assert n_runs == 1
    # 10 tasks × k=1 = 10 trials.
    assert n_trials == 10
    assert finished is not None


def test_stub_runner_records_per_grader_outcomes(tmp_path: Path) -> None:
    """The grader_scores JSON column persists per-grader outcomes including
    skipped status — needed by the Trace Inspector and Slice Explorer."""
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot recipe not present in this checkout")

    store = tmp_path / "results.sqlite"
    _run(
        "--recipe", str(CHARTER),
        "--k", "1",
        "--runner", "stub",
        "--store", str(store),
    )

    import json
    with sqlite3.connect(store) as conn:
        rows = conn.execute(
            "SELECT task_id, grader_scores_json FROM trial ORDER BY id"
        ).fetchall()
    assert rows, "expected at least one trial row"
    for task_id, scores_json in rows:
        scores = json.loads(scores_json)
        assert "outcomes" in scores
        outcome_ids = {o["id"] for o in scores["outcomes"]}
        # All four declared graders should appear, even when skipped.
        assert "g-output-shape" in outcome_ids
        assert "g-charter-sections" in outcome_ids
        assert "g-charter-sme-review" in outcome_ids
        assert "g-charter-judge" in outcome_ids


def test_stub_runner_polarity_inversion_applied_in_evidence(tmp_path: Path) -> None:
    """For negative-polarity tasks, the evidence string for g-charter-sections
    must show 'polarity-inverted' — proof that the composition layer ran."""
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot recipe not present in this checkout")

    store = tmp_path / "results.sqlite"
    _run(
        "--recipe", str(CHARTER),
        "--k", "1",
        "--runner", "stub",
        "--store", str(store),
    )

    import json
    with sqlite3.connect(store) as conn:
        # Find a negative-polarity trial (vague / empty / off-scope).
        rows = conn.execute(
            "SELECT task_id, grader_scores_json FROM trial WHERE polarity = 'negative'"
        ).fetchall()
    assert rows, "expected at least one negative-polarity trial"
    sample = json.loads(rows[0][1])
    sections_evidence = sample["evidence"]["g-charter-sections"]
    assert "polarity-inverted" in sections_evidence
    assert "raw=pass" in sections_evidence  # stub produces full charter = raw pass


def test_goose_runner_default_errors_when_binary_missing(tmp_path: Path) -> None:
    """Without overriding --runner, the default 'goose' runner is used.
    Without goose installed, every trial reports a runner error and the
    overall trial fails — but the harness completes cleanly."""
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot recipe not present in this checkout")

    store = tmp_path / "results.sqlite"
    # Use --tag regression to keep it small (only one task).
    result = _run(
        "--recipe", str(CHARTER),
        "--k", "1",
        "--tag", "regression",
        "--store", str(store),
    )
    # All trials fail because the runner errored — pass^1 = 0 < 0.80 → exit 1.
    assert result.returncode == 1
    assert "pass@1 = 0.000" in result.stdout


# ---------- L3 judge invocation through the harness ----------


def _copy_recipe_with_fresh_calibration(src: Path, dst: Path) -> Path:
    """Copy a recipe directory and inject a fresh green calibration log.

    Used to test the harness's L3 judge invocation path. Returns the new
    recipe directory path.
    """
    import shutil
    from datetime import datetime, timezone

    shutil.copytree(src, dst)
    cal = dst / "evals" / "calibration.jsonl"
    record = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "judge_id": "g-charter-judge",
        "judge_model": "anthropic:claude-opus-4-7",
        "sample_size": 50,
        "agreement": 0.95,
        "deployed": True,
    }
    import json
    cal.write_text(json.dumps(record) + "\n", encoding="utf-8")
    return dst


def test_l3_judge_stub_fires_when_calibration_is_fresh(tmp_path: Path) -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot recipe not present in this checkout")
    recipe_copy = _copy_recipe_with_fresh_calibration(CHARTER, tmp_path / "charter-copy")
    store = tmp_path / "results.sqlite"

    result = _run(
        "--recipe", str(recipe_copy),
        "--k", "1",
        "--runner", "stub",
        "--judge", "stub",
        "--store", str(store),
    )
    # Header must report the L3 grader as `ok` (calibration green).
    assert "L3 g-charter-judge: ok" in result.stdout

    # Inspect persisted outcomes: g-charter-judge must NOT be skipped on at
    # least some trials (the judge was actually invoked).
    import json
    import sqlite3
    with sqlite3.connect(store) as conn:
        rows = conn.execute("SELECT grader_scores_json FROM trial").fetchall()
    judge_outcomes = []
    for (scores_json,) in rows:
        scores = json.loads(scores_json)
        for o in scores["outcomes"]:
            if o["id"] == "g-charter-judge":
                judge_outcomes.append(o)
    assert judge_outcomes, "expected at least one trial with a g-charter-judge outcome"
    assert any(not o["skipped"] for o in judge_outcomes), (
        "stub judge should have fired on some trials and produced a non-skipped outcome"
    )


def test_l3_judge_off_disables_invocation_with_clear_skip_reason(tmp_path: Path) -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot recipe not present in this checkout")
    recipe_copy = _copy_recipe_with_fresh_calibration(CHARTER, tmp_path / "charter-copy")
    store = tmp_path / "results.sqlite"

    _run(
        "--recipe", str(recipe_copy),
        "--k", "1",
        "--runner", "stub",
        "--judge", "off",
        "--store", str(store),
    )

    import json
    import sqlite3
    with sqlite3.connect(store) as conn:
        (scores_json,) = conn.execute(
            "SELECT grader_scores_json FROM trial LIMIT 1"
        ).fetchone()
    scores = json.loads(scores_json)
    judge_outcome = next(o for o in scores["outcomes"] if o["id"] == "g-charter-judge")
    assert judge_outcome["skipped"] is True


def test_l3_judge_help_lists_choices() -> None:
    result = _run("--help")
    assert "--judge" in result.stdout
    assert "anthropic" in result.stdout
    assert "stub" in result.stdout
    assert "off" in result.stdout


# ---------- L2 sampling through the harness ----------


def _copy_recipe_with_l2_sample_rate(src: Path, dst: Path, *, sample_rate: float) -> Path:
    """Copy a recipe and override the L2 grader's sample_rate to the given
    value (typically 1.0 to force sampling on every trial in tests)."""
    import shutil

    import yaml

    shutil.copytree(src, dst)
    graders_path = dst / "evals" / "graders.yaml"
    data = yaml.safe_load(graders_path.read_text(encoding="utf-8"))
    for g in data["graders"]:
        if g.get("level") == "L2":
            g["sample_rate"] = sample_rate
    graders_path.write_text(yaml.safe_dump(data, sort_keys=False), encoding="utf-8")
    return dst


def test_l2_sampling_writes_annotation_files(tmp_path: Path) -> None:
    """With sample_rate=1.0, every trial must produce an annotation file in
    the recipe's evals/annotations/ directory."""
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot recipe not present in this checkout")
    recipe_copy = _copy_recipe_with_l2_sample_rate(
        CHARTER, tmp_path / "charter-copy", sample_rate=1.0,
    )
    annotations_dir = recipe_copy / "evals" / "annotations"
    # The .gitkeep file ships with the directory; remove it so we can count
    # only actual annotation files.
    for stale in annotations_dir.glob("*"):
        if stale.is_file():
            stale.unlink()

    store = tmp_path / "results.sqlite"
    _run(
        "--recipe", str(recipe_copy),
        "--k", "1",
        "--runner", "stub",
        "--judge", "off",
        "--store", str(store),
    )
    annotation_files = list(annotations_dir.glob("*.json"))
    # 10 tasks × 1 trial × 1 L2 grader at sample_rate=1.0 = 10 annotations.
    assert len(annotation_files) == 10


def test_l2_sampling_zero_rate_writes_no_annotations(tmp_path: Path) -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot recipe not present in this checkout")
    recipe_copy = _copy_recipe_with_l2_sample_rate(
        CHARTER, tmp_path / "charter-copy", sample_rate=0.0,
    )
    annotations_dir = recipe_copy / "evals" / "annotations"
    for stale in annotations_dir.glob("*"):
        if stale.is_file():
            stale.unlink()

    store = tmp_path / "results.sqlite"
    _run(
        "--recipe", str(recipe_copy),
        "--k", "1",
        "--runner", "stub",
        "--judge", "off",
        "--store", str(store),
    )
    annotation_files = list(annotations_dir.glob("*.json"))
    assert annotation_files == []


def test_l2_annotation_carries_task_and_output(tmp_path: Path) -> None:
    """The persisted annotation file must carry everything the SME needs:
    task input, expected, recipe output, rubric path."""
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot recipe not present in this checkout")
    recipe_copy = _copy_recipe_with_l2_sample_rate(
        CHARTER, tmp_path / "charter-copy", sample_rate=1.0,
    )
    annotations_dir = recipe_copy / "evals" / "annotations"
    for stale in annotations_dir.glob("*"):
        if stale.is_file():
            stale.unlink()

    store = tmp_path / "results.sqlite"
    _run(
        "--recipe", str(recipe_copy),
        "--k", "1",
        "--runner", "stub",
        "--judge", "off",
        "--store", str(store),
    )
    import json
    files = sorted(annotations_dir.glob("*.json"))
    assert files
    one = json.loads(files[0].read_text(encoding="utf-8"))
    assert one["status"] == "pending"
    assert one["review"] is None
    assert one["rubric_path"] == "rubrics/sme_quality.md"
    assert "feature_brief" in one["task_input"]
    assert one["recipe_output"]


def test_l2_sampling_skip_reason_reflects_queued_or_not_sampled(tmp_path: Path) -> None:
    """The L2 grader's outcome in SQLite must record either 'queued for
    review at <path>' (sampled) or 'not sampled (sample_rate=...)' so the
    Trace Inspector and Slice Explorer can show the L2 status."""
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot recipe not present in this checkout")
    recipe_copy = _copy_recipe_with_l2_sample_rate(
        CHARTER, tmp_path / "charter-copy", sample_rate=1.0,
    )
    annotations_dir = recipe_copy / "evals" / "annotations"
    for stale in annotations_dir.glob("*"):
        if stale.is_file():
            stale.unlink()

    store = tmp_path / "results.sqlite"
    _run(
        "--recipe", str(recipe_copy),
        "--k", "1",
        "--runner", "stub",
        "--judge", "off",
        "--store", str(store),
    )
    import json
    import sqlite3
    with sqlite3.connect(store) as conn:
        rows = conn.execute("SELECT grader_scores_json FROM trial").fetchall()
    found_queued = False
    for (scores_json,) in rows:
        scores = json.loads(scores_json)
        for o in scores["outcomes"]:
            if o["id"] == "g-charter-sme-review":
                assert o["skipped"] is True
                # The reason is in the evidence dict written by composition.
        evidence = scores["evidence"]
        if "queued for review at" in evidence.get("g-charter-sme-review", ""):
            found_queued = True
    assert found_queued, "expected at least one trial to show 'queued for review' for the L2 grader"
