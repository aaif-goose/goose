"""failure-modes.yaml loader.

Every recipe ships a living failure-mode taxonomy (Hamel Husain). Modes start
as `proposed` (agent-suggested), promote to `active` after human review, and
are eventually `retired` if they stop being observed.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import yaml


@dataclass
class FailureMode:
    id: str
    name: str
    description: str
    first_observed: str  # ISO date
    status: str  # "proposed" | "active" | "retired"
    last_seen: str | None = None
    example_trace_ids: list[str] = field(default_factory=list)
    severity: str | None = None  # "high" | "medium" | "low"
    approved_by: str | None = None
    related_aio_defects: list[str] = field(default_factory=list)


@dataclass
class FailureModes:
    version: int
    modes: list[FailureMode]

    def by_id(self, id_: str) -> FailureMode | None:
        for m in self.modes:
            if m.id == id_:
                return m
        return None

    def active(self) -> list[FailureMode]:
        return [m for m in self.modes if m.status == "active"]


def load_failure_modes(path: str | Path) -> FailureModes:
    path = Path(path)
    with path.open("r", encoding="utf-8") as f:
        data: dict[str, Any] = yaml.safe_load(f) or {}

    if data.get("version") != 1:
        raise ValueError(f"{path}: unsupported failure-modes.yaml version: {data.get('version')!r}")

    raw_modes = data.get("modes", [])
    if not isinstance(raw_modes, list):
        raise ValueError(f"{path}: `modes` must be a list")

    modes = [_mode_from_dict(m, path) for m in raw_modes]
    _check_unique_ids(modes, path)
    return FailureModes(version=1, modes=modes)


def _mode_from_dict(d: dict[str, Any], path: Path) -> FailureMode:
    required = {"id", "name", "description", "first_observed", "status"}
    missing = required - d.keys()
    if missing:
        raise ValueError(f"{path}: failure mode missing required fields: {sorted(missing)}")
    if d["status"] not in {"proposed", "active", "retired"}:
        raise ValueError(f"{path}: failure mode {d['id']!r} has invalid status {d['status']!r}")
    if d["status"] == "active" and not d.get("approved_by"):
        raise ValueError(
            f"{path}: failure mode {d['id']!r} is `active` but has no `approved_by`. "
            "Active modes require a human reviewer on record."
        )
    return FailureMode(
        id=d["id"],
        name=d["name"],
        description=d["description"],
        first_observed=d["first_observed"],
        status=d["status"],
        last_seen=d.get("last_seen"),
        example_trace_ids=list(d.get("example_trace_ids", [])),
        severity=d.get("severity"),
        approved_by=d.get("approved_by"),
        related_aio_defects=list(d.get("related_aio_defects", [])),
    )


def _check_unique_ids(modes: list[FailureMode], path: Path) -> None:
    seen: set[str] = set()
    for m in modes:
        if m.id in seen:
            raise ValueError(f"{path}: duplicate failure mode id {m.id!r}")
        seen.add(m.id)
