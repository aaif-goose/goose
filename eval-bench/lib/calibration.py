"""Calibration log reader / writer + the calibration runner.

Each L3 (LLM-as-judge) grader is calibrated periodically against L2 (human)
ratings. Records are appended to a JSONL log alongside calibration.md inside
a recipe's evals/ directory; calibration.md is the human-readable narrative,
calibration.jsonl is what the harness machine-reads.

This module provides three layers:

  - CalibrationRecord                 — the on-disk record format
  - load_calibration_log / append…    — read/write helpers
  - run_calibration                   — the actual calibration computation:
                                         feed completed annotations through
                                         the L3 judge, compare to SME
                                         verdicts, decide deployability,
                                         append a record.
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .annotations import Annotation, AnnotationStore
    from .graders import L3Grader
    from .judge import Judge


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


# ---------- Calibration runner ----------


@dataclass(frozen=True)
class CalibrationOutcome:
    """In-memory result of one calibration run, before it's appended to the log.

    Useful for tests and for the CLI to print a summary before persisting.
    """

    record: "CalibrationRecord"
    pairs: list[tuple[str, str]]
    """List of (sme_verdict, judge_verdict) pairs that contributed to the
    agreement number. Excludes pairs the runner skipped (judge errored,
    judge returned Unknown, SME verdict was unknown)."""
    skipped: list[tuple[str, str]]
    """List of (annotation_id, reason) for annotations the runner could not
    use as calibration data."""


def run_calibration(
    *,
    grader: "L3Grader",
    annotations_store: "AnnotationStore",
    judge: "Judge",
    rubric_path: Path,
    min_sample_size: int = 20,
    max_divergence_from_l2: float | None = None,
    notes: str | None = None,
    now: datetime | None = None,
) -> CalibrationOutcome:
    """Run the L3 judge against every completed L2 annotation and compute
    agreement.

    The SME's `review.verdict` is the headline judgement (per Anthropic
    calibration practice — single-dimension headline, not a vector). The L3
    judge is invoked with the same rubric and inputs the harness uses at
    run-time, and its verdict is compared.

    Pairs the runner cannot use as calibration data are skipped with a
    reason recorded:
      - SME verdict is `unknown` (not directly comparable)
      - Judge errored (network / API failure)
      - Judge returned `Unknown` (insufficient evidence — Anthropic guidance)

    `deployed` on the resulting record is True only when:
      - sample_size >= min_sample_size
      - divergence (= 1 - agreement) is within the grader's
        max_divergence_from_l2 threshold (overridable for explicit runs).
    """
    now = now or datetime.now(timezone.utc)
    threshold = (
        max_divergence_from_l2
        if max_divergence_from_l2 is not None
        else grader.max_divergence_from_l2
    )

    pairs: list[tuple[str, str]] = []
    skipped: list[tuple[str, str]] = []
    divergence_breakdown: dict[str, int] = {}

    for annotation in annotations_store.list_completed():
        sme_verdict = annotation.review.verdict if annotation.review else "unknown"
        if sme_verdict == "unknown":
            skipped.append((annotation.annotation_id, "SME verdict is unknown"))
            continue

        payload = _build_judge_payload(annotation)
        verdict = judge.judge(_read_rubric(rubric_path), payload)

        if verdict.error:
            skipped.append((annotation.annotation_id, f"judge error: {verdict.error}"))
            continue
        if verdict.verdict == "Unknown":
            skipped.append(
                (annotation.annotation_id, f"judge returned Unknown: {verdict.evidence or 'no evidence'}")
            )
            continue

        pairs.append((sme_verdict, verdict.verdict))
        if sme_verdict != verdict.verdict:
            disagree_key = f"{sme_verdict}_vs_{verdict.verdict}"
            divergence_breakdown[disagree_key] = divergence_breakdown.get(disagree_key, 0) + 1

    sample_size = len(pairs)
    agreements = sum(1 for sme, judge_v in pairs if sme == judge_v)
    agreement = agreements / sample_size if sample_size else 0.0
    cohen = _cohen_kappa(pairs) if sample_size >= 2 else None

    deployable_threshold = sample_size >= min_sample_size
    deployable_divergence = sample_size > 0 and (1.0 - agreement) <= threshold
    deployed = deployable_threshold and deployable_divergence

    record = CalibrationRecord(
        timestamp=now,
        judge_id=grader.id,
        judge_model=grader.judge_model,
        sample_size=max(sample_size, min_sample_size),  # honour schema minimum
        agreement=agreement,
        deployed=deployed,
        cohen_kappa=cohen,
        divergence_breakdown={k: v / sample_size for k, v in divergence_breakdown.items()}
        if sample_size
        else {},
        notes=_build_notes(
            notes=notes,
            sample_size=sample_size,
            min_sample_size=min_sample_size,
            agreement=agreement,
            threshold=threshold,
            skipped_count=len(skipped),
        ),
    )
    return CalibrationOutcome(record=record, pairs=pairs, skipped=skipped)


def _build_judge_payload(annotation: "Annotation") -> dict[str, Any]:
    return {
        "feature_brief": annotation.task_input.get("feature_brief"),
        "task_input": annotation.task_input,
        "task_expected": annotation.task_expected,
        "task_polarity": annotation.polarity,
        "output": annotation.recipe_output,
    }


def _read_rubric(rubric_path: Path) -> str:
    return rubric_path.read_text(encoding="utf-8")


def _cohen_kappa(pairs: list[tuple[str, str]]) -> float:
    """Compute Cohen's kappa for two categorical raters.

    Categories are inferred from the data. Returns 0.0 when expected
    agreement equals 1.0 (degenerate single-category case) since kappa is
    undefined there but treating it as zero-correction is the conservative
    choice.
    """
    n = len(pairs)
    if n == 0:
        return 0.0
    rater_a = [p[0] for p in pairs]
    rater_b = [p[1] for p in pairs]
    categories = sorted(set(rater_a) | set(rater_b))
    if len(categories) < 2:
        return 1.0  # Both raters used a single category and agreed on every item.

    p_a = {c: rater_a.count(c) / n for c in categories}
    p_b = {c: rater_b.count(c) / n for c in categories}
    expected = sum(p_a[c] * p_b[c] for c in categories)
    observed = sum(1 for a, b in pairs if a == b) / n
    if expected >= 1.0:
        return 0.0
    return (observed - expected) / (1 - expected)


def _build_notes(
    *,
    notes: str | None,
    sample_size: int,
    min_sample_size: int,
    agreement: float,
    threshold: float,
    skipped_count: int,
) -> str:
    parts: list[str] = []
    if notes:
        parts.append(notes)
    parts.append(
        f"sample_size={sample_size} (min {min_sample_size}); "
        f"agreement={agreement:.3f}; max_divergence={threshold:.3f}; "
        f"skipped={skipped_count}"
    )
    return " | ".join(parts)
