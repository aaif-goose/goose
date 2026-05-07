"""L2 annotations: sampled-human-review queue.

When an L2 grader's `sample_rate` fires for a trial, the harness writes a
JSON annotation file to the recipe's `evals/annotations/` directory. The
file carries everything an SME needs to review the trial: the original
input, the recipe output, the rubric path, plus an empty `review` field.

A small `annotate.py` CLI (separate file) lets the SME work the queue:
list pending annotations, show one, complete it with a verdict and
notes. Completed annotations later feed L3 judge calibration.

This module is the data layer: file format, deterministic sampling, and
read/write helpers. It does not invoke graders or judges.
"""

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ANNOTATION_VERSION = 1
VALID_STATUSES = {"pending", "completed", "discarded"}
VALID_VERDICTS = {"pass", "fail", "unknown"}


@dataclass
class AnnotationReview:
    """The SME's verdict on one annotation."""

    verdict: str  # "pass" | "fail" | "unknown"
    reviewer: str
    reviewed_at: str
    notes: str = ""
    scores: dict[str, int] = field(default_factory=dict)
    """Per-dimension integer scores, e.g. {"honesty": 2, "tactics": 1}.
    Dimensions and scoring scale are recipe-specific (see the recipe's
    rubric markdown). The harness does not interpret these values; they
    are kept verbatim for downstream calibration tooling."""

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "AnnotationReview":
        verdict = str(d.get("verdict", ""))
        if verdict not in VALID_VERDICTS:
            raise ValueError(
                f"verdict must be one of {sorted(VALID_VERDICTS)}; got {verdict!r}"
            )
        return cls(
            verdict=verdict,
            reviewer=str(d.get("reviewer") or ""),
            reviewed_at=str(d.get("reviewed_at") or ""),
            notes=str(d.get("notes") or ""),
            scores=dict(d.get("scores") or {}),
        )


@dataclass
class Annotation:
    """One trial sampled for human review."""

    annotation_id: str
    run_id: int
    task_id: str
    trial_index: int
    grader_id: str
    polarity: str
    tags: list[str]
    axes: dict[str, Any]
    task_input: dict[str, Any]
    task_expected: Any
    recipe_output: str
    rubric_path: str
    created_at: str
    status: str = "pending"
    review: AnnotationReview | None = None
    version: int = ANNOTATION_VERSION

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "Annotation":
        version = d.get("version", 1)
        if version != ANNOTATION_VERSION:
            raise ValueError(
                f"unsupported annotation version {version!r}; expected {ANNOTATION_VERSION}"
            )
        status = str(d.get("status", "pending"))
        if status not in VALID_STATUSES:
            raise ValueError(
                f"status must be one of {sorted(VALID_STATUSES)}; got {status!r}"
            )
        review_obj = d.get("review")
        review = AnnotationReview.from_dict(review_obj) if review_obj else None
        return cls(
            annotation_id=str(d["annotation_id"]),
            run_id=int(d["run_id"]),
            task_id=str(d["task_id"]),
            trial_index=int(d["trial_index"]),
            grader_id=str(d["grader_id"]),
            polarity=str(d["polarity"]),
            tags=list(d.get("tags") or []),
            axes=dict(d.get("axes") or {}),
            task_input=dict(d.get("task_input") or {}),
            task_expected=d.get("task_expected"),
            recipe_output=str(d.get("recipe_output") or ""),
            rubric_path=str(d.get("rubric_path") or ""),
            created_at=str(d.get("created_at") or ""),
            status=status,
            review=review,
            version=version,
        )

    def to_dict(self) -> dict[str, Any]:
        d = asdict(self)
        if self.review is None:
            d["review"] = None
        return d


# ---------- Deterministic sampling ----------


def should_sample(
    *,
    run_id: int,
    task_id: str,
    trial_index: int,
    grader_id: str,
    sample_rate: float,
) -> bool:
    """Decide whether to sample this trial for the given L2 grader.

    Deterministic in (run_id, task_id, trial_index, grader_id, sample_rate)
    so re-running the same run id reproduces the same sampling decisions.
    Implemented via SHA256 of the joined key, mapped to [0, 1).

    Edge cases:
      sample_rate <= 0 → never sample
      sample_rate >= 1 → always sample
    """
    if sample_rate <= 0:
        return False
    if sample_rate >= 1:
        return True
    key = f"{run_id}|{task_id}|{trial_index}|{grader_id}".encode("utf-8")
    digest = hashlib.sha256(key).digest()
    # First 8 bytes as an unsigned int, mapped to [0, 1).
    n = int.from_bytes(digest[:8], "big") / (1 << 64)
    return n < sample_rate


def make_annotation_id(
    *, run_id: int, task_id: str, trial_index: int, grader_id: str
) -> str:
    """Filesystem-safe deterministic annotation id.

    Format: r<run>-t<task>-i<trial>-g<grader>. Non-alphanumeric chars in
    task_id and grader_id are replaced with `_` so the id is safe to use
    as a filename on every platform.
    """
    safe_task = re.sub(r"[^A-Za-z0-9_-]", "_", task_id)
    safe_grader = re.sub(r"[^A-Za-z0-9_-]", "_", grader_id)
    return f"r{run_id}-t{safe_task}-i{trial_index}-g{safe_grader}"


# ---------- AnnotationStore: a directory of JSON files ----------


class AnnotationStore:
    """Directory-backed annotation queue.

    Layout:
        <directory>/<annotation_id>.json
    """

    def __init__(self, directory: Path) -> None:
        self.directory = directory
        self.directory.mkdir(parents=True, exist_ok=True)

    def path_for(self, annotation_id: str) -> Path:
        return self.directory / f"{annotation_id}.json"

    def write(self, annotation: Annotation) -> Path:
        path = self.path_for(annotation.annotation_id)
        with path.open("w", encoding="utf-8") as f:
            json.dump(annotation.to_dict(), f, indent=2, sort_keys=True)
            f.write("\n")
        return path

    def read(self, annotation_id: str) -> Annotation:
        path = self.path_for(annotation_id)
        return self._read_path(path)

    def _read_path(self, path: Path) -> Annotation:
        with path.open("r", encoding="utf-8") as f:
            data = json.load(f)
        return Annotation.from_dict(data)

    def list_all(self) -> list[Annotation]:
        out: list[Annotation] = []
        for path in sorted(self.directory.glob("*.json")):
            out.append(self._read_path(path))
        return out

    def list_pending(self) -> list[Annotation]:
        return [a for a in self.list_all() if a.status == "pending"]

    def list_completed(self) -> list[Annotation]:
        return [a for a in self.list_all() if a.status == "completed"]

    def complete(
        self,
        annotation_id: str,
        *,
        verdict: str,
        reviewer: str,
        notes: str = "",
        scores: dict[str, int] | None = None,
    ) -> Annotation:
        """Mark an annotation as completed with the SME's verdict."""
        if verdict not in VALID_VERDICTS:
            raise ValueError(
                f"verdict must be one of {sorted(VALID_VERDICTS)}; got {verdict!r}"
            )
        annotation = self.read(annotation_id)
        if annotation.status == "completed":
            raise ValueError(f"annotation {annotation_id!r} is already completed")
        annotation.status = "completed"
        annotation.review = AnnotationReview(
            verdict=verdict,
            reviewer=reviewer,
            reviewed_at=datetime.now(timezone.utc).isoformat(),
            notes=notes,
            scores=dict(scores or {}),
        )
        self.write(annotation)
        return annotation

    def discard(self, annotation_id: str, *, reason: str) -> Annotation:
        """Mark an annotation as discarded — removed from the active queue
        without becoming a calibration data point. Reason is recorded in
        the review.notes field."""
        annotation = self.read(annotation_id)
        if annotation.status != "pending":
            raise ValueError(
                f"can only discard pending annotations; "
                f"{annotation_id!r} is {annotation.status}"
            )
        annotation.status = "discarded"
        annotation.review = AnnotationReview(
            verdict="unknown",
            reviewer="<discarded>",
            reviewed_at=datetime.now(timezone.utc).isoformat(),
            notes=reason,
        )
        self.write(annotation)
        return annotation


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()
