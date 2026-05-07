"""End-to-end smoke for run_kpass.py against the bundled recipe template."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]
HARNESS = REPO_ROOT / "eval-bench" / "run_kpass.py"
TEMPLATE = REPO_ROOT / "recipes" / "_template"


def _run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(HARNESS), *args],
        capture_output=True,
        text=True,
        check=False,
    )


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
    # The template's L3 grader has no calibration, so it must be reported skipped.
    assert "L3 g-judge-policy: skipped" in out
    assert "--dry-run set" in out


def test_dry_run_filters_by_tag() -> None:
    if not TEMPLATE.exists():
        pytest.skip("recipes/_template not present in this checkout")
    result = _run("--recipe", str(TEMPLATE), "--tag", "regression", "--dry-run")
    # Template ships only `capability` tasks; --tag regression must exit non-zero.
    assert result.returncode != 0
    assert "no tasks match" in result.stderr


def test_missing_recipe_dir_errors_cleanly() -> None:
    result = _run("--recipe", "/nonexistent/path", "--dry-run")
    assert result.returncode != 0
    assert "no evals/" in result.stderr
