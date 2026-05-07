"""Tests for the SQLite results store."""

from __future__ import annotations

import sqlite3
from pathlib import Path

from lib.store import ResultsStore


def test_init_creates_schema(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    with sqlite3.connect(store.path) as conn:
        tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()}
    assert {"run", "trial"}.issubset(tables)


def test_record_run_and_trials(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    run_id = store.start_run(recipe="recipes/test/example", k=3, notes="smoke")
    for i, passed in enumerate([True, True, False]):
        store.record_trial(
            run_id=run_id,
            task_id="t1",
            trial_index=i,
            passed=passed,
            polarity="positive",
            tags=["regression"],
            axes={"model": "opus", "complexity": "low"},
            grader_scores={"g-l1": 1.0},
            duration_ms=42,
            trace_id=f"tr-{i}",
        )
    store.finish_run(run_id)

    with sqlite3.connect(store.path) as conn:
        rows = conn.execute(
            "SELECT task_id, trial_index, passed, axes_json, trace_id FROM trial WHERE run_id = ? ORDER BY trial_index",
            (run_id,),
        ).fetchall()
        runs = conn.execute("SELECT recipe, k, finished_at FROM run WHERE id = ?", (run_id,)).fetchall()

    assert len(rows) == 3
    assert [r[2] for r in rows] == [1, 1, 0]  # passed booleans persisted as 0/1
    assert [r[4] for r in rows] == ["tr-0", "tr-1", "tr-2"]
    assert runs[0][0] == "recipes/test/example"
    assert runs[0][1] == 3
    assert runs[0][2] is not None  # finish_run wrote a timestamp


def test_default_path_can_be_overridden(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "subdir" / "results.sqlite")
    assert store.path.exists()
    assert store.path.parent == tmp_path / "subdir"


def test_two_separate_runs_share_one_store(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    r1 = store.start_run(recipe="r1", k=1)
    r2 = store.start_run(recipe="r2", k=1)
    assert r1 != r2
    store.record_trial(
        run_id=r1, task_id="a", trial_index=0, passed=True,
        polarity="positive", tags=["c"], axes={}, grader_scores={},
    )
    store.record_trial(
        run_id=r2, task_id="b", trial_index=0, passed=False,
        polarity="negative", tags=["c"], axes={}, grader_scores={},
    )

    with sqlite3.connect(store.path) as conn:
        count_r1 = conn.execute("SELECT COUNT(*) FROM trial WHERE run_id = ?", (r1,)).fetchone()[0]
        count_r2 = conn.execute("SELECT COUNT(*) FROM trial WHERE run_id = ?", (r2,)).fetchone()[0]
    assert count_r1 == 1
    assert count_r2 == 1
