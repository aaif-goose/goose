"""graders.yaml loader and the L1/L2/L3 ladder.

The harness composes graders into a pass/fail per task. L3 graders that lack
fresh calibration evidence are auto-skipped, not failed; the result row is
annotated so the slice explorer can show "judge skipped due to stale calibration."
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import yaml

from .calibration import CalibrationRecord, load_calibration_log


@dataclass
class Grader:
    id: str
    level: str  # "L1" | "L2" | "L3"
    type: str
    weight: float
    dimension: str | None
    negate_on_polarity_negative: bool = False
    """If True, the grader's raw verdict is inverted when applied to a task
    with polarity=negative. Set on shape-checking graders that should pass on
    positive tasks but fail on refusal/negative tasks (and vice versa)."""


@dataclass
class L1Grader(Grader):
    runner: str = ""
    timeout_s: int = 30


@dataclass
class L2Grader(Grader):
    sample_rate: float = 0.0
    queue: str = ""
    rubric: str | None = None


@dataclass
class L3Grader(Grader):
    judge_model: str = ""
    rubric: str = ""
    requires_calibration_within_days: int = 30
    max_divergence_from_l2: float = 0.15


@dataclass
class Graders:
    version: int
    min_passk_target: float | None
    graders: list[Grader]

    def by_level(self, level: str) -> list[Grader]:
        return [g for g in self.graders if g.level == level]


def load_graders(path: str | Path) -> Graders:
    path = Path(path)
    with path.open("r", encoding="utf-8") as f:
        data: dict[str, Any] = yaml.safe_load(f) or {}

    if data.get("version") != 1:
        raise ValueError(f"{path}: unsupported graders.yaml version: {data.get('version')!r}")

    raw_graders = data.get("graders", [])
    if not isinstance(raw_graders, list) or not raw_graders:
        raise ValueError(f"{path}: `graders` must be a non-empty list")

    graders = [_grader_from_dict(g, path) for g in raw_graders]
    _check_at_least_one_l1(graders, path)
    return Graders(
        version=1,
        min_passk_target=data.get("min_passk_target"),
        graders=graders,
    )


def is_l3_calibrated(
    grader: L3Grader,
    log_path: str | Path,
    *,
    now: datetime | None = None,
) -> tuple[bool, str]:
    """Return (deployable, reason) for an L3 grader based on the calibration log."""
    now = now or datetime.now(timezone.utc)
    try:
        records = load_calibration_log(log_path)
    except FileNotFoundError:
        return False, f"no calibration log at {log_path}"

    relevant = [r for r in records if r.judge_id == grader.id and r.judge_model == grader.judge_model]
    if not relevant:
        return False, f"no calibration record for judge_id={grader.id} judge_model={grader.judge_model}"

    latest = max(relevant, key=lambda r: r.timestamp)
    if not latest.deployed:
        return False, f"latest calibration was not deployable (agreement={latest.agreement:.2f})"

    age_days = (now - latest.timestamp).total_seconds() / 86400
    if age_days > grader.requires_calibration_within_days:
        return False, (
            f"latest calibration is {age_days:.1f} days old, "
            f"exceeds requires_calibration_within_days={grader.requires_calibration_within_days}"
        )

    divergence = 1.0 - latest.agreement
    if divergence > grader.max_divergence_from_l2:
        return False, (
            f"calibration divergence {divergence:.2f} exceeds "
            f"max_divergence_from_l2={grader.max_divergence_from_l2:.2f}"
        )

    return True, "ok"


def _grader_from_dict(d: dict[str, Any], path: Path) -> Grader:
    required = {"id", "level", "type", "weight"}
    missing = required - d.keys()
    if missing:
        raise ValueError(f"{path}: grader missing required fields: {sorted(missing)}")
    level = d["level"]
    common = dict(
        id=d["id"],
        level=level,
        type=d["type"],
        weight=float(d["weight"]),
        dimension=d.get("dimension"),
        negate_on_polarity_negative=bool(d.get("negate_on_polarity_negative", False)),
    )
    if level == "L1":
        if not d.get("runner"):
            raise ValueError(f"{path}: L1 grader {d['id']!r} requires `runner`")
        return L1Grader(
            **common,
            runner=d["runner"],
            timeout_s=int(d.get("timeout_s", 30)),
        )
    if level == "L2":
        return L2Grader(
            **common,
            sample_rate=float(d.get("sample_rate", 0.0)),
            queue=d.get("queue", "annotations/"),
            rubric=d.get("rubric"),
        )
    if level == "L3":
        for f in ("judge_model", "rubric", "requires_calibration_within_days"):
            if f not in d:
                raise ValueError(f"{path}: L3 grader {d['id']!r} requires `{f}`")
        return L3Grader(
            **common,
            judge_model=d["judge_model"],
            rubric=d["rubric"],
            requires_calibration_within_days=int(d["requires_calibration_within_days"]),
            max_divergence_from_l2=float(d.get("max_divergence_from_l2", 0.15)),
        )
    raise ValueError(f"{path}: grader {d['id']!r} has unknown level {level!r}")


def _check_at_least_one_l1(graders: list[Grader], path: Path) -> None:
    if not any(g.level == "L1" for g in graders):
        raise ValueError(
            f"{path}: graders.yaml must contain at least one L1 (code) grader. "
            "L3 (LLM-as-judge) without an L1 floor is not allowed; the floor of "
            "correctness must be deterministic."
        )
