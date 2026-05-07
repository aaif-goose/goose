"""Tests for the slices.py CLI — runs / show / compare against a fixture store."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

from lib.store import ResultsStore

REPO_ROOT = Path(__file__).resolve().parents[2]
SLICES_CLI = REPO_ROOT / "eval-bench" / "slices.py"


def _seed_run(
    store: ResultsStore,
    *,
    recipe: str,
    k: int,
    trials: list[tuple[str, int, bool, dict]],
) -> int:
    run_id = store.start_run(recipe=recipe, k=k, notes="fixture")
    for task_id, idx, passed, axes in trials:
        store.record_trial(
            run_id=run_id,
            task_id=task_id,
            trial_index=idx,
            passed=passed,
            polarity="positive",
            tags=["capability"],
            axes=axes,
            grader_scores={"outcomes": []},
        )
    store.finish_run(run_id)
    return run_id


def _run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(SLICES_CLI), *args],
        capture_output=True,
        text=True,
        check=False,
    )


def _store_path(tmp_path: Path) -> Path:
    return tmp_path / "results.sqlite"


# ---------- top-level CLI ----------


def test_help_runs() -> None:
    res = _run("--help")
    assert res.returncode == 0
    assert "runs" in res.stdout and "show" in res.stdout and "compare" in res.stdout


def test_missing_store_errors_cleanly(tmp_path: Path) -> None:
    res = _run("--store", str(tmp_path / "no.sqlite"), "runs")
    assert res.returncode == 2
    assert "does not exist" in res.stderr


# ---------- runs ----------


def test_runs_empty_store(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    ResultsStore(path=store_path)  # creates the schema, no runs
    res = _run("--store", str(store_path), "runs")
    assert res.returncode == 0
    assert "no runs in store" in res.stdout


def test_runs_lists_recent_first_with_passk(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    store = ResultsStore(path=store_path)
    a = _seed_run(store, recipe="r/a", k=1, trials=[("t1", 0, True, {})])
    b = _seed_run(store, recipe="r/b", k=1, trials=[("t1", 0, False, {})])
    res = _run("--store", str(store_path), "runs")
    assert res.returncode == 0
    out = res.stdout
    # Newest first.
    assert out.find(f"{b}") < out.find(f"{a}")
    # Headline pass^k present.
    assert "PASS^K" in out
    assert "0.000" in out  # b had a failing trial
    assert "1.000" in out  # a passed


def test_runs_recipe_filter(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    store = ResultsStore(path=store_path)
    _seed_run(store, recipe="r/a", k=1, trials=[("t1", 0, True, {})])
    _seed_run(store, recipe="r/b", k=1, trials=[("t1", 0, True, {})])
    res = _run("--store", str(store_path), "runs", "--recipe", "r/a")
    assert res.returncode == 0
    assert "r/a" in res.stdout
    assert "r/b" not in res.stdout


def test_runs_limit_flag(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    store = ResultsStore(path=store_path)
    for _ in range(5):
        _seed_run(store, recipe="r/x", k=1, trials=[("t1", 0, True, {})])
    res = _run("--store", str(store_path), "runs", "--limit", "2")
    assert res.returncode == 0
    # Header line + 2 rows = 3 visible rows.
    body_lines = [line for line in res.stdout.splitlines() if line.strip()]
    assert len(body_lines) == 3


# ---------- show ----------


def test_show_unknown_run_id_errors(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    ResultsStore(path=store_path)
    res = _run("--store", str(store_path), "show", "999")
    assert res.returncode == 2
    assert "not found" in res.stderr


def test_show_prints_header_and_overall_passk(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    store = ResultsStore(path=store_path)
    rid = _seed_run(
        store,
        recipe="recipes/test/charter-sfdipot",
        k=2,
        trials=[
            ("t1", 0, True, {"complexity": "low"}),
            ("t1", 1, True, {"complexity": "low"}),
            ("t2", 0, False, {"complexity": "high"}),
            ("t2", 1, True, {"complexity": "high"}),
        ],
    )
    res = _run("--store", str(store_path), "show", str(rid))
    assert res.returncode == 0
    out = res.stdout
    assert f"run {rid}" in out
    assert "recipes/test/charter-sfdipot" in out
    assert "k:" in out
    assert "trials:" in out
    assert "overall:" in out
    assert "pass@2" in out and "pass^2" in out


def test_show_breaks_down_by_recorded_axes(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    store = ResultsStore(path=store_path)
    rid = _seed_run(
        store,
        recipe="r/x",
        k=1,
        trials=[
            ("t1", 0, True, {"complexity": "low", "domain": "api"}),
            ("t2", 0, False, {"complexity": "low", "domain": "ui"}),
            ("t3", 0, True, {"complexity": "high", "domain": "api"}),
        ],
    )
    res = _run("--store", str(store_path), "show", str(rid))
    assert res.returncode == 0
    out = res.stdout
    # Both axes should appear, sorted alphabetically.
    assert "slice by complexity:" in out
    assert "slice by domain:" in out


def test_show_axis_filter(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    store = ResultsStore(path=store_path)
    rid = _seed_run(
        store,
        recipe="r/x",
        k=1,
        trials=[
            ("t1", 0, True, {"complexity": "low", "domain": "api"}),
            ("t2", 0, False, {"complexity": "high", "domain": "ui"}),
        ],
    )
    res = _run("--store", str(store_path), "show", str(rid), "--axis", "complexity")
    assert res.returncode == 0
    assert "slice by complexity:" in res.stdout
    assert "slice by domain:" not in res.stdout


def test_show_run_with_no_trials(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    store = ResultsStore(path=store_path)
    rid = store.start_run(recipe="r/x", k=1)
    store.finish_run(rid)
    res = _run("--store", str(store_path), "show", str(rid))
    assert res.returncode == 0
    assert "no trials recorded" in res.stdout


# ---------- compare ----------


def test_compare_prints_overall_and_per_axis_deltas(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    store = ResultsStore(path=store_path)
    a = _seed_run(
        store,
        recipe="r/x",
        k=1,
        trials=[
            ("t1", 0, True, {"complexity": "low"}),
            ("t2", 0, False, {"complexity": "high"}),
        ],
    )
    b = _seed_run(
        store,
        recipe="r/x",
        k=1,
        trials=[
            ("t1", 0, True, {"complexity": "low"}),
            ("t2", 0, True, {"complexity": "high"}),  # improved
        ],
    )
    res = _run("--store", str(store_path), "compare", str(a), str(b))
    assert res.returncode == 0
    out = res.stdout
    assert f"compare run {a} → run {b}" in out
    assert "overall:" in out
    assert "Δ=" in out
    assert "slice by complexity:" in out
    # `high` slice should show improvement marker.
    assert "↑" in out


def test_compare_warns_on_different_recipes(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    store = ResultsStore(path=store_path)
    a = _seed_run(store, recipe="r/a", k=1, trials=[("t1", 0, True, {"x": "y"})])
    b = _seed_run(store, recipe="r/b", k=1, trials=[("t1", 0, True, {"x": "y"})])
    res = _run("--store", str(store_path), "compare", str(a), str(b))
    assert res.returncode == 0
    assert "different recipes" in res.stderr


def test_compare_warns_on_different_k(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    store = ResultsStore(path=store_path)
    a = _seed_run(store, recipe="r/x", k=1, trials=[("t1", 0, True, {"x": "y"})])
    b = _seed_run(
        store, recipe="r/x", k=3,
        trials=[("t1", 0, True, {"x": "y"}), ("t1", 1, True, {"x": "y"}), ("t1", 2, True, {"x": "y"})],
    )
    res = _run("--store", str(store_path), "compare", str(a), str(b))
    assert res.returncode == 0
    assert "different k" in res.stderr


def test_compare_unknown_run_id_errors(tmp_path: Path) -> None:
    store_path = _store_path(tmp_path)
    ResultsStore(path=store_path)
    res = _run("--store", str(store_path), "compare", "1", "999")
    assert res.returncode == 2


def test_compare_marks_new_or_gone_slice_values(tmp_path: Path) -> None:
    """If a slice value exists in only one of the two runs, the compare
    output must say so explicitly rather than silently dropping it."""
    store_path = _store_path(tmp_path)
    store = ResultsStore(path=store_path)
    a = _seed_run(
        store, recipe="r/x", k=1,
        trials=[
            ("t1", 0, True, {"complexity": "low"}),
            ("t2", 0, True, {"complexity": "medium"}),
        ],
    )
    b = _seed_run(
        store, recipe="r/x", k=1,
        trials=[
            ("t1", 0, True, {"complexity": "low"}),
            ("t3", 0, True, {"complexity": "high"}),  # new value, no medium
        ],
    )
    res = _run("--store", str(store_path), "compare", str(a), str(b))
    assert res.returncode == 0
    out = res.stdout
    assert "(new in b)" in out
    assert "(gone in b)" in out
