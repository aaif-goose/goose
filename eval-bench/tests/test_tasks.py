"""Tests for tasks.jsonl loading and the two-sided eval discipline."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from lib.tasks import Task, load_tasks


def _write_jsonl(path: Path, records: list[dict]) -> Path:
    with path.open("w", encoding="utf-8") as f:
        for r in records:
            f.write(json.dumps(r) + "\n")
    return path


# ---------- Task.from_json ----------


def test_task_from_json_minimal() -> None:
    t = Task.from_json(
        {
            "id": "t1",
            "description": "x",
            "input": {"q": "hi"},
            "polarity": "positive",
            "tags": ["regression"],
        }
    )
    assert t.id == "t1"
    assert t.polarity == "positive"
    assert t.tags == ["regression"]
    assert t.axes == {}
    assert t.failure_modes_targeted == []


def test_task_from_json_rejects_missing_required() -> None:
    with pytest.raises(ValueError, match="missing required fields"):
        Task.from_json({"id": "t1", "description": "x", "input": {}, "polarity": "positive"})


def test_task_from_json_rejects_invalid_polarity() -> None:
    with pytest.raises(ValueError, match="invalid polarity"):
        Task.from_json(
            {
                "id": "t1",
                "description": "x",
                "input": {},
                "polarity": "neutral",
                "tags": ["regression"],
            }
        )


def test_task_from_json_rejects_empty_tags() -> None:
    with pytest.raises(ValueError, match="no tags"):
        Task.from_json(
            {
                "id": "t1",
                "description": "x",
                "input": {},
                "polarity": "positive",
                "tags": [],
            }
        )


# ---------- load_tasks: two-sided discipline ----------


def test_load_tasks_rejects_one_sided_suite(tmp_path: Path) -> None:
    """Suites with only positive polarity tasks are the one-sided eval anti-pattern."""
    p = _write_jsonl(
        tmp_path / "tasks.jsonl",
        [
            {"id": "t1", "description": "x", "input": {}, "polarity": "positive", "tags": ["c"]},
            {"id": "t2", "description": "y", "input": {}, "polarity": "positive", "tags": ["c"]},
        ],
    )
    with pytest.raises(ValueError, match="one-sided eval suites"):
        load_tasks(p)


def test_load_tasks_accepts_two_sided_suite(tmp_path: Path) -> None:
    p = _write_jsonl(
        tmp_path / "tasks.jsonl",
        [
            {
                "id": "t1",
                "description": "should fire",
                "input": {},
                "polarity": "positive",
                "tags": ["capability"],
            },
            {
                "id": "t2",
                "description": "should NOT fire",
                "input": {},
                "polarity": "negative",
                "tags": ["capability"],
            },
        ],
    )
    tasks = load_tasks(p)
    assert len(tasks) == 2
    assert {t.polarity for t in tasks} == {"positive", "negative"}


def test_load_tasks_rejects_only_negative_suite(tmp_path: Path) -> None:
    """Symmetry: an all-negative suite is also one-sided. The current rule
    catches all-positive (the common antipattern); we document the asymmetry
    here by asserting the current behaviour, so a future tightening to also
    reject all-negative is a deliberate and obvious change."""
    p = _write_jsonl(
        tmp_path / "tasks.jsonl",
        [
            {
                "id": "t1",
                "description": "x",
                "input": {},
                "polarity": "negative",
                "tags": ["capability"],
            },
        ],
    )
    # Current rule: rejects only the all-positive case; this loads.
    tasks = load_tasks(p)
    assert len(tasks) == 1


# ---------- load_tasks: structural ----------


def test_load_tasks_rejects_empty_file(tmp_path: Path) -> None:
    p = tmp_path / "tasks.jsonl"
    p.write_text("", encoding="utf-8")
    with pytest.raises(ValueError, match="empty"):
        load_tasks(p)


def test_load_tasks_rejects_duplicate_ids(tmp_path: Path) -> None:
    p = _write_jsonl(
        tmp_path / "tasks.jsonl",
        [
            {"id": "dup", "description": "x", "input": {}, "polarity": "positive", "tags": ["c"]},
            {"id": "dup", "description": "y", "input": {}, "polarity": "negative", "tags": ["c"]},
        ],
    )
    with pytest.raises(ValueError, match="duplicate task id"):
        load_tasks(p)


def test_load_tasks_skips_blank_and_comment_lines(tmp_path: Path) -> None:
    p = tmp_path / "tasks.jsonl"
    p.write_text(
        "\n"
        "# comment\n"
        '{"id": "t1", "description": "x", "input": {}, "polarity": "positive", "tags": ["c"]}\n'
        "\n"
        '{"id": "t2", "description": "y", "input": {}, "polarity": "negative", "tags": ["c"]}\n',
        encoding="utf-8",
    )
    tasks = load_tasks(p)
    assert [t.id for t in tasks] == ["t1", "t2"]


def test_load_tasks_reports_line_number_on_invalid_json(tmp_path: Path) -> None:
    p = tmp_path / "tasks.jsonl"
    p.write_text(
        '{"id": "t1", "description": "x", "input": {}, "polarity": "positive", "tags": ["c"]}\n'
        "this is not json\n",
        encoding="utf-8",
    )
    with pytest.raises(ValueError, match=":2 invalid JSON"):
        load_tasks(p)


def test_load_tasks_round_trip_all_fields(tmp_path: Path) -> None:
    p = _write_jsonl(
        tmp_path / "tasks.jsonl",
        [
            {
                "id": "rich",
                "description": "fully populated task",
                "input": {"q": "what?"},
                "expected": {"contains": "answer"},
                "axes": {"model": "opus", "complexity": "low"},
                "polarity": "positive",
                "failure_modes_targeted": ["fm-001"],
                "tags": ["regression"],
                "min_passk_target": 0.9,
                "source": {"kind": "production_trace", "ref": "tr-abc", "first_observed": "2026-05-07"},
            },
            {
                "id": "neg",
                "description": "neg",
                "input": {},
                "polarity": "negative",
                "tags": ["capability"],
            },
        ],
    )
    tasks = load_tasks(p)
    assert tasks[0].axes == {"model": "opus", "complexity": "low"}
    assert tasks[0].min_passk_target == 0.9
    assert tasks[0].source is not None
    assert tasks[0].source["kind"] == "production_trace"
    assert tasks[0].failure_modes_targeted == ["fm-001"]
