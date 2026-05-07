"""Tests for the store read methods used by the slices CLI."""

from __future__ import annotations

from pathlib import Path

import pytest

from lib.store import ResultsStore, RunRow, TrialRow


def _seed(store: ResultsStore, *, recipe: str, k: int, trials: list[tuple[str, int, bool, dict]]) -> int:
    """Helper: start a run, record the given trials, finish it. Returns run_id.

    Each trial is (task_id, trial_index, passed, axes).
    """
    run_id = store.start_run(recipe=recipe, k=k, notes="test seed")
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


# ---------- list_runs ----------


def test_list_runs_empty_store(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    assert store.list_runs() == []


def test_list_runs_returns_newest_first(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    a = _seed(store, recipe="r/a", k=1, trials=[("t1", 0, True, {})])
    b = _seed(store, recipe="r/b", k=1, trials=[("t1", 0, False, {})])
    c = _seed(store, recipe="r/c", k=1, trials=[("t1", 0, True, {})])
    rows = store.list_runs()
    assert [r.id for r in rows] == [c, b, a]


def test_list_runs_includes_trial_count(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    rid = _seed(
        store,
        recipe="r/x",
        k=2,
        trials=[("t1", 0, True, {}), ("t1", 1, True, {}), ("t2", 0, False, {})],
    )
    rows = store.list_runs()
    [row] = [r for r in rows if r.id == rid]
    assert row.n_trials == 3
    assert row.k == 2


def test_list_runs_recipe_filter(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    _seed(store, recipe="r/a", k=1, trials=[("t1", 0, True, {})])
    _seed(store, recipe="r/b", k=1, trials=[("t1", 0, True, {})])
    _seed(store, recipe="r/a", k=1, trials=[("t1", 0, True, {})])
    rows = store.list_runs(recipe="r/a")
    assert all(r.recipe == "r/a" for r in rows)
    assert len(rows) == 2


def test_list_runs_limit(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    for _ in range(5):
        _seed(store, recipe="r/x", k=1, trials=[("t1", 0, True, {})])
    rows = store.list_runs(limit=3)
    assert len(rows) == 3


# ---------- get_run ----------


def test_get_run_returns_run_with_trial_count(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    rid = _seed(
        store,
        recipe="r/x",
        k=2,
        trials=[("t1", 0, True, {}), ("t1", 1, False, {})],
    )
    run = store.get_run(rid)
    assert isinstance(run, RunRow)
    assert run.id == rid
    assert run.n_trials == 2


def test_get_run_returns_none_for_missing_id(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    assert store.get_run(999999) is None


def test_get_run_unfinished_run_has_none_finished_at(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    rid = store.start_run(recipe="r/x", k=1)
    # Don't finish.
    run = store.get_run(rid)
    assert run is not None
    assert run.finished_at is None


# ---------- get_trials ----------


def test_get_trials_orders_by_task_then_trial_index(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    rid = _seed(
        store,
        recipe="r/x",
        k=3,
        trials=[
            ("t2", 1, True, {"complexity": "low"}),
            ("t1", 0, False, {"complexity": "low"}),
            ("t1", 2, True, {"complexity": "low"}),
            ("t2", 0, True, {"complexity": "high"}),
            ("t1", 1, False, {"complexity": "low"}),
        ],
    )
    trials = store.get_trials(rid)
    keys = [(t.task_id, t.trial_index) for t in trials]
    assert keys == [("t1", 0), ("t1", 1), ("t1", 2), ("t2", 0), ("t2", 1)]


def test_get_trials_deserialises_axes(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    rid = _seed(
        store,
        recipe="r/x",
        k=1,
        trials=[("t1", 0, True, {"complexity": "high", "domain": "api"})],
    )
    [t] = store.get_trials(rid)
    assert t.axes == {"complexity": "high", "domain": "api"}


def test_get_trials_deserialises_grader_scores_and_tags(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    run_id = store.start_run(recipe="r/x", k=1)
    store.record_trial(
        run_id=run_id,
        task_id="t1",
        trial_index=0,
        passed=False,
        polarity="negative",
        tags=["capability", "regression"],
        axes={},
        grader_scores={
            "evidence": {"g-foo": "passed"},
            "outcomes": [{"id": "g-foo", "passed": True, "skipped": False, "score": 1.0}],
        },
        duration_ms=42,
        trace_id="tr-xyz",
    )
    [t] = store.get_trials(run_id)
    assert isinstance(t, TrialRow)
    assert t.tags == ["capability", "regression"]
    assert t.passed is False
    assert t.polarity == "negative"
    assert t.duration_ms == 42
    assert t.trace_id == "tr-xyz"
    assert t.grader_scores["outcomes"][0]["id"] == "g-foo"


def test_get_trials_empty_for_missing_run(tmp_path: Path) -> None:
    store = ResultsStore(path=tmp_path / "results.sqlite")
    assert store.get_trials(999) == []
