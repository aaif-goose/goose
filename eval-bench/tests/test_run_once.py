"""Tests for run_once.py — single-shot ad-hoc recipe execution CLI."""

from __future__ import annotations

import json
import sqlite3
import subprocess
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]
RUN_ONCE = REPO_ROOT / "eval-bench" / "run_once.py"
CHARTER = REPO_ROOT / "recipes" / "test" / "charter-sfdipot"
ORACLES = REPO_ROOT / "recipes" / "test" / "oracles-fewhiccupps"


def _run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(RUN_ONCE), *args],
        capture_output=True,
        text=True,
        check=False,
    )


# ---------- help & basic shape ----------


def test_help_runs() -> None:
    res = _run("--help")
    assert res.returncode == 0
    assert "single-shot ad-hoc recipe execution" in res.stdout


def test_missing_recipe_yaml_errors_cleanly(tmp_path: Path) -> None:
    bare = tmp_path / "no-recipe"
    bare.mkdir()
    res = _run(str(bare), "--input", "feature_brief=x", "--runner", "stub")
    assert res.returncode == 2
    assert "no recipe.yaml" in res.stderr


# ---------- parameter parsing ----------


def test_input_and_inputs_are_mutually_exclusive() -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(
        str(CHARTER),
        "--input", "feature_brief=x",
        "--inputs", '{"feature_brief": "y"}',
        "--runner", "stub",
    )
    assert res.returncode == 2
    assert "mutually exclusive" in res.stderr


def test_input_key_value_form() -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(
        str(CHARTER),
        "--input", "feature_brief=Add a healthz endpoint",
        "--runner", "stub",
        "--no-graders",
    )
    assert res.returncode == 0
    assert "## Structure" in res.stdout  # stub output


def test_input_at_file_form_reads_file_contents(tmp_path: Path) -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    brief = tmp_path / "brief.md"
    brief.write_text(
        "Add a /healthz endpoint that returns 200 with {status, version, timestamp}.\n",
        encoding="utf-8",
    )
    res = _run(
        str(CHARTER),
        "--input", f"feature_brief=@{brief}",
        "--runner", "stub",
        "--no-graders",
    )
    assert res.returncode == 0


def test_input_at_file_missing_errors() -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(
        str(CHARTER),
        "--input", "feature_brief=@/nonexistent/file",
        "--runner", "stub",
    )
    assert res.returncode == 2
    assert "file not found" in res.stderr


def test_input_malformed_errors() -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(str(CHARTER), "--input", "no_equals_sign", "--runner", "stub")
    assert res.returncode == 2
    assert "KEY=VALUE" in res.stderr


def test_inputs_json_form() -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(
        str(CHARTER),
        "--inputs", '{"feature_brief": "Add /healthz"}',
        "--runner", "stub",
        "--no-graders",
    )
    assert res.returncode == 0


def test_inputs_invalid_json_errors() -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(str(CHARTER), "--inputs", "{not json}", "--runner", "stub")
    assert res.returncode == 2
    assert "valid JSON" in res.stderr


def test_inputs_must_be_object_not_array() -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(str(CHARTER), "--inputs", "[1,2,3]", "--runner", "stub")
    assert res.returncode == 2
    assert "must be a JSON object" in res.stderr


# ---------- recipe parameter validation ----------


def test_missing_required_parameter_errors() -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(str(CHARTER), "--runner", "stub")
    assert res.returncode == 2
    assert "feature_brief" in res.stderr
    assert "requires" in res.stderr


def test_unknown_parameter_warns_but_runs() -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(
        str(CHARTER),
        "--input", "feature_brief=x",
        "--input", "unknown_param=y",
        "--runner", "stub",
        "--no-graders",
    )
    assert res.returncode == 0
    assert "warning" in res.stderr
    assert "unknown_param" in res.stderr


# ---------- output formatting ----------


def test_default_output_includes_header_on_stderr() -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(
        str(CHARTER),
        "--input", "feature_brief=x",
        "--runner", "stub",
        "--no-graders",
    )
    # The recipe output goes to stdout; the header / runner+judge info goes to stderr.
    assert "## Structure" in res.stdout
    assert "# recipe:" in res.stderr
    assert "# runner: stub" in res.stderr


def test_output_only_suppresses_chrome() -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(
        str(CHARTER),
        "--input", "feature_brief=x",
        "--runner", "stub",
        "--no-graders",
        "--output-only",
    )
    assert "## Structure" in res.stdout
    assert "# recipe:" not in res.stderr  # no chrome on stderr either


def test_output_pipes_cleanly_to_a_file(tmp_path: Path) -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    out_path = tmp_path / "charter.md"
    with out_path.open("w") as f:
        completed = subprocess.run(
            [
                sys.executable, str(RUN_ONCE), str(CHARTER),
                "--input", "feature_brief=Add /healthz",
                "--runner", "stub",
                "--no-graders",
                "--output-only",
            ],
            stdout=f, stderr=subprocess.PIPE, text=True, check=False,
        )
    assert completed.returncode == 0
    body = out_path.read_text(encoding="utf-8")
    # No chrome leaked into the file.
    assert "# recipe:" not in body
    # The seven SFDIPOT sections are in the stub output.
    for s in ["Structure", "Function", "Time"]:
        assert f"## {s}" in body


# ---------- grading flow ----------


def test_full_charter_with_graders_passes() -> None:
    """Stub runner produces a full SFDIPOT charter; with --judge off, L1
    graders pass on a positive ad-hoc task."""
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(
        str(CHARTER),
        "--input", "feature_brief=Add /healthz",
        "--runner", "stub",
        "--judge", "off",
    )
    assert res.returncode == 0, f"stderr:\n{res.stderr}"
    assert "PASS" in res.stderr
    assert "# grading" in res.stderr


def test_no_graders_skips_grading_entirely() -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    res = _run(
        str(CHARTER),
        "--input", "feature_brief=x",
        "--runner", "stub",
        "--no-graders",
    )
    assert res.returncode == 0
    assert "# grading" not in res.stderr


# ---------- store flag ----------


def test_no_store_by_default_for_adhoc(tmp_path: Path) -> None:
    """Ad-hoc runs do not write to ~/.skein/eval-bench.sqlite by default."""
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    store = tmp_path / "should_not_appear.sqlite"
    res = _run(
        str(CHARTER),
        "--input", "feature_brief=x",
        "--runner", "stub",
        "--no-graders",
    )
    assert res.returncode == 0
    assert not store.exists()


def test_store_flag_persists_run(tmp_path: Path) -> None:
    if not CHARTER.exists():
        pytest.skip("charter-sfdipot not present")
    store = tmp_path / "results.sqlite"
    res = _run(
        str(CHARTER),
        "--input", "feature_brief=Add /healthz",
        "--runner", "stub",
        "--judge", "off",
        "--store", str(store),
    )
    assert res.returncode == 0
    assert store.exists()
    with sqlite3.connect(store) as conn:
        runs = conn.execute("SELECT recipe, k, notes FROM run").fetchall()
        trials = conn.execute("SELECT task_id, polarity FROM trial").fetchall()
    assert len(runs) == 1
    assert "run_once" in (runs[0][2] or "")
    assert trials == [("adhoc", "positive")]


# ---------- works against the second recipe too ----------


def test_runs_against_oracles_fewhiccupps() -> None:
    """The CLI is recipe-agnostic — it must work against any recipe with
    a recipe.yaml and an evals/graders.yaml."""
    if not ORACLES.exists():
        pytest.skip("oracles-fewhiccupps not present")
    res = _run(
        str(ORACLES),
        "--input", "target_description=function clamp(value, lo, hi) -> float",
        "--input", "target_kind=function",
        "--runner", "stub",
        "--no-graders",
    )
    assert res.returncode == 0
    # Stub output is the SFDIPOT placeholder regardless of recipe; assert
    # it ran rather than asserting recipe-specific output.
    assert "## Structure" in res.stdout
