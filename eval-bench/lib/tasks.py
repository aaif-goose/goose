"""tasks.jsonl loader and validator.

Enforces the two-sided eval discipline (Anthropic): a recipe whose tasks are 100%
positive polarity is rejected at load time as a one-sided eval suite.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class Task:
    id: str
    description: str
    input: dict[str, Any]
    polarity: str  # "positive" | "negative"
    tags: list[str]
    expected: Any = None
    axes: dict[str, str | int | float | bool] = field(default_factory=dict)
    failure_modes_targeted: list[str] = field(default_factory=list)
    min_passk_target: float | None = None
    source: dict[str, Any] | None = None

    @classmethod
    def from_json(cls, obj: dict[str, Any]) -> "Task":
        required = {"id", "description", "input", "polarity", "tags"}
        missing = required - obj.keys()
        if missing:
            raise ValueError(f"task missing required fields: {sorted(missing)}")
        if obj["polarity"] not in {"positive", "negative"}:
            raise ValueError(f"task {obj['id']!r} has invalid polarity {obj['polarity']!r}")
        if not obj["tags"]:
            raise ValueError(f"task {obj['id']!r} has no tags; must include 'regression' or 'capability'")
        return cls(
            id=obj["id"],
            description=obj["description"],
            input=obj["input"],
            polarity=obj["polarity"],
            tags=list(obj["tags"]),
            expected=obj.get("expected"),
            axes=dict(obj.get("axes", {})),
            failure_modes_targeted=list(obj.get("failure_modes_targeted", [])),
            min_passk_target=obj.get("min_passk_target"),
            source=obj.get("source"),
        )


def load_tasks(path: str | Path) -> list[Task]:
    """Load and validate tasks.jsonl.

    Validates each task against the schema (structurally; full JSON Schema
    validation is performed by the harness when the `jsonschema` package is
    available). Raises on the one-sided eval anti-pattern.
    """
    path = Path(path)
    tasks: list[Task] = []
    with path.open("r", encoding="utf-8") as f:
        for line_num, raw in enumerate(f, start=1):
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError as e:
                raise ValueError(f"{path}:{line_num} invalid JSON: {e}") from e
            tasks.append(Task.from_json(obj))

    _check_two_sided(tasks, path)
    _check_unique_ids(tasks, path)
    return tasks


def _check_two_sided(tasks: list[Task], path: Path) -> None:
    if not tasks:
        raise ValueError(f"{path}: tasks.jsonl is empty")
    polarities = {t.polarity for t in tasks}
    if polarities == {"positive"}:
        raise ValueError(
            f"{path}: tasks file contains only positive tasks. "
            "Skein refuses one-sided eval suites — for every 'behaviour should fire' "
            "task, ship at least one 'behaviour should NOT fire' counterpart."
        )


def _check_unique_ids(tasks: list[Task], path: Path) -> None:
    seen: set[str] = set()
    for t in tasks:
        if t.id in seen:
            raise ValueError(f"{path}: duplicate task id {t.id!r}")
        seen.add(t.id)
