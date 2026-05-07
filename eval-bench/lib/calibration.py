"""Calibration log reader / writer.

Each L3 (LLM-as-judge) grader is calibrated periodically against L2 (human)
ratings. Records are appended to a JSONL log alongside calibration.md inside
a recipe's evals/ directory; calibration.md is the human-readable narrative,
calibration.jsonl is what the harness machine-reads.
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any


@dataclass
class CalibrationRecord:
    timestamp: datetime
    judge_id: str
    judge_model: str
    sample_size: int
    agreement: float
    deployed: bool
    cohen_kappa: float | None = None
    divergence_breakdown: dict[str, float] = field(default_factory=dict)
    notes: str | None = None

    @classmethod
    def from_json(cls, obj: dict[str, Any]) -> "CalibrationRecord":
        ts = obj["timestamp"]
        if isinstance(ts, str):
            ts = datetime.fromisoformat(ts.replace("Z", "+00:00"))
        return cls(
            timestamp=ts,
            judge_id=obj["judge_id"],
            judge_model=obj.get("judge_model", ""),
            sample_size=int(obj["sample_size"]),
            agreement=float(obj["agreement"]),
            deployed=bool(obj["deployed"]),
            cohen_kappa=obj.get("cohen_kappa"),
            divergence_breakdown=dict(obj.get("divergence_breakdown", {})),
            notes=obj.get("notes"),
        )

    def to_json(self) -> dict[str, Any]:
        d = asdict(self)
        d["timestamp"] = self.timestamp.isoformat()
        return d


def load_calibration_log(path: str | Path) -> list[CalibrationRecord]:
    """Load a calibration JSONL log. Missing file raises FileNotFoundError."""
    path = Path(path)
    with path.open("r", encoding="utf-8") as f:
        records: list[CalibrationRecord] = []
        for line_num, raw in enumerate(f, start=1):
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError as e:
                raise ValueError(f"{path}:{line_num} invalid JSON: {e}") from e
            records.append(CalibrationRecord.from_json(obj))
    return records


def append_calibration_record(path: str | Path, record: CalibrationRecord) -> None:
    """Append a calibration record. Creates the file if it does not exist."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as f:
        f.write(json.dumps(record.to_json()))
        f.write("\n")
